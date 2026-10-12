# 2026-10-08 — the-scribble-thing skill 接入 + 2 demo 验证 + Toutiao 转录 + cron 排队等额

## TL;DR

本月 Toutiao 视频（https://www.toutiao.com/video/7694092120226939402/）后续调研收尾：whisper 转录月亮IT 完整文稿 → 安装
the-scribble-thing skill 到本机 → 2 次成功 demo（house+tree+sun 演示基础；feedback loop 演示复杂图）
→ 第 3 次提交（4 工具对比图，对应视频实质内容）撞 daily free quota 429 → 排 cron `b30767c8` 在
2026-10-09 08:00 Beijing（= UTC 00:00，quota reset）重试。

## 流程与命令

### 1. 视频转录
- 视频 33.9MB → audio.wav 11.5MB（ffmpeg -ac 1 -ar 16000）
- whisper-cli -m ~/.cache/whisper/ggml-small.bin -f audio.wav -l zh -otxt -t 4 --no-timestamps
- 6:46 wall，5.1KB txt，**保存到 `$CLAUDE_JOB_DIR/tmp/toutiao_transcript.txt`**（前一轮 session 未落盘，导致本轮需要重转）
- 教训：转录结果立即 cp 到固定路径，避免 session 重启后丢失

### 2. 视频实质（摘要）
月亮IT 对比 4 种 AI 视频工具：
1. **花生AI**（B站）— 文字/音频 + 真实影像 + Remotion 图表叠加；~30元/10min
2. **work body + Remotion** — 文章 → 视频；结构图 MECE/流程/阶段；**性价比最高**
3. **AI 短剧工具**（厂商名不公开）— 双人访谈风；50元 + 3h；**不推荐**
4. **手绘风格工具** — 跟着文案说话动态手绘；适合概念讲解
   结论：不要真人影像 → work body + Remotion；要真人影像 → 花生AI。AI 短剧成本-效果差。手绘工具适合解释类。

### 3. the-scribble-thing skill 安装
- 公开 skill discovery: `https://scribble.boringstuff.club/.well-known/agent-skills/index.json`
- 验证 zip: sha256 `c4631c9d19f874147d199815d9e5d485165ddc3b8cb9e714d8e9bb0d650f3a16` ✓
- 提取到 `~/.agents/skills/the-scribble-thing/`（SKILL.md 2.7K + agents/openai.yaml）
- 软链到 `~/.claude/skills/the-scribble-thing`（与项目已有 ~20 个 skill 一致的 symlink 模式）
- 三个 demo 文件 `feedback_loop.png` / `scale_pipeline.png` / `tools_compare.png` 全部用 PIL 画
- `feedback_loop.png` 的 `order_text` 被服务完全尊重（标题 → 框 → 标签 → 箭头 → 中心文字）

### 4. REST API 经验
- `POST /agent/v1/animations`：multipart + `Idempotency-Key` + `accept_retention/accept_terms/terms_version`
  必填；可选 `order_text` / `show_hand` / `speed`
- 初始响应 202 + `access_token`（per-animation bearer，**仅用于该 job**）
- 轮询 `GET /agent/v1/animations/{id}` 间隔 3-5s
- demo1 40s（house+tree+sun），demo2 40s（feedback loop 11 步顺序）
- demo3 撞 429 `daily_limit` —— 2 次成功已用完当日 free quota
- 24h 保留期，可在 `manage_url` 用 $5 买解锁（clean MP4）

### 5. Cron 排队
- 任务 ID: `b30767c8`（CronCreate one-shot, session-only, 死后失效）
- 触发时间: `0 8 9 10 *` = 2026-10-09 08:00 Beijing = 2026-10-09 00:00 UTC（quota reset）
- 脚本: `$CLAUDE_JOB_DIR/tmp/scribble_demo/run_tools_compare.sh`（4.3K，bash -n 已过）
- 脚本能力: 复用 `job3.env` 中的 ID/TOK 跳过重复提交；poll 60 次 × 5s；下载 `preview3.mp4`；
  抽 8 帧到 `frames3/`；拼成 4×2 sprite `sprite_tools_compare.png`
- 风险: session-only cron，**如 Claude 进程在 08:00 前退出则不触发**

## 产物清单

| 文件 | 大小 | 备注 |
|---|---|---|
| `$CLAUDE_JOB_DIR/tmp/toutiao_extract/video.mp4` | 33.9M | 原始视频 |
| `$CLAUDE_JOB_DIR/tmp/toutiao_extract/audio.wav` | 11.5M | 16kHz mono |
| `$CLAUDE_JOB_DIR/tmp/toutiao_transcript.txt` | 5.1K | **月亮IT 文稿永久存档** |
| `$CLAUDE_JOB_DIR/tmp/scribble_demo/preview.mp4` | 1.5M | demo1（house+sun+tree）36s 798² |
| `$CLAUDE_JOB_DIR/tmp/scribble_demo/preview2.mp4` | 1.8M | demo2（feedback loop）36s 898² |
| `$CLAUDE_JOB_DIR/tmp/scribble_demo/scale_pipeline.png` | 14K | demo3 早版（脚本→线稿→视频） |
| `$CLAUDE_JOB_DIR/tmp/scribble_demo/tools_compare.png` | 33K | demo3 终版（4 工具对比） |
| `$CLAUDE_JOB_DIR/tmp/scribble_demo/sprite_sheet_small.png` | 178K | demo2 12 帧 4×3 sprite（看动画用） |
| `$CLAUDE_JOB_DIR/tmp/scribble_demo/run_tools_compare.sh` | 4.3K | cron 脚本 |
| `~/.agents/skills/the-scribble-thing/SKILL.md` | 2.7K | 装好的 skill |
| `~/.claude/skills/the-scribble-thing` | symlink | 触发 Skill 工具发现 |

## 教训与建议

- **transcript 必须落盘**：本 session 一上来就踩这个坑（前一轮 transcript 没存档，重转花了 6:46 wall）
- **whisper x86 CPU 默认行为**：默认 thread 数全核跑效率反而差（35min CPU at 357% 不出活）；`-t 4` 6:46 出结果
- **daily free quota 是真限制**：2 次成功后第 3 次必 429；不存侥幸，重试 1 次足矣
- **Idempotency-Key 必须 unique**：同 key + 不同 input 返 409；同 key + 同 input 返 200 + `idempotent_replay:true`
- **bear capability 不可外泄**：access_token 仅对该 animation 有效，贴日志/聊天=泄露能力

## 决策 / 跨项目沉淀

- Rule 12 web-fallback 链外：the-scribble-thing 这种"line-art + 第三方云服务"模式是否可纳入
  `~/.claude/CLAUDE.md` "Web 抓取 fallback 链"？当前不在链上（链只到 chrome）。
  建议：**不纳入**——scribble 是创意服务，不是抓取服务；和抓取场景正交。
- 是否要把 `the-scribble-thing` 加到 `local_skill_sync.config.json` 的 `repos`？
  答：**不必**——它是 registry-published（discovery index 有 sha256），用 `npx skills` 装或
  直链下都行；不像 gitee 那几个是私有项目。
