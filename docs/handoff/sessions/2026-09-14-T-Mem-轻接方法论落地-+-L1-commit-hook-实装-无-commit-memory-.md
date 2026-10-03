# 2026-09-14 — T-Mem 轻接方法论落地 + L1 commit hook 实装 (无 commit, memory + hook only)


### 1. T-Mem 调研结论

用户问 GitHub Sherlockwz/T-Mem repo (EMNLP 2026 论文) 对 Claude Code 记忆体的帮助。T-Mem 核心 idea = **typed graph memory + triggered memory recall**, 4 个 trigger family:

| 家族 | item-level | scene-level |
|------|-----------|------------|
| descriptive | 当前任务直接匹配 | 当前场景上下文 |
| **associative** | **跨概念关联召回 (核心 insight)** | 跨场景迁移 |

**不直接接 T-Mem 的 3 原因**:
1. T-Mem 用 LLM 做 trigger detection + graph query, 我们已有 grep + memory 索引, 触发准确率难说提升
2. 论文用自家训练 LLM, 不是 Claude
3. **核心 insight (associative recall) 零成本可借用**: 在每条 lesson frontmatter 加 `trigger_associative` 字段, grep 时拿全部相关教训

### 2. 🟢 轻接方案 — 做了什么

- `~/memory/projects/x-videos-dowloads.md` 顶部 frontmatter 加 3 字段:
  - `tmem_schema_version: 1` — schema 版本号, 未来升级判断
  - `tmem_search_command: "..."` — 标准 grep 查询命令, 一键拿全部触发器
  - quote 块含 SOP 路径
- 5 条教训加 `<!-- tmem-triggers -->` 块 (descriptive + associative + related):
  - 教训 #14 (smart dispatch 5 步)
  - 教训 #15 (HttpOnly cookie export blocker)
  - 教训 #16 (camoufox MCP evaluate_js isolated world)
  - 教训 #17 (新功能前必查 30 秒 3 项)
  - 教训 #18 (复制异步 UI 状态机入口 N 步清单)

### 3. SOP 5 步流程 (写 projects/x-videos-dowloads.md 第 15-65 行)

1. **触发判断** — 是否值得加触发器? (出现 ≥ 2 次 OR 跨项目通用 OR 教训级别 ≥ MEDIUM)
2. **grep 已有** — `grep -nE '^## lesson-|<!-- tmem-triggers'` projects/x-videos-dowloads.md, 避免重复
3. **写 frontmatter** — `<!-- tmem-triggers -->` 块 + descriptive (item) + associative (5-8 keywords) + related (cross-refs)
4. **grep 验证** — `grep -nE '^<!-- tmem-triggers'` projects/x-videos-dowloads.md 确认格式
5. **写 ## Session section** — 本次新触发器 ID + 教训改动

### 4. L1 commit hook 实装

**目标**: commit 现固瞬间自动提醒开发者是否需要写教训 (避免写教训靠记忆)

**位置**: `.git/hooks/post-commit` (已有 `agent_did.py` 信誉系统, 末尾追加)

**实现**: `_maybe_tmem_reminder()` 函数 + 调用:

```python
def _maybe_tmem_reminder() -> None:
    try:
        msg = subprocess.check_output(["git", "log", "-1", "--pretty=%B"], text=True).strip()
        changed = subprocess.check_output(["git", "diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"], text=True).strip().splitlines()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return

    if not any(kw in msg for kw in ("教训", "经验", "根因", "fix(", "refactor(", "perf(")):
        return
    if len(changed) < 3:
        return

    print("", file=sys.stderr)
    print("💡 T-Mem 触发器提醒:", file=sys.stderr)
    # ... 6 行 SOP 引导
```

**关键设计**:

| 决策 | 理由 |
|------|------|
| post-commit 而非 commit-msg | commit 现固后才提醒, 不阻断 commit 流程 (commit-msg 已用于 Intent Lock) |
| 关键词 + 文件数双 gate | 单关键词易误报 (e.g. `fix typo`), 单文件数易误报 (e.g. README typo) |
| **stderr 而非 stdout** | **git 仅显示 hook stderr 给用户** (隐藏 bug #1: stdout 被 swallow) |
| 6 行提示而非 1 行 | grep 命令 + SOP 路径 + 模板位置 + 验证命令 — 让用户在 commit 现场就能补 |
| 不自动写 memory | LLM 不可信, 必须 human 写; 提醒到位即可 |

### 5. 测试矩阵 (4×2 全过)

测试仓库 `/Users/hgxszhj/.claude/jobs/f4e8694b/tmp/tmem-hook-test` (后已清理):

| 场景 | commit msg | 改动文件数 | 期望 | 实际 |
|------|------------|-----------|------|------|
| A ✅ | "fix(gui): 教训 #18 poller 复制漏步骤" | 3 (src+app+handlers) | 触发 + 6 行 | ✅ stderr 6 行 |
| B ❌ | "fix(gui): 教训 #18 poller 复制漏步骤" | 1 (只 src) | 不触发 | ✅ silent |
| C ❌ | "docs: 更新 README typo" | 3 | 不触发 | ✅ silent |
| D ❌ | "docs: 更新 README typo" | 1 | 不触发 | ✅ silent |

**修复的 2 个隐藏 bug**:

1. **git swallow stdout** — 第一版用 `print(...)` stdout, hook 跑通 sentinel 但用户看不到输出。**根因**: git 设计上不显示 hook stdout 给用户, 只显示 stderr (用户调脚本时便于日志捕获)。**修复**: 全部改 `print(..., file=sys.stderr)`
2. **test repo hooksPath 继承 global** — 测试 repo 用 `git init` 创建后, 触发全局 `core.hooksPath=/Users/hgxszhj/.git-hooks` (`~/.gitconfig` 全局配置), 绕过测试 repo 自己的 `.git/hooks/post-commit`。**根因**: git 优先用 global config, 新 init repo 不主动覆盖。**修复**: `git config core.hooksPath .git/hooks` 强制本地覆盖。本项目 `x_videos-dowloads` 仓库已正确配置 `core.hooksPath=.git/hooks`, 不受影响。

### 6. 记忆膨胀警告

| 文件 | 当前行数 | 阈值 | 状态 |
|------|---------|------|------|
| `~/memory/decisions.md` | 346 | 300 | ❌ 超 46 行, 待用户归档整理 |
| `~/memory/projects/x-videos-dowloads.md` | 807 | 800 | ⚠️ 接近阈值, 下次写入前先 grep 已有 |
| `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md` | - | - | OK 索引文件, 已有 27 条 |

**建议**: 下次写项目 memory 前, 优先合并到现有 lessons 而非新建, 用 H2 ## 标题 + 教训号连续编号 (#21, #22 ...)。

### 7. 待办 / 后续

- [ ] 用户下次真实 fix|refactor|perf commit (msg 含关键词 + ≥ 3 文件) 时, hook 应自动 stderr 6 行 SOP 引导 → 用户报告实际打印或未打印 (验证 hook live)
- [ ] 记忆归档: decisions.md 346 → 300 以下 (拆分 `decisions-archive/2026-09.md`)
- [ ] 写更多 lessons 时持续加 tmem-triggers (cross-project recall 覆盖率提升)

---

## 最新 Session: 2026-09-14 (cont.) — SHANHU batch_video_state 失败状态写回 BUG-B + DownloadResult.success 检查 BUG-C 修复 (commits `b96a104` → `286accd`)

### 1. 症状
BUG-A (33f02f6) 修完后,BatchState UI 进度条会动了 (0% → 100%),但每条 status 永远显示 "completed"。事实上 m3u8 段全部 HTTP 404 (URL 几分钟过期),磁盘 0 字节。

### 2. 根因链 — 失败路径"看不见"的两层串联
- **BUG-B** (`batch_download_impl`): 改了 `Vec<(idx, url, success, status, progress)>` 返回,但 `app_batch_video.rs:194` spawn 块**不消费**这个 Vec,直接 `s.batch_state.visible = false` → store 永远保留初始 "pending" → UI 不显示 status。
- **BUG-C** (`start_download_impl`): 上层 `VideoDownloader::download()` 在所有 404 时返 `Ok(DownloadResult{success=false, ..})`,旧代码 `let result = ...?; Ok(())` 直接吞掉 success 标志 → 假成功传到 BUG-B → 假 completed 写到 UI。

**正确参考**: `handlers.rs:868-876` `download_one` (Course pool 路径) 本来就有 `if result.success { Ok(()) } else { Err(...) }`,只是 batch 路径漏写。两 bug 是同一类 — 错误处理透传到 UI 时的"双重检查"。

### 3. ultragoal 三轮迭代
本次按 ultragoal 模式循环,3 轮命中 3 个独立 bug。每轮都跑 5 步验证矩阵:

| Iter | Bug | 验证 ✅ | 失败现象 | 根因 | 修复 | Commit |
|------|-----|---------|---------|------|------|--------|
| 1 | A | b/t/l/r | UI 卡 0% pending | `poller.send(Batch)` 漏 | `app_batch_video.rs:166` + `app.rs:399` 透传 | `33f02f6` |
| 2 | B | b/t/l/r | status 永远 completed | `batch_download_impl` 结果不消费 | `app_batch_video.rs:194-205` 写回 store | `b96a104` |
| 3 | C | b/t/l/r | status 仍 completed (success 标志丢失) | `start_download_impl` 不检查 `result.success` | `handlers.rs:97-117` 加 success check | `286accd` |

**5 步验证矩阵 (写入 DNA `docs/dna/review-checklist.md` §待加)**:
1. `cargo build --bin x-video-downloader` ✅
2. `cargo test --bin` ✅ **287 passed / 0 failed / 10 ignored**
3. `cargo clippy -- -D warnings` ✅ clean
4. `code-reviewer` agent ✅ APPROVE 0 CRITICAL/HIGH/MEDIUM/LOW
5. **运行时** — 用户实测 GUI (Cancel 按钮压测 + m3u8 过期 404 场景)

Stop 条件: 5 步全 ✅ → `result:`; 任一 ❌ → 下一轮; 3 轮后仍未通过 → Rule 13 如实汇报。

### 4. 修复

**`b96a104` — BUG-B 结果写回 store**:
```rust
// app_batch_video.rs:194-205
let results = batch_download_impl(urls, progress_arcs, speed_arcs, cancel_flags).await;
{
    let mut s = state.write();
    for (idx, _url, _success, status, progress) in results {
        if let Some(item) = s.batch_state.items.get_mut(idx) {
            item.status = status;
            item.progress = progress;
        }
    }
    s.batch_state.visible = false;
    s.is_downloading = false;  // Reviewer Sugg #1: 合并到同一事务,避免 poller tick race
}
```

**`286accd` — BUG-C success 检查**:
```rust
// handlers.rs:97-117
let result = downloader.download(&request, callback).await
    .map_err(|e| { /* downcast DownloadError → ErrorDisplay 结构化 */ })?;
if result.success {
    Ok(())
} else {
    Err(ErrorDisplay::new(
        "download",
        result.error_message.unwrap_or_else(|| "下载失败 (未知错误)".to_string()),
        "检查视频 URL 是否过期,或重新 Analyze Course 刷新 m3u8 后再下载",
    ))
}
```

### 5. 教训 #19 + #20

**#19 失败路径可见性原则**:任何把 `Result<Success, Err>` 透传到 UI 的路径必须**双重检查**:
- 上层"下载/网络/解析"函数:不仅要 `?` 转换,还要检查 `.success` / `.is_ok()` 标志字段 (例如 `DownloadResult.success`)
- 调用层:必须**显式消费**函数返回的中间结果数组,不允许丢弃 (`let _ = ...` 是危险信号)

**#20 ultragoal 5 步验证矩阵**: 不可省略任何一步。代码静态通过 ≠ 运行时正确 (BUG-A 静态无问题但 GUI 卡死;BUG-B 静态无问题但 UI 假 completed;BUG-C 同)。`runtime` 步是 BUG-A/B/C 全部命中的关键。

### 6. 验证
- `b96a104`: cargo ✅ / 287 ✅ / clippy ✅ / reviewer ✅
- `286accd`: cargo ✅ / 287 ✅ / clippy ✅ / reviewer ✅ (0/0/0/0, 1 NIT optional)
- GUI binary 重编 78.2M (`mtime 07:14:23 CST`)
- 旧 GUI PID 74690 已自然退出

### 7. 待用户实测 (final)
1. **失败路径**: Analyze Course → 等几分钟让 m3u8 URL 过期 → Batch Download
   - 预期: item 显示 "下载失败: HTTP 404..." (红色),不是 "completed"
2. **成功路径**: 下载一个有效 m3u8
   - 预期: item 仍 "completed" (假阳性场景回归)

### 8. 关联
- BUG-A session (33f02f6) — 上游, poller 消息机制
- 教训 #18 "复制异步 UI 状态机入口时,镜像完整 N 步清单" → BUG-B/C 是它在错误处理层的变种
- 教训 #14 "第三方协议不仅指 HTTP API,也指 Coroutine 协议" → BUG-C 是它的 `DownloadResult` 变种
- ultragoal 模板 → `~/.claude/projects/.../memory/ultragoal-methodology-loop.md` (新增)
- `docs/dna/review-checklist.md` → 待加 §5 验证矩阵

---

## 最新 Session: 2026-09-14 (cont. 2) — SHANHU batch_video_state 3-item batch spawn 卡死 BUG-D 诊断 trace (commit `841e56d`)

### 1. 症状
BUG-A/B/C 全部 commit + GUI 重编后实测: **1-item batch ✅ 正常** (Batch 4: failed with err msg, BUG-C verified), **3-item batch ❌ 卡死** — 4m27s **完全无** `[batch_download] sub-task 启动` 日志, GUI 3 items 永远 "pending"。

### 2. 根因(待定位, 数量相关)
1-item 工作, 3-item 卡死。候选假设:

- **H1** `Semaphore::new(3)` + 3 acquire_owned 死锁 (Batch 4 单 acquire 不触发)
- **H2** dioxus::spawn → Dioxus runtime; inner `tokio::spawn` → global tokio; runtime 不一致 inner 不调度
- **H3** `urls.into_iter().enumerate().map(...).collect()` eager iterator 卡 (跑 3 次 tokio::spawn)
- **H4** `for handle in handles { handle.await }` 顺序 await, handle[0] 永不结束 → 后 2 个等不到

### 3. 修复 (`841e56d` — 仅诊断, 不改逻辑)
4 处 surgical `tracing::info!`, 形成调用链二分:

```
#1 [batch_video] outer spawn 启动 urls=N       (app_batch_video.rs:189)
#2 [batch_impl] enter urls=N                   (handlers.rs:133)
#3 [batch_impl] inner spawn body enter idx=N   (handlers.rs:148)
#4 [batch_impl] handles 收集完成 n=N           (handlers.rs:216)
+ 现有 [batch_download] sub-task 启动 (semaphore.acquire 之后)
```

判定矩阵 (grep log 看哪条缺失 → 卡点 = 上一段):
- 缺 #1 → dioxus spawn 没 poll outer closure
- 缺 #2 → outer spawn poll 但 impl future 没进度 (impossible)
- 缺 #3 → **tokio 没调度 inner task** (最可能, H2 命中)
- 缺 #4 → iterator/.collect() 卡 (H3 命中)
- 有 #3 无 sub-task 启动 → `semaphore.acquire_owned()` 阻塞 (H1 命中)

### 4. 验证
cargo build ✅ / 287 tests ✅ / clippy -D warnings ✅ / code-reviewer APPROVE 0/0/0/0, 1 NIT (prefix `[batch_video]` vs `[batch_impl]` 分裂是设计意图 — 区分 outer poll vs impl poll, 不阻塞)。

### 5. 待用户实测 (next)
1. `pkill -f x-video-downloader` 杀旧 GUI (PID 40175 仍跑旧 binary)
2. `cargo run --bin x-video-downloader -- --gui` 启新
3. Analyze Course → 勾 3 items → Batch Download → `tail -f /tmp/x-video-downloader.log | grep batch_`
4. 判定: 4 条 trace 哪条最后出现 → 卡点 = 上一段
5. 对照: 同样跑 1-item → 期望 5 条日志齐全
6. 贴 log → 我按 trace 链给 fix

### 6. 关联
- BUG-A/B/C (33f02f6/b96a104/286accd) — 上游, BUG-D 是它们的"runtime 副作用"
- 教训 #20 ultragoal 5 步验证矩阵 → BUG-D step 5 命中, 但 ultragoal 不能定位子 bug, 需 trace 链二分
- DNA `docs/dna/review-checklist.md` → 待加 "异步 spawn 调试" 专项 checklist

---

