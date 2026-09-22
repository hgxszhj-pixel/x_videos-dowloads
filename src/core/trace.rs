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

/// 全局 session_id(由 `trace_session_open` 设置一次,所有 trace_event 共用)
static SESSION_ID: LazyLock<std::sync::Mutex<Option<String>>> = LazyLock::new(|| std::sync::Mutex::new(None));

/// 全局 trace 文件句柄(由 `trace_session_open` 打开,`trace_session_close` 关闭)
/// 用 std::sync::Mutex + std::fs::File 同步写,确保 RAII Drop 可直接落盘,事件顺序严格匹配代码顺序。
static TRACE_FILE: LazyLock<std::sync::Mutex<Option<std::fs::File>>> = LazyLock::new(|| std::sync::Mutex::new(None));

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
            *TRACE_FILE.lock().unwrap() = Some(f);
            *SESSION_ID.lock().unwrap() = Some(session_id.clone());

            // 写入 session meta 作为 NDJSON 第一行(special step="session_open")
            let meta = json!({
                "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
                "session_id": session_id,
                "step": "session_open",
                "input": {},
                "output": {"file_path": file_path.to_string_lossy()},
                "duration_ms": 0u64,
                "extra": {"prefix": prefix}
            });
            let _ = write_event(&meta);
        }
        Err(e) => {
            eprintln!("[trace] 打开 trace 文件失败 {file_path:?}: {e}");
        }
    }

    session_id
}

/// 关闭当前 trace session(写一行 `session_close` 事件 + flush)
///
/// OCR HIGH #6 修复:旧实现连续 lock 两个 Mutex 各两次(SESSION_ID + TRACE_FILE),
/// IO(flush) 发生在两次 lock 之间,任一 lock 中毒都会 unwrap() panic。
/// 改法—先拷数据释放锁,IO 在锁外,各自 lock 一次清空,用 unwrap_or_else
/// 兼容 mutex poison 不再 panic(panic 不该从 close 路径冒出来)。
pub async fn trace_session_close(summary: Value) {
    let session_id = SESSION_ID
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default();
    let close_event = json!({
        "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        "session_id": session_id,
        "step": "session_close",
        "input": {},
        "output": summary,
        "duration_ms": 0u64,
        "extra": {}
    });
    let _ = write_event(&close_event);

    // 各自独立 lock + poison-tolerant 清空
    if let Ok(mut g) = TRACE_FILE.lock() {
        *g = None;
    }
    if let Ok(mut g) = SESSION_ID.lock() {
        *g = None;
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
}

impl TraceSession {
    /// 显式关闭并写 summary;幂等(重复调用无副作用)。
    pub async fn close(mut self, summary: Value) {
        self.closed = true;
        if self.opened {
            trace_session_close(summary).await;
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
        // 1. 先 lock SESSION_ID 拷 session_id 后立即释放,
        // 2. 再 lock TRACE_FILE 清空(独立临界区,无嵌套),
        // 3. 最后 lock SESSION_ID 清空(独立临界区)。
        // IO(flush)完全在 lock 外执行。
        let session_id = SESSION_ID
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_default();
        let event = json!({
            "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "session_id": session_id,
            "step": "session_close",
            "input": {},
            "output": {
                "status": "dropped",
                "reason": "session guard dropped without explicit close (early return / error / panic)"
            },
            "duration_ms": 0u64,
            "extra": {"implicit": true}
        });
        // write_event 自己内部 lock TRACE_FILE 短暂,flush 在锁内但只此一次。
        let _ = write_event(&event);
        // 各自独立 lock,清空 globals。
        if let Ok(mut g) = TRACE_FILE.lock() {
            *g = None;
        }
        if let Ok(mut g) = SESSION_ID.lock() {
            *g = None;
        }
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
    // 成功设上(目录创建失败 / file open Err)。检查 TRACE_FILE 是否真设了。
    let opened = TRACE_FILE.lock().map(|g| g.is_some()).unwrap_or(false);
    // 即使 opened=false 也不覆盖 — 旧 session 可能仍开着,我们不该把它清掉。
    let _ = session_id;
    TraceSession {
        closed: false,
        opened,
    }
}

/// 写入单个 trace 事件(5 元组 + extra)
pub async fn trace_event(step: &str, input: Value, output: Value, duration_ms: u64, extra: Value) {
    let event = json!({
        "ts": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        "session_id": SESSION_ID.lock().unwrap().clone().unwrap_or_default(),
        "step": step,
        "input": input,
        "output": output,
        "duration_ms": duration_ms,
        "extra": extra
    });
    let _ = write_event(&event);
}

/// 内部:同步写 NDJSON 一行(append + flush)
fn write_event(event: &Value) -> std::io::Result<()> {
    let mut guard = TRACE_FILE.lock().unwrap();
    if let Some(f) = guard.as_mut() {
        use std::io::Write;
        let mut line = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
        line.push('\n');
        f.write_all(line.as_bytes())?;
        f.flush()?;
    } else {
        // 无活动 session:fallback best-effort append 到 no-session/<date>.ndjson
        // 失败静默,不阻塞主流程,避免 stderr 刷屏
        let _ = append_no_session_event(event);
    }
    Ok(())
}

/// Fallback:无活动 session 时把事件写到 `<trace_root>/no-session/YYYY-MM-DD.ndjson`
/// 与正常 session NDJSON 隔离,session_id 为空(便于 grep / 过滤)。
/// 任何 IO 错误均返回 Err,调用方静默丢弃。
fn append_no_session_event(event: &Value) -> std::io::Result<()> {
    use std::io::Write;
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let path = trace_root().join("no-session").join(format!("{date}.ndjson"));
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
///     .extra(json!({"idx": idx}))
///     .on_exit(|out| out["status"] = json!("ok"));
/// // ... 业务逻辑 ...
/// // drop 时自动写 trace_event
/// ```
pub struct TraceStep {
    step: String,
    input: Value,
    extra: Value,
    output_mutator: Option<Box<dyn FnOnce(&mut Value) + Send>>,
    started_at: Instant,
}

impl TraceStep {
    pub fn new(step: impl Into<String>) -> Self {
        Self {
            step: step.into(),
            input: json!({}),
            extra: json!({}),
            output_mutator: None,
            started_at: Instant::now(),
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
            "session_id": SESSION_ID.lock().unwrap().clone().unwrap_or_default(),
            "step": self.step,
            "input": self.input,
            "output": output,
            "duration_ms": duration_ms,
            "extra": self.extra,
        });
        let _ = write_event(&event);
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
                .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect::<Vec<_>>())
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
        // 关闭文件
        *TRACE_FILE.lock().unwrap() = None;
        *SESSION_ID.lock().unwrap() = None;
        // 等 50ms 让上一测试的 spawn 任务完成 flush
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    /// 测试用:写一条事件后能读回合法 JSON
    #[serial]
    #[tokio::test]
    async fn test_trace_event_roundtrip() {
        reset_globals().await;
        // 用 tmpdir 隔离
        let tmp = std::env::temp_dir().join(format!("trace-test-{}", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
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
        assert_eq!(lines.len(), 3, "应 3 行: open / event / close, 实得 {lines:?}");

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
        let tmp = std::env::temp_dir().join(format!("trace-raii-{}", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
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
        assert!(mid["duration_ms"].as_u64().unwrap() >= 50, "duration 应 ≥ 50ms");
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
            let _t = TraceStep::new("download_one")
                .input(json!({"title": "章节3", "m3u8_len": 256}));
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
        assert_eq!(lines.len(), 8, "应 8 行事件(TraceStep RAII 产 2 行),实得 {} 行:\n{content}", lines.len());

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
                "fetch_video_info_ok",   // 显式 await 同步写
                "fetch_video_info",      // RAII drop 在 _t 块结束时
                "resolver_try",
                "resolver_try",
                "resolver_try",
                "download_one",          // RAII drop 在 _t 块结束时
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
        assert_eq!(lines.len(), 2, "应 open + 兜底 close 共 2 行,实得 {lines:?}");

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

            let body_result: Result<(), String> = Err("VideoDownloader 初始化失败：missing yt-dlp".into());
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
}
