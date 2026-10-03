# 2026-09-17 — commit-msg hook 自动注入 Ref-trace (session_id 跳转 + 4 tests)


### 1. 工具交付

| 文件 | 类型 | 行数 | 职责 |
|------|------|------|------|
| `scripts/trace_link.py` | 新增 | ~85 | 扫描今天 NDJSON → 注入 `Ref-trace: <abs_path>` 到 commit msg |
| `tests/test_trace_link.py` | 新增 | ~99 | 4 case: 无 trace_dir / 注入 / 已存在 skip / 无 \n 自动补空行 |
| `scripts/git_hooks/commit-msg` | 改 | +18 | Intent Lock → Ref-trace 注入 → DID/VC trailer 三段流水线 |

### 2. 链路

```
git commit → .git/hooks/commit-msg
            ├─ Intent Lock 校验(必须含 "Intent: <一句话>")
            ├─ Ref-trace 注入(扫描 ~/.local/share/x_videos-dowloads-trace/<today>/*.ndjson)
            └─ Agent DID + VC trailer(已有)
```

### 3. 验证

- `python3 -m py_compile scripts/git_hooks/commit-msg`: OK
- `python3 -m pytest tests/test_trace_link.py -v`: **4 passed**
- 真 hook 冒烟测试(exec source + 假 trace file): Ref-trace 正确插入 Intent 之后 / DID 之前
- `cmp scripts/git_hooks/commit-msg .git/hooks/commit-msg`: 一致(`install_git_hooks.sh` 重装同步)
- 退出码 0(Intent + Ref-trace + DID 都通过)

### 4. 设计要点

- **场景**: trace instrumentation(commit baeea9c)落地后,BUG 复发需要按 `session_id` 一键跳转 trace NDJSON,手工写 `Ref-trace:` 易忘 → 自动注入
- **位置**: Intent Lock 之后 / DID 之前(避免 DID trailer 把 Ref-trace 行掩盖)
- **幂等**: `^Ref-trace:\s*.+$` MULTILINE 匹配,已存在 skip
- **兼容**: trace_dir 不存在 → no-op,无副作用
- **git 不跟踪 hook**: `scripts/git_hooks/commit-msg` 是源,`.git/hooks/commit-msg` 是 install 后的副本

### 5. 决策链

- `MEMORY.md:local-ndjson-trace-instrumentation.md`(commit baeea9c,trace 落盘)
- `MEMORY.md:trace-eval-pipeline-4-metrics.md`(commit 767011d,eval 4 指标)
- → 本次: Ref-trace 把"trace"和"commit"关联,闭环可观测

### 6. 已知 follow-up

- 真有 NDJSON 时才注入 — 无 trace 时无 Ref-trace 行,首次 commit(还没跑过下载) 不会出现
- 跨日 commit 会注入当天 trace,BUG 跨天复发需手动 `git commit --amend`

---

