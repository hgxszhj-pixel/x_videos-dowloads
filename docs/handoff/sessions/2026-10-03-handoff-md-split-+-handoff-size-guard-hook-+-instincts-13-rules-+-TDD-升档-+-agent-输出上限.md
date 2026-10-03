# 2026-10-03 — HANDOFF.md 拆分 + size-guard hook + 13 Rules 升级 + TDD 升档 + agent 输出上限

## 触发

「下次如果 update handoff.md 会不会还会出现原来的问题呀?」— 单文件 2117 行拆分只解决了 HANDOFF.md 自身,但 `docs/handoff/sessions/` 目录仍可无界膨胀,且用户全局 workflow instincts 仍停留在 11 Rules 时代。

## Deliverables

### 1. HANDOFF.md 拆分 (`1b1c5ac`)

- 单文件 2117 行 → ~70 行索引 + 21 个独立 session 文件
- 96% 行数下降

### 2. DNA「必读 → 按档 grep」(`9ef972d`)

- `docs/dna/三档地盘.md` 等 3 份改为按档 grep,常规改动直接动键盘
- 经验证 rexxi SKILL.md 4 条工程纪律逐条对应 DNA — 五派三条规则是已独立发现的最佳实践的正式命名

### 3. handoff_size_guard hook (`1f6be17`)

- 新增 `scripts/handoff_size_guard.py`: staged 文件 > 200 行 hard reject,> 100 行 soft warn
- 关键设计: 用 `git diff --cached` 检查 **staged** version,匹配 `git commit` 实际写入
- 接入点: Intent Lock 之后 / DID 之前 — 拒绝时不消耗 DID credit
- 端到端验证: 80 行 silent pass / 151 行 warn+pass / 355 行 hard reject

### 4. 用户全局 instincts.md (13 Rules)

| # | 变更 | Why |
|---|------|-----|
| 1b workflow-light-plan | + **自决策白名单** (8 项) + **暂停 input 条件** (4 项) | 长篇前置 Spec/Plan 低效(参见「Plan 模式已死」),模型需知道「何时自决 vs 何时问」 |
| 6 workflow-tdd-red-green | confidence 0.8 → **0.9** | 测试绿灯优先于文档审查(文章第 3 策略) |
| 13 workflow-agent-output-bounded (新) | confidence 0.85 | 用户面摘要 ≤ 5 bullet,详细落文件;例外:code diff / 错误信息 / API 文档 / 用户明确要求详细 |

### 5. 用户全局 slash-commands.md

- `/spec` → `/goal` 重命名: 一句话目标 + 验收标准,不写完整规约

## 新 Memory Files

- `handoff-sessions-quarterly-archive.md`: > 90 天 sessions 文件季度归档到 `archive/YYYY-Qx/`,与 size-guard hook 协同防目录无界膨胀
- `handoff-size-guard-hook.md`: hook 设计 + 阈值 + 测试路径

## 验证

- Hook 端到端 3 案例: `80 silent PASS` / `151 WARN+PASS` / `355 REJECT`
- Hook 接入位置正确: 拒绝时不消耗 DID credit(Intent Lock 在前)
- 季度归档 cadence: 2026-Q4 → 2027-Q1,> 90 天文件 mv

## Pits

- 早期用 lefthook 在 tmp repo 跑 hook 测试,lefthook 未装 — 切到直接函数测试
- 软警告 151 行测试意外产生 `5f8e1cc` commit — `git reset --soft HEAD~1` + restore HANDOFF.md

## Decisions

详见 memory 文件 `handoff-size-guard-hook.md` + `handoff-sessions-quarterly-archive.md`。
