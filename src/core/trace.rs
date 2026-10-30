//! 运行时 trace 收集(本地 NDJSON)
//!
//! 复刻 LangSmith Trace 的 5 元组 (ts, session_id, step, input, output, duration_ms, extra),
//! 写到 `~/.local/share/x_videos-dowloads-trace/YYYY-MM-DD/<session_id>.ndjson`。
//!
//! 设计原则:
//! - 100% 本地,零外部依赖(无 LangSmith / 无 TokenPilot / 无网络上传)
//! - append-only NDJSON,grep/jq 友好
//! - thread-safe + 异步安全(`tokio::sync::Mutex`)
//! - 旁路注入,不影响函数签名 / 返回值
//!
//! 参考 docs/dna/trace-schema.md 的 5 元组 schema + 4 故障点规范。

use std::sync::LazyLock;
use std::time::Instant;

use chrono::Utc;
use serde_json::{json, Value};

/// 埋点 schema 版本 —— **改埋点语义时必须 +1**。
///
/// 为什么需要它(2026-10-30):本轮把 `resolver_try` 的 duration 从「写死 0」改成
/// marker + `resolver_try_done` 携带真实耗时,并新增了一批 `cli_*` 步骤。
/// 这意味着**旧 baseline 与新 current 的 stall_count 口径不同** ——
/// 旧 baseline 里 resolver 永远不可能被判成 stall(阈值 30s,duration 恒 0),
/// 新 current 里会。同一个 delta 既可能是「性能变差」也可能是「测得更准」,
/// 光看数字分不出来。
///
/// 之前这条规则只写在文档里(`docs/dna/trace-eval-metrics.md`),**没有任何东西检查它**
/// ⇒ 下一次拿旧 baseline 比新 trace,stop-trace-eval hook 会报一个假的「退化」。
/// 现在版本号写进每个 session 的事件里,`eval_trace.py` 版本不一致就**拒绝出判决**。
///
/// 版本号语义:整数,单调递增;只有在「同名字段的含义或口径变了」时才 +1
/// (加新步骤、加可选字段 **不用** +1 —— 那不影响已有指标的计算)。
pub const TRACE_SCHEMA_VERSION: u32 = 2;

/// 一个 trace session 的运行时状态:session_id + **它自己的** NDJSON 文件句柄。
struct TraceSessionState {
    session_id: String,
    /// 文件打开失败时为 None(此时该 session 的事件走 no-session fallback)
    file: Option<std::fs::File>,
    /// 打开时刻 —— 用来算 session 的墙钟总耗时(写进 `session_close.output.total_ms_proxy`)。
    opened_at: Instant,
}

/// 全局 session **栈**(2026-10-30 从「单个全局 SESSION_ID + TRACE_FILE」改成栈)。
///
/// **为什么改**:旧实现只有**一个**全局 session_id + 文件句柄,新开 session 直接覆盖。
/// 而 session 是会嵌套的 —— `batch_download_impl` 开一个 "batch" session,内部每个
/// item 的 `start_download_impl` 又各开一个 "gui-single" session,后者把前者的
/// SESSION_ID / TRACE_FILE 直接顶掉。后果:
///   · batch 的 `session_close` 被写进某个 item 的文件(事件误归档)
///   · batch 自己的文件只剩 `session_open` → `eval_trace.py` 判「有 open 无 close」
///     → 整个 batch session 被当「中断」丢弃,指标分母残缺
///   · 嵌套 session 的文件被覆盖时,先前写进去的事件还在,但 session_id 已经对不上
///
/// **改成栈之后**:每个 session 保有自己的文件;`trace_event` 写给**最内层**(当前)
/// session;每个 session 的 `session_close` 写回**它自己**的文件(按 id 找回,
/// 不是 pop —— 嵌套 close 顺序未必是栈序)。
///
/// 用 std::sync::Mutex + std::fs::File 同步写,确保 RAII Drop 可直接落盘,事件顺序严格匹配代码顺序。
static SESSIONS: LazyLock<std::sync::Mutex<Vec<TraceSessionState>>> =
    LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

/// 访问全局 session 栈。
///
/// poison-tolerant(`unwrap_or_else(|e| e.into_inner())`):trace 是**旁路**埋点,
/// 一次 panic 不应该让后续所有 trace 永久失效(旧实现对 SESSION_ID 用 `expect` 会直接 panic)。
fn with_sessions<R>(f: impl FnOnce(&mut Vec<TraceSessionState>) -> R) -> R {
    let mut guard = SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

/// 当前(**最内层**)session 的 id;栈空时返回空串(事件走 no-session fallback)。
fn current_session_id() -> String {
    with_sessions(|sessions| {
        sessions
            .last()
            .map(|st| st.session_id.clone())
            .unwrap_or_default()
    })
}

/// 指定 session 是否真的拿到了文件句柄(file open 失败 → false)。
///
/// 用途:`trace_session_open_guarded` 判 `opened`。**必须按 id 查自己的**,
/// 不能看「栈里有没有任何文件」——嵌套时外层 session 仍开着,那样会误判。
fn session_has_file(session_id: &str) -> bool {
    with_sessions(|sessions| {
        sessions
            .iter()
            .any(|st| st.session_id == session_id && st.file.is_some())
    })
}

/// 指定 session 的打开时刻 —— 用来算墙钟总耗时。
///
/// session 已从栈里移除(已关闭)时返回 None ⇒ **不写** `total_ms_proxy`
/// (宁可标「没测到」,也不要写一个假的 0 —— 这正是 H2「不可测 ≠ 零值」)。
fn session_opened_at(session_id: &str) -> Option<Instant> {
    with_sessions(|sessions| {
        sessions
            .iter()
            .find(|st| st.session_id == session_id)
            .map(|st| st.opened_at)
    })
}

/// trace 根目录(可被环境变量覆盖,便于测试用 tmpdir)
fn trace_root() -> std::path::PathBuf {
    if let Ok(custom) = std::env::var("X_VIDEOS_TRACE_DIR") {
        return std::path::PathBuf::from(custom);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("x_videos-dowloads-trace")
}

/// 生成 ULID-like 26 字符 session_id(时间戳 + 随机后缀)
fn generate_session_id(prefix: &str) -> String {
    let ts_ms = Utc::now().timestamp_millis();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // 简单 hash mix — 26 字符,首 10 位时间,后 16 位随机
    let random_part: String = (0..16)
        .map(|i| {
            let b = ((nanos >> (i % 32)) ^ (ts_ms as u32 >> (i % 16))) as u8;
            let chars = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ"; // Crockford base32
            chars[(b % 32) as usize] as char
        })
        .collect();
    format!("{}{}", prefix, random_part)
}

/// 打开一个 trace session,产 session_id 并打开 NDJSON 文件
///
/// `prefix` 建议:`batch` / `single` / `cli-{command}` / `gui-{action}`
///
/// OCR P1 #7 修复:旧实现无条件加 `YYYY-MM-DD/` 子目录,导致同日多测试共享同一
/// date_dir(都是 `2026-09-22/`),用 `read_dir().next()` 取文件不确定拿谁的。
/// 改:当 `X_VIDEOS_TRACE_DIR` env 显式设置时(测试场景),直接用该目录作 file_path
/// 父目录 — 测试自己保证目录唯一(已经 `timestamp_nanos_opt` 做后缀),不再二次
/// 折叠到日期子目录。线上场景(无 env)行为不变。
pub async fn trace_session_open(prefix: &str) -> String {
    let session_id = generate_session_id(prefix);

    let file_dir = if std::env::var("X_VIDEOS_TRACE_DIR").is_ok() {
        // 测试路径:env 已指向唯一 tmpdir,直接用,不再加日期子目录
        trace_root()
    } else {
        // 线上路径:按日期分目录,便于人肉浏览
        trace_root().join(Utc::now().format("%Y-%m-%d").to_string())
    };
    if let Err(e) = std::fs::create_dir_all(&file_dir) {
        eprintln!("[trace] 创建目录失败 {file_dir:?}: {e}");
        return session_id;
    }

    let file_path = file_dir.join(format!("{session_id}.ndjson"));
    // 每次 open 时 truncate(测试场景必全新;线上场景 session_id 是 ULID 永不重复)
    match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&file_path)
    {
        Ok(f) => {
            // **压栈**(2026-10-30):旧实现直接覆盖全局 SESSION_ID / TRACE_FILE,
            // 嵌套 session 会把外层 session 的文件句柄顶掉 → 外层事件被误写进内层文件。
            with_sessions(|sessions| {
                sessions.push(TraceSessionState {
                    session_id: session_id.clone(),
                    file: Some(f),
                    opened_at: Instant::now(),
                })
            });

            // 写入 session meta 作为 NDJSON 第一行(special step="session_open")
            // 写给**自己**的文件(此时它就是最内层,但显式按 id 写更抗嵌套)
            //
            // `extra.schema_version`:埋点 schema 版本(2026-10-30 新增)。
            // `eval_trace.py` 靠它拒绝「旧埋点 baseline vs 新埋点 current」的无效对比。
            let meta = json!({
                "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
                "session_id": session_id,
                "step": "session_open",
                "input": {},
                "output": {"file_path": file_path.to_string_lossy()},
                "duration_ms": 0u64,
                "extra": {"prefix": prefix, "schema_version": TRACE_SCHEMA_VERSION}
            });
            let _ = write_event_to(&session_id, &meta);
        }
        Err(e) => {
            eprintln!("[trace] 打开 trace 文件失败 {file_path:?}: {e}");
        }
    }

    session_id
}

/// 关闭**当前(最内层)** trace session(写一行 `session_close` 事件 + 从栈里移除)
///
/// OCR HIGH #6 修复:旧实现连续 lock 两个 Mutex 各两次(SESSION_ID + TRACE_FILE),
/// IO(flush) 发生在两次 lock 之间,任一 lock 中毒都会 unwrap() panic。
/// 改法—先拷数据释放锁,IO 在锁外,各自 lock 一次清空,用 unwrap_or_else
/// 兼容 mutex poison 不再 panic(panic 不该从 close 路径冒出来)。
///
/// 2026-10-30:改为 session **栈**后,「关掉谁」必须显式:
/// - [`trace_session_close`] 关最内层(本函数)
/// - [`close_session`] 按 session_id 关指定的那个(RAII guard 用它)
pub async fn trace_session_close(summary: Value) {
    let session_id = current_session_id();
    close_session(&session_id, summary).await;
}

/// 关闭**指定** session:把 `session_close` 写进**它自己**的文件,然后从栈里移除。
///
/// 2026-10-30 新增(嵌套 session 修复的核心):旧实现只有一个全局文件句柄,
/// close 事件被写进「当前」文件 —— 嵌套 session 场景下,batch 的 session_close
/// 会落进某个 item 的 session 文件,batch 自己的文件只剩 session_open,
/// `eval_trace.py` 判「有 open 无 close」→ 整个 batch session 被丢弃。
///
/// 按 id 移除而不是 pop:嵌套 session 的 close 顺序未必是栈序(并发时更不是)。
async fn close_session(session_id: &str, summary: Value) {
    let mut summary = summary;
    inject_total_ms(&mut summary, session_id);
    let close_event = json!({
        "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        "session_id": session_id,
        "step": "session_close",
        "input": {},
        "output": summary,
        "duration_ms": 0u64,
        "extra": {"schema_version": TRACE_SCHEMA_VERSION}
    });
    let _ = write_event_to(session_id, &close_event);

    // 从栈里按 id 移除(IO 已在锁外,各自独立 lock + poison-tolerant)
    with_sessions(|sessions| sessions.retain(|st| st.session_id != session_id));
}

/// 把 session 的**墙钟总耗时**写进 `output.total_ms_proxy`。
///
/// 口径(2026-10-30 定义,此前这个 key 从没被写过 ⇒ `avg_total_ms` 是死指标):
/// **session_open → session_close 的真实经过时间**,含全部等待。
///
/// 为什么不用「各叶子步骤 duration 之和」:那会把**没有埋点的等待**漏掉 ——
/// 而「转圈但没有速度」恰恰就是那部分等待(解析阶段的固定开销)。
/// 叶子之和会系统性低报总耗时,拿它当「这次下载花了多久」是错的。
///
/// session 已从栈里移除(重复 close / 非本进程打开)→ **不写**这个 key:
/// 宁可让 `eval_trace.py` 显示「⚪ 不可测」,也不写一个假的 0(H2)。
fn inject_total_ms(summary: &mut Value, session_id: &str) {
    let Some(opened_at) = session_opened_at(session_id) else {
        return;
    };
    let total_ms = opened_at.elapsed().as_millis() as u64;
    if summary.is_object() {
        summary["total_ms_proxy"] = json!(total_ms);
    } else {
        // summary 不是对象(如 json!(null))→ 包一层,别把调用方的字段弄丢
        let mut wrapper = json!({ "summary": summary.clone() });
        wrapper["total_ms_proxy"] = json!(total_ms);
        *summary = wrapper;
    }
}

/// RAII guard:保证 session 一定被关闭 —— 覆盖所有 `?` / early return / panic 展开路径。
///
/// 与 `TraceStep` 对称。显式 [`TraceSession::close`] 写业务 summary;
/// 未显式关闭则 drop 兜底写 `status="dropped"`。
///
/// OCR HIGH #6 修复:`opened` 字段记录 session 是否真打开(file open 失败时
/// trace_session_open 会提前 return session_id 但不留文件句柄,旧实现
/// Drop 仍然写 "session_close / dropped",误导为"已开过再被 drop"。改:
/// open 失败时 opened=false,Drop 直接放空不写任何事件。
pub struct TraceSession {
    closed: bool,
    opened: bool,
    /// 本 session 自己的 id —— close 时按 id 找回**自己的**文件(2026-10-30)。
    /// 旧实现没有这个字段:close 事件写给「当前(最内层)」session,嵌套时张冠李戴。
    session_id: String,
}

impl TraceSession {
    /// 显式关闭并写 summary;幂等(重复调用无副作用)。
    pub async fn close(mut self, summary: Value) {
        self.closed = true;
        if self.opened {
            // 自己就是最内层 → 走公共的「关最内层」入口(单一实现路径);
            // 不是最内层(嵌套乱序关闭)→ 按 id 关自己,免得关错人。
            if self.session_id == current_session_id() {
                trace_session_close(summary).await;
            } else {
                close_session(&self.session_id, summary).await;
            }
        }
    }
}

impl Drop for TraceSession {
    fn drop(&mut self) {
        if self.closed || !self.opened {
            return;
        }
        // 兜底:未显式 close 就退出(early return / 错误传播 / panic 展开)。
        //
        // OCR HIGH #6 修复:旧实现先 lock SESSION_ID 构建 event,再调 write_event
        // (内部 lock TRACE_FILE + 同步 flush),最后再分别 lock 两次清空 globals。
        // 结果:全程持锁跨同步 IO,任何并发 trace_event 都被 stall。改法—
        // 1. 先拷 session_id 后立即释放,
        // 2. 再写事件(锁内只此一次),
        // 3. 最后从 session 栈里按 id 移除(独立临界区,无嵌套)。
        // **诚实说明(2026-10-30 更正)**:这句注释在旧实现里就不准确 ——
        // 写 + flush 一直在 `write_event_to` 的锁内(与旧实现一致,同一 session 内
        // 的事件顺序严格匹配代码顺序)。改掉的是**锁的次数**:旧实现 4 次
        // lock/unlock(其中 2 次跨 IO),现在 2 次(1 次跨 IO)。
        // 想要「IO 完全在锁外」需要重构写路径(按 session 分桶 + 每桶独立锁),
        // 本轮没做 —— 现状是**吞吐**问题,不是**正确性**问题。
        let session_id = self.session_id.clone();
        let mut output = json!({
            "status": "dropped",
            "reason": "session guard dropped without explicit close (early return / error / panic)"
        });
        inject_total_ms(&mut output, &session_id);
        let event = json!({
            "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "session_id": session_id,
            "step": "session_close",
            "input": {},
            "output": output,
            "duration_ms": 0u64,
            "extra": {"implicit": true, "schema_version": TRACE_SCHEMA_VERSION}
        });
        // write_event_to 自己内部短暂 lock,flush 在锁内但只此一次。
        let _ = write_event_to(&session_id, &event);
        // 从栈里按 id 移除(独立临界区)。
        with_sessions(|sessions| sessions.retain(|st| st.session_id != session_id));
    }
}

/// 与 [`trace_session_open`] 相同,但返回 RAII guard。
///
/// **新代码请用这个**:它保证任何早退路径都会留下 `session_close` 事件。
///
/// OCR HIGH #6 修复:旧实现无条件返回 `closed: false`,Drop 永远写 "session_close /
/// dropped",即使 file open 失败(此时实际没 session)。改:跟踪 `opened` 标志,
/// Drop 见 `!opened` 直接放空(globals 也保持 None,无副作用)。
pub async fn trace_session_open_guarded(prefix: &str) -> TraceSession {
    let session_id = trace_session_open(prefix).await;
    // session_id 是 ULID-like 格式(trace_session_open 总会返),但文件句柄可能没
    // 成功设上(目录创建失败 / file open Err)。
    // **按自己的 id 查**(2026-10-30):旧实现查全局「有没有任何文件」,
    // 嵌套场景下外层 session 的文件会让这个判断永远为 true —— 即使本 session
    // 根本没打开文件,Drop 也会写一条假的 session_close。
    let opened = session_has_file(&session_id);
    TraceSession {
        closed: false,
        opened,
        session_id,
    }
}

/// 写入单个 trace 事件(5 元组 + extra)
pub async fn trace_event(step: &str, input: Value, output: Value, duration_ms: u64, extra: Value) {
    let event = json!({
        "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        "session_id": current_session_id(),
        "step": step,
        "input": input,
        "output": output,
        "duration_ms": duration_ms,
        "extra": extra
    });
    let _ = write_event(&event);
}

/// 内部:同步写 NDJSON 一行(append + flush)——写给**当前(最内层)**session 的文件。
fn write_event(event: &Value) -> std::io::Result<()> {
    let session_id = current_session_id();
    write_event_to(&session_id, event)
}

/// 内部:同步写 NDJSON 一行到**指定** session 的文件。
///
/// 该 session 不存在(还没开 / 已关闭 / 文件打开失败)→ 退回 no-session fallback。
///
/// 持锁期间写 + flush:保证同一 session 内的事件顺序严格匹配代码顺序
/// (旧实现也是持锁写,语义一致)。
fn write_event_to(session_id: &str, event: &Value) -> std::io::Result<()> {
    use std::io::Write;
    let mut sessions = SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(st) = sessions.iter_mut().find(|st| st.session_id == session_id) {
        if let Some(f) = st.file.as_mut() {
            let mut line = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
            line.push('\n');
            f.write_all(line.as_bytes())?;
            f.flush()?;
            return Ok(());
        }
    }
    // 无活动 session:fallback best-effort append 到 no-session/<date>.ndjson
    // 失败静默,不阻塞主流程,避免 stderr 刷屏
    let _ = append_no_session_event(event);
    Ok(())
}

/// Fallback:无活动 session 时把事件写到 `<trace_root>/no-session/YYYY-MM-DD.ndjson`
/// 与正常 session NDJSON 隔离,session_id 为空(便于 grep / 过滤)。
/// 任何 IO 错误均返回 Err,调用方静默丢弃。
fn append_no_session_event(event: &Value) -> std::io::Result<()> {
    use std::io::Write;
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let path = trace_root()
        .join("no-session")
        .join(format!("{date}.ndjson"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let mut line = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    f.write_all(line.as_bytes())?;
    f.flush()?;
    Ok(())
}

/// RAII guard:自动在 drop 时算 duration_ms 并写 trace_event
///
/// 用法:
/// ```ignore
/// let _trace = TraceStep::new("resolver_try")
///     .input(json!({"url": url}))
///     .on_exit(|out| { out["status"] = json!("ok"); });
/// // drop 时自动写 trace_event
/// ```
type OutputMutatorFn = Box<dyn FnOnce(&mut Value) + Send>;

pub struct TraceStep {
    step: String,
    input: Value,
    extra: Value,
    output_mutator: Option<OutputMutatorFn>,
    started_at: Instant,
    /// 构造时当前 session 的 id(2026-10-30)。
    ///
    /// **为什么要记**:旧实现在 drop 时才查「当前 session」—— 嵌套场景下
    /// (batch → gui-single)drop 时最内层可能已经换人了,事件会落进别人的文件。
    /// 在构造时绑定,保证「开始」和「结束」落在同一个文件里。
    session_id: String,
}

impl TraceStep {
    pub fn new(step: impl Into<String>) -> Self {
        Self {
            step: step.into(),
            input: json!({}),
            extra: json!({}),
            output_mutator: None,
            started_at: Instant::now(),
            session_id: current_session_id(),
        }
    }

    pub fn input(mut self, v: Value) -> Self {
        self.input = v;
        self
    }

    #[allow(dead_code)] // 仅 #[cfg(test)] 使用:bin 构建里这是 dead;test profile 里 test_trace_step_raii 实际调用它
    pub fn extra(mut self, v: Value) -> Self {
        self.extra = v;
        self
    }

    /// 闭包在 drop 时调用,可修改 output(如设置 status / err / bytes_done)
    pub fn on_exit(mut self, f: impl FnOnce(&mut Value) + Send + 'static) -> Self {
        self.output_mutator = Some(Box::new(f));
        self
    }
}

impl Drop for TraceStep {
    fn drop(&mut self) {
        let duration_ms = self.started_at.elapsed().as_millis() as u64;
        let mut output = json!({});
        if let Some(f) = self.output_mutator.take() {
            f(&mut output);
        }
        // 同步写 NDJSON — 确保事件顺序严格匹配代码顺序,RAII drop 紧跟业务逻辑
        let event = json!({
            "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "session_id": self.session_id,
            "step": self.step,
            "input": self.input,
            "output": output,
            "duration_ms": duration_ms,
            "extra": self.extra,
        });
        let _ = write_event_to(&self.session_id, &event);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// TraceStage —— 「开始标记 + 真实耗时」两段式 stage 埋点
// ─────────────────────────────────────────────────────────────────────────

/// 一个 stage 的两段式埋点:**开始标记** + **真实耗时**。
///
/// **为什么需要它(2026-10-30)**:旧代码用 `trace_event("resolver_try", ..., 0, ...)`
/// 把 `duration_ms` **写死 0**,而 `eval_trace.py` 的 stall 判定是
/// `step in {resolver_try, download_one, fetch_video_info} and duration_ms >= 30_000`
/// —— duration 恒为 0 ⇒ resolver 这一步**永远**不可能被判成 stall,
/// 而 BUG-D 恰恰就是「串行 resolver 链把会话挂住」。
///
/// 现在:
/// - [`TraceStage::start`] **立刻**写一条 marker 事件(step=`<step>`, duration_ms=0,
///   output=`{"phase":"start"}`)。卡死时进程被 kill / 掉电,marker 已经在盘上,
///   `grep '"step":"resolver_try"'` 就能看到卡在哪个 idx。
/// - [`TraceStage::finish`] 写一条 done 事件(step=`<step>_done`),
///   `duration_ms` = **真实**耗时(内部用 `Instant` 算)。
///
/// **duration 由类型内部计算,调用点没有 duration 参数** —— 结构上就写不死 0
/// (这是「已 codified ≠ 有效」那条纪律的落地方式:类型签名挡住手滑)。
///
/// marker 与 done 绑定**同一个 session**:构造时记下 session_id,
/// 结束时按 id 写回 —— 嵌套 session 下也不会张冠李戴。
pub struct TraceStage {
    /// done 事件的 step 名(默认 `<step>_done`)
    done_step: String,
    input: Value,
    extra: Value,
    started_at: Instant,
    /// 绑定的 session id:marker 与 done 必须落在同一个文件
    session_id: String,
    finished: bool,
}

impl TraceStage {
    /// 开始一个 stage:写开始标记事件,并返回句柄。
    ///
    /// `step` 是 marker 的 step 名;done 事件名自动为 `<step>_done`。
    pub fn start(step: &str, input: Value, extra: Value) -> Self {
        let session_id = current_session_id();
        let stage = Self {
            done_step: format!("{step}_done"),
            input: input.clone(),
            extra: extra.clone(),
            started_at: Instant::now(),
            session_id: session_id.clone(),
            finished: false,
        };
        let marker = json!({
            "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "session_id": session_id,
            "step": step,
            "input": input,
            "output": {"phase": "start"},
            "duration_ms": 0u64,
            "extra": extra,
        });
        let _ = write_event_to(&session_id, &marker);
        stage
    }

    /// 不写开始标记,只写 done 事件。
    ///
    /// 用于「总耗时」这类**聚合** stage:它的耗时是子步骤之和,
    /// 再放一个 marker 只会让 trace 里多一行没信息量的噪声。
    pub fn without_marker(done_step: &str, input: Value, extra: Value) -> Self {
        Self {
            done_step: done_step.to_string(),
            input,
            extra,
            started_at: Instant::now(),
            session_id: current_session_id(),
            finished: false,
        }
    }

    /// 到目前为止的真实耗时(ms)。
    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    /// 放弃这次 stage:**不写任何事件**(连兜底的 abandoned 也不写)。
    ///
    /// 用于「先探一下、探到才记录」的分支 —— 例如「文件已存在」的本地检查:
    /// 探到不存在时这次检查**没有产出**,真正的 done 事件由后面的解析尝试发出。
    /// 不加这个 API 的话,`TraceStage` 被 drop 会写一条 `status=abandoned` 的假事件。
    pub fn discard(mut self) {
        self.finished = true;
    }

    /// 结束 stage:写 done 事件,`duration_ms` = 真实耗时。
    pub fn finish(mut self, output: Value) {
        self.finished = true;
        let duration_ms = self.elapsed_ms();
        let event = json!({
            "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "session_id": self.session_id,
            "step": self.done_step,
            "input": self.input,
            "output": output,
            "duration_ms": duration_ms,
            "extra": self.extra,
        });
        let _ = write_event_to(&self.session_id, &event);
    }
}

impl Drop for TraceStage {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // 没显式 finish(early return / `?` 传播 / panic 展开)→ 兜底写 done,
        // 标记 status="abandoned",让读者知道这次 stage 没正常结束。
        let duration_ms = self.elapsed_ms();
        let event = json!({
            "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "session_id": self.session_id,
            "step": self.done_step,
            "input": self.input,
            "output": {"status": "abandoned"},
            "duration_ms": duration_ms,
            "extra": self.extra,
        });
        let _ = write_event_to(&self.session_id, &event);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 测试 helper:在唯一 tmpdir 下找 session 文件 (OCR P1 #7 修复)
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod test_helpers {
    use std::path::{Path, PathBuf};

    /// 按 session_id 前缀(generate_session_id 输出形如 `bug2ABCDEFG...`)定位 NDJSON。
    /// 不依赖 read_dir().next()(那在多测试同 tmpdir 时返回不确定文件)。
    pub(super) fn find_session_file(tmp: &Path, session_id_prefix: &str) -> PathBuf {
        // trace_session_open 在测试场景(env 已设)直接用 trace_root() 作 file_dir,
        // 不再二次折叠到 YYYY-MM-DD 子目录。线上场景不在测试 helper 用。
        let entries: Vec<_> = std::fs::read_dir(tmp)
            .expect("tmpdir 可读")
            .flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with(session_id_prefix) && name.ends_with(".ndjson")
            })
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "{tmp:?} 下 {session_id_prefix}*.ndjson 应恰好 1 个,实际 {} (文件列表: {:?})",
            entries.len(),
            std::fs::read_dir(tmp)
                .map(|d| d
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
        entries.into_iter().next().unwrap().path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use test_helpers::find_session_file;

    /// 测试 helper: 重置全局 session/file 状态(因为 LazyLock 在测试间共享)
    async fn reset_globals() {
        // 关闭所有 session 的文件句柄(2026-10-30:全局状态从「单例」变成「栈」)
        with_sessions(|sessions| sessions.clear());
        // 等 50ms 让上一测试的 spawn 任务完成 flush
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    /// 测试用:写一条事件后能读回合法 JSON
    #[serial]
    #[tokio::test]
    async fn test_trace_event_roundtrip() {
        reset_globals().await;
        // 用 tmpdir 隔离
        let tmp = std::env::temp_dir().join(format!(
            "trace-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("test").await;
        trace_event(
            "test_step",
            json!({"foo": "bar"}),
            json!({"status": "ok"}),
            42,
            json!({"k": "v"}),
        )
        .await;
        trace_session_close(json!({"total_steps": 1})).await;
        // 等 flush 完成
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 读回(OCR P1 #7 修复:按 session_id 前缀精确定位,不再 read_dir().next())
        let path = find_session_file(&tmp, "test");
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().split('\n').collect();
        assert_eq!(
            lines.len(),
            3,
            "应 3 行: open / event / close, 实得 {lines:?}"
        );

        // 中间行必须是合法 JSON + 5 元组齐全
        let mid: Value = serde_json::from_str(lines[1]).expect("中间行必须合法 JSON");
        assert!(mid.get("ts").is_some());
        assert!(mid.get("session_id").is_some());
        assert_eq!(mid["step"], "test_step");
        assert_eq!(mid["duration_ms"], 42);
        assert_eq!(mid["input"]["foo"], "bar");
        assert_eq!(mid["output"]["status"], "ok");
        assert_eq!(mid["extra"]["k"], "v");

        // 清理
        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    /// 测试用:TraceStep drop 时自动算 duration_ms
    #[serial]
    #[tokio::test]
    async fn test_trace_step_raii() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-raii-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("raii").await;
        {
            let _t = TraceStep::new("sleep_step")
                .input(json!({"ms": 50}))
                .extra(json!({"foo": "bar"}))
                .on_exit(|out| {
                    out["status"] = json!("done");
                });
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        // 给 tokio::spawn 的任务一点时间 flush
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        trace_session_close(json!({})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "raii");
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().split('\n').collect();
        let mid: Value = serde_json::from_str(lines[1]).expect("中间行合法");
        assert_eq!(mid["step"], "sleep_step");
        assert!(
            mid["duration_ms"].as_u64().unwrap() >= 50,
            "duration 应 ≥ 50ms"
        );
        assert_eq!(mid["output"]["status"], "done");

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    /// 集成测试:模拟 handlers.rs 4 故障点完整链路
    /// 验证:
    ///   - session 内所有事件 session_id 一致(grep 关联性)
    ///   - 5 元组字段齐全
    ///   - 顺序:session_open → fetch_video_info → resolver_try×N → download_one → session_close
    ///   - 总事件数 == open + 1 + N + 1 + close = 模拟真实 BUG 复发时 grep 体验
    #[serial]
    #[tokio::test]
    async fn test_integration_4_fault_points() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-integ-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        // === 故障点 1: start_download_impl 入口开 session ===
        let _session = trace_session_open("gui-single").await;

        // === 故障点 2: fetch_video_info_impl — TraceStep RAII ===
        {
            let _t = TraceStep::new("fetch_video_info")
                .input(json!({"url": "https://x.com/user/status/123"}));
            // 模拟 resolver 内部调用
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            trace_event(
                "fetch_video_info_ok",
                json!({"url": "https://x.com/user/status/123"}),
                json!({"status": "ok", "video_count": 1}),
                20,
                json!({"title_len": 50}),
            )
            .await;
        }

        // === 故障点 3: resolve_items_serially — 循环内 trace_event ===
        for idx in 0..3 {
            trace_event(
                "resolver_try",
                json!({"idx": idx, "title": format!("章节{}", idx + 1)}),
                json!({"selected": true}),
                0,
                json!({"resolver_chain_length": 3}),
            )
            .await;
        }

        // === 故障点 4: download_one — TraceStep RAII ===
        {
            let _t =
                TraceStep::new("download_one").input(json!({"title": "章节3", "m3u8_len": 256}));
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }

        // 等 RAII spawn 任务 flush
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // === session close ===
        trace_session_close(json!({"status": "ok", "total_steps": 5})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // === 读回 NDJSON 验证 ===
        let path = find_session_file(&tmp, "gui-single");
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().split('\n').collect();

        // 期望事件数: open + fetch_video_info + fetch_video_info_ok + resolver_try×3 + download_one(RAAI input) + close = 8
        // 注:TraceStep::drop 也会 spawn 一个 trace_event,所以 download_one 块产 2 行(input 行 + drop 触发行)
        assert_eq!(
            lines.len(),
            8,
            "应 8 行事件(TraceStep RAII 产 2 行),实得 {} 行:\n{content}",
            lines.len()
        );

        // 解析所有事件,提取 session_id
        let events: Vec<Value> = lines
            .iter()
            .map(|l| serde_json::from_str(l).expect("每行必须合法 JSON"))
            .collect();
        let session_ids: std::collections::HashSet<&str> = events
            .iter()
            .filter_map(|e| e["session_id"].as_str())
            .collect();

        // 全部事件 session_id 必须一致(grep 才能关联)
        assert_eq!(
            session_ids.len(),
            1,
            "所有事件应共享同一个 session_id,实得 {session_ids:?}"
        );

        // OCR finding #1+#2+#6+#8 regression test 之一:handlers.rs 的 on_exit 模式
        // 写死 "status": "entered",实际结果未知时应写 "entered" 作为初始 placeholder,
        // 但 caller 拿到 Result 后必须改写 status 反映真实 outcome。
        // 这个测试断言:正确捕获 outcome 的模式能写出真实 status。
        #[serial]
        #[tokio::test]
        async fn test_on_exit_records_actual_outcome() {
            reset_globals().await;
            let tmp = std::env::temp_dir().join(format!(
                "trace-on-exit-{}",
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ));
            std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

            let _session = trace_session_open("on_exit").await;

            // 正确模式:用 Arc<AtomicBool> 在 on_exit 里读取 scope 内最终设置的 outcome。
            // on_exit 在 scope drop 时调用,此时 succeeded 已是最新值。
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;
            let succeeded = Arc::new(AtomicBool::new(false));
            let s_clone = succeeded.clone();
            {
                let _t = TraceStep::new("fetch_video_info")
                    .input(json!({"url": "https://x.com/test"}))
                    .on_exit(move |out| {
                        if s_clone.load(Ordering::SeqCst) {
                            out["status"] = json!("ok");
                        } else {
                            out["status"] = json!("err");
                            out["err"] = json!("业务失败");
                        }
                    });
                // 模拟业务执行但不设 succeeded → 走 err 分支
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            trace_session_close(json!({})).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let path = find_session_file(&tmp, "on_exit");
            let content = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = content.trim().split('\n').collect();
            let fetch_line = lines
                .iter()
                .find(|l| l.contains("\"step\":\"fetch_video_info\""))
                .expect("应存在 fetch_video_info 行");
            let ev: Value = serde_json::from_str(fetch_line).expect("JSON 应合法");
            assert_eq!(ev["output"]["status"], "err", "正确模式应捕获 err 路径");
            assert_eq!(ev["output"]["err"], "业务失败");

            let _ = std::fs::remove_dir_all(&tmp);
            reset_globals().await;
        }

        // OCR finding #1+#2+#6+#8 regression test 之二:复现 handlers.rs L461-463/L1122-1126
        // 的反模式 — on_exit 闭包硬编码 status="entered",不论函数实际成功还是失败
        // 都记录 "entered",掩盖真实 outcome。
        #[serial]
        #[tokio::test]
        async fn test_on_exit_wrong_pattern_loses_outcome() {
            reset_globals().await;
            let tmp = std::env::temp_dir().join(format!(
                "trace-on-exit-bug-{}",
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ));
            std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

            let _session = trace_session_open("bug").await;

            // 完全复刻 handlers.rs L459-463 的写法
            {
                let _ = TraceStep::new("fetch_video_info")
                    .input(json!({"url": "https://x.com/test"}))
                    .on_exit(|out| {
                        out["status"] = json!("entered");
                    });
                // 模拟业务执行得 50ms 后 drop(scope 结束)
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            trace_session_close(json!({})).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let path = find_session_file(&tmp, "bug");
            let content = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = content.trim().split('\n').collect();
            let fetch_line = lines
                .iter()
                .find(|l| l.contains("\"step\":\"fetch_video_info\""))
                .expect("应存在 fetch_video_info 行");
            let ev: Value = serde_json::from_str(fetch_line).expect("JSON 应合法");

            // 这就是 OCR finding 的核心证据:不管业务成功还是失败,trace 永远记录 "entered"
            assert_eq!(
                ev["output"]["status"], "entered",
                "OCR finding #1+#2+#6+#8: on_exit 闭包写死 'entered' 覆盖真实 outcome"
            );
            // 反向断言 — 真实 status 不应该被记录
            assert!(
                ev["output"].get("err").is_none() || ev["output"]["err"].is_null(),
                "Bug 模式下不应有 err 字段(被 'entered' 覆盖)"
            );

            let _ = std::fs::remove_dir_all(&tmp);
            reset_globals().await;
        }

        // 验证事件顺序:fetch_video_info RAII drop 在 _t 块结束时 → 在 fetch_video_info_ok 之后
        let steps: Vec<&str> = events.iter().filter_map(|e| e["step"].as_str()).collect();
        assert_eq!(
            steps,
            vec![
                "session_open",
                "fetch_video_info_ok", // 显式 await 同步写
                "fetch_video_info",    // RAII drop 在 _t 块结束时
                "resolver_try",
                "resolver_try",
                "resolver_try",
                "download_one", // RAII drop 在 _t 块结束时
                "session_close",
            ],
            "事件顺序应匹配代码真实控制流"
        );

        // 验证 5 元组字段齐全
        for (i, e) in events.iter().enumerate() {
            assert!(e.get("ts").is_some(), "事件 {i} 缺 ts");
            assert!(e.get("session_id").is_some(), "事件 {i} 缺 session_id");
            assert!(e.get("step").is_some(), "事件 {i} 缺 step");
            assert!(e.get("input").is_some(), "事件 {i} 缺 input");
            assert!(e.get("output").is_some(), "事件 {i} 缺 output");
            assert!(e.get("duration_ms").is_some(), "事件 {i} 缺 duration_ms");
        }

        // 验证 fetch_video_info RAII duration ≥ 20ms (events[2] 是 RAII drop 触发的事件)
        let fetch_event = &events[2];
        assert!(fetch_event["duration_ms"].as_u64().unwrap() >= 20);

        // 验证 resolver_try 的 idx 字段对得上(events[3..6])
        for (i, expected_idx) in (0..3).enumerate() {
            assert_eq!(events[3 + i]["input"]["idx"], expected_idx);
        }

        // 验证 download_one duration ≥ 30ms (events[6] 是 RAII drop 触发的事件)
        let dl_event = &events[6];
        assert!(dl_event["duration_ms"].as_u64().unwrap() >= 30);
        assert_eq!(dl_event["input"]["title"], "章节3");

        // 验证 session_close.output 含 status
        let close_event = &events[7];
        assert_eq!(close_event["output"]["status"], "ok");
        assert_eq!(close_event["output"]["total_steps"], 5);

        // === 模拟 jq 查询: 找耗时最长 step ===
        let mut by_duration: Vec<&Value> = events.iter().collect();
        by_duration.sort_by_key(|e| -(e["duration_ms"].as_u64().unwrap_or(0) as i64));
        let top2: Vec<&str> = by_duration[..2]
            .iter()
            .filter_map(|e| e["step"].as_str())
            .collect();
        // download_one(≥30ms) + fetch_video_info(≥20ms) 应排前二
        assert!(
            top2.contains(&"download_one") && top2.contains(&"fetch_video_info"),
            "jq 排序应暴露 download_one + fetch_video_info,实得 {top2:?}"
        );

        // 清理
        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    // OCR finding #1+#2+#6+#8 regression test 之一:handlers.rs 的 on_exit 模式
    // 写死 "status": "entered",实际结果未知时应写 "entered" 作为初始 placeholder,
    // 但 caller 拿到 Result 后必须改写 status 反映真实 outcome。
    // 这个测试断言:正确捕获 outcome 的模式能写出真实 status。
    #[serial]
    #[tokio::test]
    async fn test_on_exit_records_actual_outcome() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-on-exit-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("on_exit").await;

        // 正确模式:用 Arc<AtomicBool> 在 on_exit 里读取 scope 内最终设置的 outcome。
        // on_exit 在 scope drop 时调用,此时 succeeded 已是最新值。
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let succeeded = Arc::new(AtomicBool::new(false));
        let s_clone = succeeded.clone();
        {
            let _t = TraceStep::new("fetch_video_info")
                .input(json!({"url": "https://x.com/test"}))
                .on_exit(move |out| {
                    if s_clone.load(Ordering::SeqCst) {
                        out["status"] = json!("ok");
                    } else {
                        out["status"] = json!("err");
                        out["err"] = json!("业务失败");
                    }
                });
            // 模拟业务执行但不设 succeeded → 走 err 分支
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        trace_session_close(json!({})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "on_exit");
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().split('\n').collect();
        let fetch_line = lines
            .iter()
            .find(|l| l.contains("\"step\":\"fetch_video_info\""))
            .expect("应存在 fetch_video_info 行");
        let ev: Value = serde_json::from_str(fetch_line).expect("JSON 应合法");
        assert_eq!(ev["output"]["status"], "err", "正确模式应捕获 err 路径");
        assert_eq!(ev["output"]["err"], "业务失败");

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    // OCR finding #1+#2+#6+#8 regression test 之二:复现 handlers.rs L461-463/L1122-1126
    // 的反模式 — on_exit 闭包硬编码 status="entered",不论函数实际成功还是失败
    // 都记录 "entered",掩盖真实 outcome。
    #[serial]
    #[tokio::test]
    async fn test_on_exit_wrong_pattern_loses_outcome() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-on-exit-bug-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("bug").await;

        // 完全复刻 handlers.rs L459-463 的写法
        {
            let _ = TraceStep::new("fetch_video_info")
                .input(json!({"url": "https://x.com/test"}))
                .on_exit(|out| {
                    out["status"] = json!("entered");
                });
            // 模拟业务执行得 50ms 后 drop(scope 结束)
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        trace_session_close(json!({})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "bug");
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().split('\n').collect();
        let fetch_line = lines
            .iter()
            .find(|l| l.contains("\"step\":\"fetch_video_info\""))
            .expect("应存在 fetch_video_info 行");
        let ev: Value = serde_json::from_str(fetch_line).expect("JSON 应合法");

        // 这就是 OCR finding 的核心证据:不管业务成功还是失败,trace 永远记录 "entered"
        assert_eq!(
            ev["output"]["status"], "entered",
            "OCR finding #1+#2+#6+#8: on_exit 闭包写死 'entered' 覆盖真实 outcome"
        );
        // 反向断言 — 真实 status 不应该被记录
        assert!(
            ev["output"].get("err").is_none() || ev["output"]["err"].is_null(),
            "Bug 模式下不应有 err 字段(被 'entered' 覆盖)"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    /// 覆盖 handlers.rs 里 `?` 早退导致「半截 trace」的场景。
    #[serial]
    #[tokio::test]
    async fn test_session_auto_close_on_early_return() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-guard-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        {
            let _session = trace_session_open_guarded("early-return").await;
            // 这里故意不调用 close(),模拟 early return / 错误传播
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "early-return");
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().split('\n').collect();
        assert_eq!(
            lines.len(),
            2,
            "应 open + 兜底 close 共 2 行,实得 {lines:?}"
        );

        let last: Value = serde_json::from_str(lines[1]).expect("末行合法 JSON");
        assert_eq!(last["step"], "session_close");
        assert_eq!(last["output"]["status"], "dropped");

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // OCR HIGH #1+#2+#6+#8 的修复回归:on_exit 通过 Arc<StdMutex<Option<Result>>>
    // 拿到真实 outcome(handlers.rs::download_one / fetch_video_info_impl 已落地)。
    //
    // 这是 "测试验证意图"(rules.md #9):不只测 TraceStep 本身,还测调用者用对模式后
    // 真实 outcome 能穿透 RAII。
    // ─────────────────────────────────────────────────────────────────────────
    #[serial]
    #[tokio::test]
    async fn test_on_exit_with_outcome_mutex_records_real_ok() {
        use std::sync::Arc;
        use std::sync::Mutex as StdMutex;

        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-on-exit-ok-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("ok_fix").await;

        // ── 复刻 handlers.rs::download_one L1122-1163 修复后的写法 ─────────
        let outcome: Arc<StdMutex<Option<Result<(), String>>>> = Arc::new(StdMutex::new(None));
        let outcome_for_exit = outcome.clone();
        {
            let _trace = TraceStep::new("download_one_fix")
                .input(json!({"title": "demo"}))
                .on_exit(move |out| {
                    if let Ok(slot) = outcome_for_exit.lock() {
                        match slot.as_ref() {
                            Some(Ok(())) => out["status"] = json!("ok"),
                            Some(Err(e)) => {
                                out["status"] = json!("err");
                                out["err"] = json!(e);
                            }
                            None => out["status"] = json!("unknown"),
                        }
                    } else {
                        out["status"] = json!("lock_poisoned");
                    }
                });

            let body_result: Result<(), String> = Ok(());
            if let Ok(mut slot) = outcome.lock() {
                *slot = Some(body_result);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        trace_session_close(json!({})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "ok_fix");
        let content = std::fs::read_to_string(&path).unwrap();
        let line = content
            .lines()
            .find(|l| l.contains("\"step\":\"download_one_fix\""))
            .expect("应存在 download_one_fix 行");
        let ev: Value = serde_json::from_str(line).expect("JSON 应合法");

        assert_eq!(ev["output"]["status"], "ok", "Ok 分支必须落 status='ok'");
        assert!(ev["output"].get("err").is_none(), "Ok 分支不应有 err 字段");

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    #[serial]
    #[tokio::test]
    async fn test_on_exit_with_outcome_mutex_records_real_err() {
        use std::sync::Arc;
        use std::sync::Mutex as StdMutex;

        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-on-exit-err-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("err_fix").await;

        let outcome: Arc<StdMutex<Option<Result<(), String>>>> = Arc::new(StdMutex::new(None));
        let outcome_for_exit = outcome.clone();
        {
            let _trace = TraceStep::new("download_one_fix_err")
                .input(json!({"title": "demo_err"}))
                .on_exit(move |out| {
                    if let Ok(slot) = outcome_for_exit.lock() {
                        match slot.as_ref() {
                            Some(Ok(())) => out["status"] = json!("ok"),
                            Some(Err(e)) => {
                                out["status"] = json!("err");
                                out["err"] = json!(e);
                            }
                            None => out["status"] = json!("unknown"),
                        }
                    } else {
                        out["status"] = json!("lock_poisoned");
                    }
                });

            let body_result: Result<(), String> =
                Err("VideoDownloader 初始化失败：missing yt-dlp".into());
            if let Ok(mut slot) = outcome.lock() {
                *slot = Some(body_result);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        trace_session_close(json!({})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "err_fix");
        let content = std::fs::read_to_string(&path).unwrap();
        let line = content
            .lines()
            .find(|l| l.contains("\"step\":\"download_one_fix_err\""))
            .expect("应存在 download_one_fix_err 行");
        let ev: Value = serde_json::from_str(line).expect("JSON 应合法");

        assert_eq!(
            ev["output"]["status"], "err",
            "Err 分支必须落 status='err'(这是反模式的反面证据)"
        );
        assert_eq!(
            ev["output"]["err"], "VideoDownloader 初始化失败：missing yt-dlp",
            "真实 err 文案必须穿透(不是写死的 'entered')"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    // 边界:outcome 仍为 None 时(写之前就 panic/early-return)→ status="unknown"。
    // 退化保护,而非崩溃。
    #[serial]
    #[tokio::test]
    async fn test_on_exit_with_unset_outcome_falls_back_to_unknown() {
        use std::sync::Arc;
        use std::sync::Mutex as StdMutex;

        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-on-exit-none-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("none_fix").await;

        let outcome: Arc<StdMutex<Option<Result<(), String>>>> = Arc::new(StdMutex::new(None));
        let outcome_for_exit = outcome.clone();
        {
            let _trace = TraceStep::new("download_one_unset")
                .input(json!({}))
                .on_exit(move |out| {
                    if let Ok(slot) = outcome_for_exit.lock() {
                        if let Some(Ok(())) = slot.as_ref() {
                            out["status"] = json!("ok");
                        } else if let Some(Err(e)) = slot.as_ref() {
                            out["status"] = json!("err");
                            out["err"] = json!(e);
                        } else {
                            out["status"] = json!("unknown");
                        }
                    }
                });

            // 故意不写 outcome — 模拟 panic 路径 / 提前 return 边界
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        trace_session_close(json!({})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "none_fix");
        let content = std::fs::read_to_string(&path).unwrap();
        let line = content
            .lines()
            .find(|l| l.contains("\"step\":\"download_one_unset\""))
            .expect("应存在 download_one_unset 行");
        let ev: Value = serde_json::from_str(line).expect("JSON 应合法");

        assert_eq!(
            ev["output"]["status"], "unknown",
            "outcome 未写入时退到 unknown,而非崩溃或默认 'ok'"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 2026-10-30:嵌套 session 隔离 + TraceStage 真实耗时
    //
    // 这两条是本轮改动的**红测试**(对旧代码必红):
    //   · test_nested_sessions_keep_own_files —— 旧实现全局单例,内层 session
    //     覆盖外层的文件句柄,外层事件 + 外层 session_close 全落进内层文件,
    //     外层文件只剩 session_open。
    //   · test_trace_stage_records_real_duration —— 旧实现把 duration 写死 0,
    //     resolver 阶段永远不可能被判成 stall。
    // ─────────────────────────────────────────────────────────────────────────

    /// 嵌套 session 各自保有自己的文件 + 自己的 session_id。
    #[serial]
    #[tokio::test]
    async fn test_nested_sessions_keep_own_files() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-nested-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        // 外层 session(模拟 batch)
        let outer = trace_session_open("outer").await;
        trace_event(
            "outer_step",
            json!({"where": "outer"}),
            json!({"status": "ok"}),
            1,
            json!({}),
        )
        .await;

        // 内层 session(模拟 batch 里的单个 item)
        let inner = trace_session_open("inner").await;
        trace_event(
            "inner_step",
            json!({"where": "inner"}),
            json!({"status": "ok"}),
            2,
            json!({}),
        )
        .await;
        // 关掉最内层(inner)
        trace_session_close(json!({"status": "ok"})).await;

        // 内层关掉后,事件应回到外层文件
        trace_event(
            "outer_step_2",
            json!({"where": "outer"}),
            json!({"status": "ok"}),
            3,
            json!({}),
        )
        .await;
        // 关掉外层
        trace_session_close(json!({"status": "ok"})).await;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let outer_path = find_session_file(&tmp, "outer");
        let inner_path = find_session_file(&tmp, "inner");
        assert_ne!(outer_path, inner_path, "两个 session 必须是两个文件");

        let read_events = |p: &std::path::Path| -> Vec<Value> {
            std::fs::read_to_string(p)
                .unwrap()
                .trim()
                .split('\n')
                .map(|l| serde_json::from_str(l).expect("每行必须合法 JSON"))
                .collect()
        };

        let outer_events = read_events(&outer_path);
        let inner_events = read_events(&inner_path);

        // 外层文件:open + 2 个外层事件 + close
        let outer_steps: Vec<&str> = outer_events
            .iter()
            .filter_map(|e| e["step"].as_str())
            .collect();
        assert_eq!(
            outer_steps,
            vec![
                "session_open",
                "outer_step",
                "outer_step_2",
                "session_close"
            ],
            "外层 session 的事件必须全在外层文件里(旧实现会只剩 session_open)"
        );
        // 内层文件:open + 内层事件 + close
        let inner_steps: Vec<&str> = inner_events
            .iter()
            .filter_map(|e| e["step"].as_str())
            .collect();
        assert_eq!(
            inner_steps,
            vec!["session_open", "inner_step", "session_close"],
            "内层 session 的事件必须全在内层文件里"
        );

        // session_id 不串台
        for e in &outer_events {
            assert_eq!(
                e["session_id"],
                outer.as_str(),
                "外层文件里的 session_id 必须是外层的"
            );
        }
        for e in &inner_events {
            assert_eq!(
                e["session_id"],
                inner.as_str(),
                "内层文件里的 session_id 必须是内层的"
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    /// 外层 session 在内层**还没关**的时候就关闭 → close 必须写回**自己**的文件。
    ///
    /// 生产场景:batch 并发 3 个 item,item1 先下载完就关自己的 session,
    /// 此时 item2/item3 的 session 还开着。旧实现「close 只关最内层」会把 item1 的
    /// `session_close` 写进 item3 的文件 —— item1 的文件永远缺 close,
    /// `eval_trace.py` 判「有 open 无 close」→ 整个 session 被丢弃,
    /// 它的 stall 也一起丢了。
    ///
    /// 变异点:`TraceSession::close` 里「按 id 关自己」退化成「关最内层」
    /// (即 `trace_session_close(summary)`)⇒ 外层文件里不会有 session_close ⇒ 必红。
    #[serial]
    #[tokio::test]
    async fn test_outer_session_close_goes_to_its_own_file_while_inner_open() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-nested-close-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        // 外层(模拟 batch)
        let outer = trace_session_open_guarded("outer").await;
        // 内层(模拟并发中的某个 item)
        let inner = trace_session_open_guarded("inner").await;

        // 外层先关(内层还开着)
        let outer_id = outer.session_id.clone();
        outer.close(json!({"status": "ok", "who": "outer"})).await;

        // 内层再关
        inner.close(json!({"status": "ok", "who": "inner"})).await;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let outer_path = find_session_file(&tmp, "outer");
        let inner_path = find_session_file(&tmp, "inner");
        let read_events = |p: &std::path::Path| -> Vec<Value> {
            std::fs::read_to_string(p)
                .unwrap()
                .trim()
                .split('\n')
                .map(|l| serde_json::from_str(l).expect("每行必须合法 JSON"))
                .collect()
        };
        let outer_events = read_events(&outer_path);
        let inner_events = read_events(&inner_path);

        // 外层文件必须有 session_close,且 status/who 是外层自己的
        let outer_close = outer_events
            .iter()
            .find(|e| e["step"] == "session_close")
            .unwrap_or_else(|| panic!("外层文件必须有 session_close(实际: {outer_events:?})"));
        assert_eq!(outer_close["output"]["who"], "outer");
        assert_eq!(outer_close["session_id"], outer_id);

        // 内层文件里**不能**出现外层的 close
        assert!(
            !inner_events
                .iter()
                .any(|e| e["step"] == "session_close" && e["output"]["who"] == "outer"),
            "外层的 session_close 落进了内层文件:{inner_events:?}"
        );
        // 内层自己的 close 仍在
        let inner_close = inner_events
            .iter()
            .find(|e| e["step"] == "session_close")
            .unwrap_or_else(|| panic!("内层文件必须有 session_close(实际: {inner_events:?})"));
        assert_eq!(inner_close["output"]["who"], "inner");

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }
    ///
    /// TraceStage:开始标记先落盘,done 事件带**真实**耗时。
    ///
    /// 变异点(旧代码 / 把 duration 写死 0):`TraceStage::finish` 里的
    /// `self.elapsed_ms()` → `0u64` ⇒ `duration_ms >= 40` 断言必红。
    #[serial]
    #[tokio::test]
    async fn test_trace_stage_records_real_duration() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-stage-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("stage").await;

        let stage = TraceStage::start(
            "resolver_try",
            json!({"idx": 3, "title": "章节4"}),
            json!({"backend": "api"}),
        );
        // marker 必须**先**在盘上(卡死时只有 marker,grep 得到卡住的 idx)
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        stage.finish(json!({"status": "ok", "attempted": true}));

        trace_session_close(json!({"status": "ok"})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "stage");
        let content = std::fs::read_to_string(&path).unwrap();
        let events: Vec<Value> = content
            .trim()
            .split('\n')
            .map(|l| serde_json::from_str(l).expect("每行必须合法 JSON"))
            .collect();

        let marker_idx = events
            .iter()
            .position(|e| e["step"] == "resolver_try")
            .expect("必须有 resolver_try 开始标记");
        let done_idx = events
            .iter()
            .position(|e| e["step"] == "resolver_try_done")
            .expect("必须有 resolver_try_done 结束事件");

        // marker 在 done 之前
        assert!(
            marker_idx < done_idx,
            "开始标记必须先于 done 落盘(卡死时只剩 marker)"
        );

        // marker 自身不带耗时(它是「开始了」的信号,不是测量)
        assert_eq!(events[marker_idx]["duration_ms"], 0);
        assert_eq!(events[marker_idx]["output"]["phase"], "start");
        assert_eq!(events[marker_idx]["input"]["idx"], 3);

        // done 带**真实**耗时 —— 这条断言对「duration 写死 0」必红
        let done_ms = events[done_idx]["duration_ms"]
            .as_u64()
            .expect("duration 必须是 u64");
        assert!(
            done_ms >= 40,
            "resolver_try_done 必须带真实耗时(≥40ms),实得 {done_ms}ms —— 写死 0 就是本 bug"
        );
        assert_eq!(events[done_idx]["output"]["status"], "ok");
        assert_eq!(events[done_idx]["input"]["idx"], 3);
        assert_eq!(events[done_idx]["extra"]["backend"], "api");

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    /// TraceStage 未 finish 就 drop(early return / panic)→ 兜底写 done,标 abandoned。
    #[serial]
    #[tokio::test]
    async fn test_trace_stage_dropped_without_finish_is_marked_abandoned() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-stage-drop-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("stagedrop").await;

        {
            let _stage = TraceStage::start("cli_resolve", json!({"url": "x"}), json!({}));
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            // 不 finish —— 模拟 early return / panic 展开
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        trace_session_close(json!({"status": "ok"})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "stagedrop");
        let content = std::fs::read_to_string(&path).unwrap();
        let done_line = content
            .lines()
            .find(|l| l.contains("\"step\":\"cli_resolve_done\""))
            .expect("未 finish 也必须有一条 done 事件");
        let ev: Value = serde_json::from_str(done_line).expect("JSON 应合法");
        assert_eq!(ev["output"]["status"], "abandoned");
        assert!(
            ev["duration_ms"].as_u64().unwrap() >= 20,
            "abandoned 的 done 也要带真实耗时"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    /// session_close 必须带**真实**的墙钟总耗时。
    ///
    /// Why: `eval_trace.py` 的 `avg_total_ms` 读 `session_close.output["total_ms_proxy"]`,
    /// 而该 key 在整个 src/ 里**从没被写过** ⇒ 这一项指标恒为 0,
    /// 报告里只能显示「⚪ 不可测」(H2:不可测 ≠ 零值)。
    #[serial]
    #[tokio::test]
    async fn test_session_close_carries_real_total_ms() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-totalms-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("totalms").await;
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        trace_session_close(json!({"status": "ok"})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "totalms");
        let content = std::fs::read_to_string(&path).unwrap();
        let close_line = content
            .lines()
            .find(|l| l.contains("\"step\":\"session_close\""))
            .expect("应有 session_close 行");
        let ev: Value = serde_json::from_str(close_line).expect("JSON 应合法");
        assert!(
            ev["output"].get("total_ms_proxy").is_some(),
            "session_close 必须带 total_ms_proxy,否则 avg_total_ms 永远是死指标"
        );
        let ms = ev["output"]["total_ms_proxy"].as_u64().unwrap();
        assert!(
            ms >= 50,
            "墙钟总耗时至少要覆盖 sleep 的 60ms(留 10ms 调度余量),实得 {ms}ms"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }

    /// 测不到耗时时就**不写** `total_ms_proxy` —— 宁可「不可测」也不要假 0。
    ///
    /// H2:不可测 ≠ 零值。写一个 0 会让报告出现「avg_total_ms | 0 | 0 | ✅持平」,
    /// 看起来像「耗时指标很健康」,实际是压根没测到。
    #[serial]
    #[test]
    fn test_inject_total_ms_absent_for_unknown_session() {
        let mut summary = json!({"status": "ok"});
        inject_total_ms(&mut summary, "no-such-session-xyz");
        assert!(
            summary.get("total_ms_proxy").is_none(),
            "session 不在栈里 → 测不到 → 不写 key,不能写 0 冒充测量结果"
        );
    }

    /// 每个 session 必须带埋点 schema 版本 —— `eval_trace.py` 靠它拒绝
    /// 「旧埋点 baseline vs 新埋点 current」的无效对比。
    ///
    /// Why: 本轮改了埋点语义(`resolver_try` duration 从写死 0 变成真实耗时),
    /// 旧 baseline 与新 current 的 stall_count 口径不同,delta 分不清是
    /// 「性能变差」还是「测得更准」。规则之前只写在文档里,没有任何东西检查它。
    #[serial]
    #[tokio::test]
    async fn test_session_open_carries_schema_version() {
        reset_globals().await;
        let tmp = std::env::temp_dir().join(format!(
            "trace-schema-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::env::set_var("X_VIDEOS_TRACE_DIR", &tmp);

        let _session = trace_session_open("schema").await;
        trace_session_close(json!({"status": "ok"})).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let path = find_session_file(&tmp, "schema");
        let content = std::fs::read_to_string(&path).unwrap();
        for step in ["session_open", "session_close"] {
            let line = content
                .lines()
                .find(|l| l.contains(&format!("\"step\":\"{step}\"")))
                .unwrap_or_else(|| panic!("应有 {step} 行"));
            let ev: Value = serde_json::from_str(line).expect("JSON 应合法");
            assert_eq!(
                ev["extra"]["schema_version"],
                json!(TRACE_SCHEMA_VERSION),
                "{step} 必须带 extra.schema_version"
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
        reset_globals().await;
    }
}
