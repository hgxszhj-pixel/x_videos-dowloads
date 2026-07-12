# Intent Lock + SHA256 契约 — 本地 pre-commit 守卫

本目录包含两个防御性 hook，灵感来自 [spec-superflow](https://github.com/MageByte-Zero/spec-superflow) 的 **Intent Lock** 和 **SHA256 内容级契约** 概念。本地最小化实现，不依赖任何外部插件。

## 为什么需要这两个 hook

**历史教训**：commit `01b0599`（`refactor(hls): remove non-functional adaptive concurrency sampler`）暴露了一个结构性缺陷 — 自适应并发 sampler 实现了 25 行死代码，`downloaded` 计数器从未被 segment 下载更新，但没人拦截。

**根因**：
1. commit message 只说 "refactor"，没说"本次变更只动 X 不动 Y"，scope creep 偷溜进来
2. 即使有 `proposal.md` / `design.md`，执行阶段也不重新读、不校验其哈希

## 两个机制

### 1. Intent Lock（commit-msg hook）

每次 commit message 必须包含一行 `Intent: <一句话>`，否则被 hook 拒绝。

```
<type>(<scope>): <subject>
                            ← 空行（可选 body）
Intent: 移除 adaptive sampler 死代码，不改变下载 API
```

**约束**：
- Intent 长度 10-200 字符
- 太短（如 "fix bug"）→ 拒绝
- 太长（写成段落）→ 拒绝

### 2. SHA256 内容级契约（pre-commit hook）

若项目有 `specs/` 目录，则：
- 计算所有 `specs/**/*.md` 的拼接 SHA256
- 写入项目根目录的 `.contract.sha256`
- commit 时比对：specs/ 改了但 `.contract.sha256` 没同步 → 拒绝

**何时更新契约**：
```bash
# 修改 specs/ 后必须运行：
python3 scripts/spec_contract.py update
```

## 错误消息对照

| 错误 | 原因 | 修复 |
|------|------|------|
| `Missing Intent Lock` | commit message 没 Intent 行 | 加一行 `Intent: <一句话>` |
| `Intent too vague (X chars)` | Intent < 10 字符 | 写清楚做什么、不做什么 |
| `Intent too long (X chars)` | Intent > 200 字符 | 写一句话，不要段落 |
| `.contract.sha256 missing` | specs/ 目录存在但契约未初始化 | `python3 scripts/spec_contract.py update` |
| `specs/ changed but contract stale` | specs/ 改了但未同步契约 | 同上 |

## 紧急绕过

```bash
git commit --no-verify -m "..."
```

仅限以下情况使用：
- pre-commit hook 自身有 bug
- specs/ 改完但 `spec_contract.py update` 失败
- AI 提交验证通过但 hook 误判

## 新克隆后安装 hooks

git 不跟踪 `.git/hooks/` 内容（出于安全），新克隆后需要手动安装：

```bash
# 方案 A: 重装（覆盖现有 hook，谨慎）
cp .git/hooks/pre-commit.sample .git/hooks/pre-commit  # 或参考本 README 重建
chmod +x .git/hooks/pre-commit

# 方案 B: 项目根目录维护一份 hooks/ 目录并用 git 跟踪
# （本次未做，因为会污染 git tree，可作为后续改进）
```

**当前实现**：hooks 直接在 `.git/hooks/`，git 不会持久化。备份请复制 `scripts/intent_lock.py` + `scripts/spec_contract.py` + 本 README，重新拼装 `.git/hooks/pre-commit` 和 `.git/hooks/commit-msg`。

## 不做的事

- **不检测 diff vs Intent 不一致** — 那是 human review 责任，hook 只强制 Intent 字段存在
- **不强制 specs/ 目录存在** — 若项目无 specs 流程，spec_contract.py 静默跳过
- **不替换 OpenSpec / Superpowers / spec-superflow** — 它们仍是更完整的方案；本 hook 是最小防御层

## 工作量

- intent_lock.py: 30 分钟
- spec_contract.py: 20 分钟
- pre-commit + commit-msg hook: 30 分钟
- README: 20 分钟
- 验证: 20 分钟

**总计：~2 小时**，单 commit（`chore(hooks): add Intent Lock + SHA256 pre-commit guard`）。