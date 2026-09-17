# Trace ↔ Commit 关联 (Ref-trace 注入)

> **起源**: 2026-09-17 trace instrumentation + eval pipeline 两段落地后,把"trace NDJSON"和"git commit"用一行 trailer 闭环关联。
> **目的**: BUG 复发时 `git log --grep=Ref-trace` 5 秒定位到当天 NDJSON,不再"事后翻 trace 猜哪个 commit"。
> **对应决策**: `docs/dna/trace-schema.md` 的下游 — commit message 末尾追加 `Ref-trace: <abs_path>`,自动而非手工。

---

## 三段流水线(commit-msg hook 顺序)

每次 `git commit` 触发 `scripts/git_hooks/commit-msg`(同步到 `.git/hooks/commit-msg`),按以下顺序执行:

| 阶段 | 模块 | 输入 | 输出/动作 | 失败行为 |
|------|------|------|-----------|----------|
| 1. Intent Lock | `scripts/intent_lock.py` | commit msg | 必须含 `Intent: <一句话>` 一行 | **exit 1**(拒绝 commit) |
| 2. Ref-trace 注入 | `scripts/trace_link.py` | 今天的 trace_dir + commit msg | 追加 `Ref-trace: <abs_path>/<file>.ndjson` | no-op(trace_dir 缺失静默) |
| 3. DID/VC trailer | `scripts/agent_did.py` | DID + 信誉分 | 追加 `[DID: ...] [VC: <hash>] rep=<score>` | no-op(agent_did.py 缺失静默) |

> **顺序硬约束**: 阶段 2 必须在阶段 3 之前 — 否则 DID trailer 拼接会破坏 Ref-trace 行格式。

---

## Ref-trace 注入规范

### 注入格式

```
<原 commit msg 主体>

Ref-trace: /Users/<user>/.local/share/x_videos-dowloads-trace/2026-09-17/01JB5K8Z9R3X7V4Y-gui-single.ndjson
```

- **位置**: commit msg 主体末尾,DID/VC trailer 之前
- **路径**: **绝对路径**(跨 commit/跨机器 grep 仍有效,不依赖 cwd)
- **mtime 选最新**: 同一天多个 session 时,取 mtime 最大的 NDJSON(假设最近一次 commit 关联最近的下载)

### 幂等(skip 规则)

```python
REF_TRACE_RE = re.compile(r"^Ref-trace:\s*.+$", re.MULTILINE)
if REF_TRACE_RE.search(commit_msg):
    return commit_msg, False   # 已存在,no-op
```

- `git commit --amend` 时不会重复注入
- `git rebase -i` 后 commit msg 重写也不会重复注入(已存在的 Ref-trace 行会被新 hook 调用识别)
- 跨日 commit 注入当天 trace(后续 `git commit --amend` 可手动改日期)

### No-op 行为

| 条件 | 行为 |
|------|------|
| `~/.local/share/x_videos-dowloads-trace/YYYY-MM-DD/` 不存在 | 静默返回,不加 Ref-trace 行 |
| 当天目录存在但无 `.ndjson` | 静默返回 |
| commit msg 已含 `Ref-trace:` | 静默返回 |
| `scripts/trace_link.py` 缺失 | 整个阶段 2 跳过(不影响阶段 1/3) |

---

## 文件布局

```
~/.local/share/x_videos-dowloads-trace/        ← trace NDJSON 落盘点(已有)
├── 2026-09-17/
│   ├── 01JB5K8Z9R3X7V4Y-gui-single.ndjson
│   └── 01JB5K8Z9R4ABCDEF-batch.ndjson
└── ...

scripts/
├── trace_link.py            ← 本规范配套模块,纯 stdlib Python (~85 行)
├── git_hooks/
│   └── commit-msg           ← 源文件(git 跟踪,改这里)
└── install_git_hooks.sh     ← 把源拷到 .git/hooks/commit-msg

tests/
└── test_trace_link.py       ← 4 case:无 trace_dir / 注入 / 已存在 / 无 \n 补空行
```

### git 不跟踪 hook(关键)

```
scripts/git_hooks/commit-msg   ← 源文件(git 跟踪 ✓)
.git/hooks/commit-msg          ← 安装副本(git 忽略 ✗)
```

**踩坑必避**: 改 `.git/hooks/commit-msg` 不更新 `scripts/git_hooks/commit-msg` → 下次 `install_git_hooks.sh` 被源文件覆盖 → 改动丢失。

**正确流程**:
1. 改 `scripts/git_hooks/commit-msg`(源)
2. 跑 `bash scripts/install_git_hooks.sh`(同步副本)
3. `cmp scripts/git_hooks/commit-msg .git/hooks/commit-msg`(验证一致)
4. commit 源文件(`.git/hooks/` 不入 git)

---

## 不做的事(防 scope creep)

- ❌ **不替 Intent Lock / DID/VC trailer 做事**: 仅追加 Ref-trace 行,其他阶段逻辑零改动
- ❌ **不查 git log 反推上次 commit 的 trace**: 当天最新 mtime 已够用,反推会让 hook 慢
- ❌ **不跨日查找**: `git commit --amend` 时手动改日期即可,不要写历史回溯逻辑
- ❌ **不引入 Python 依赖**: 纯 stdlib,跟 `eval_trace.py` 一致
- ❌ **不读 NDJSON 内容**: 仅文件名 + mtime,无需解析 JSON
- ❌ **不缓存 trace_dir 列表**: 每次 commit 都重扫,目录小(< 100 文件)开销可忽略

---

## 验证标准(实跑 1 次必须满足)

```bash
# 1. 单测覆盖核心 4 case
python3 -m pytest tests/test_trace_link.py -v
# 期望: 4 passed

# 2. 真 hook 冒烟测试(exec source, 假 trace file)
python3 -c "
import sys, pathlib
from pathlib import Path
trace = Path.home() / '.local/share/x_videos-dowloads-trace/$(date +%Y-%m-%d)'
trace.mkdir(parents=True, exist_ok=True)
fake = trace / 'smoke.ndjson'
fake.write_text('{}\n')
msg_file = Path('tests/.smoke-msg.txt')
msg_file.write_text('feat(test): smoke\n\nIntent: test\n\n')
sys.argv = ['scripts/git_hooks/commit-msg', str(msg_file)]
exec(open('scripts/git_hooks/commit-msg').read(), {'__name__': '__main__', 'sys': sys, '__file__': 'scripts/git_hooks/commit-msg'})
print(msg_file.read_text())
fake.unlink(); msg_file.unlink()
"
# 期望输出含 "Ref-trace: <abs_path>/smoke.ndjson",且在 Intent 之后 / DID 之前

# 3. 源/安装副本同步
cmp scripts/git_hooks/commit-msg .git/hooks/commit-msg
# 期望: 无输出(一致)

# 4. 无 trace 时不阻塞 commit
mkdir -p /tmp/no-trace-test && cd /tmp/no-trace-test && git init -q
# 设 X_VIDEOS_TRACE_ROOT=/nonexistent, 跑 hook → exit 0, commit msg 不变

# 5. 跨日不串
# 把 trace_dir 复制到明天目录,expect 注入明天路径而非今天
```

---

## 相关 memory / dna

- `docs/dna/trace-schema.md` — 5 元组 schema(NDJSON 怎么写)
- `docs/dna/trace-eval-metrics.md` — 4 指标 baseline vs current(本规范上游)
- `MEMORY.md:local-ndjson-trace-instrumentation.md` — instrumentation v1 落地
- `MEMORY.md:trace-eval-pipeline-4-metrics.md` — eval pipeline 落地
- `MEMORY.md:trace-link-commit-injection.md` — Ref-trace 注入的工程血泪(源/副本同步坑)
- `MEMORY.md:claude-skills-symlink-pattern.md` — git 不跟踪的扩展教训
- `~/memory/decisions.md:2026-09-17 — TokenPilot 不安装` — 决策主线(零外部依赖)

---

## 工程纪律价值

- **可观测闭环**: commit ↔ trace 一键跳转,`git log --grep=Ref-trace` 5 秒定位 BUG 复发时的下载链路
- **零认知负担**: 用户 commit 不用关心 Ref-trace 怎么写,全自动
- **可逆 / 可关闭**: trace_link.py 缺失 → 整个阶段 2 no-op,降级到原 2 段流水线
- **git hook 源/副本同步**: 踩坑一次记忆终生(改源必跑 install_git_hooks.sh)
- **复用价值**: 任何 Python 项目 + NDJSON trace + git commit 场景,3 文件(`trace_link.py` / `test_trace_link.py` / hook 注入)即可移植