# 2026-10-30 — 声明式验收（soul checks）+ 转正结论导出（soul export）

> 承接 [2026-10-29](2026-10-29-lan-+-isord-递归-+-复习装载-+-ffd-工具.md)。**本轮 0 commit（除本文件与 AGENTS/CLAUDE 的落地块）**。

## 一、起因

头条稿《CMU 开源 Harness 层 RSI 框架》讲的是 **Hermit**（仓库 `Infini-AI-Lab/hermit`，360★，Apache-2.0，v0.1.0）。
它把「让 Agent 自己改自己」做成可比的实验台：外部可训练 Adapter + 统一评估器 + 多算法插拔 + `deploy --host codex` 生成 AGENTS.md。
我们自己的 `soul.py` 是同一件事的轻量版，缺的正是它的三块接口：统一评估器、算法插拔、deploy 出口。本轮先补前两块里的「统一评估器」和 deploy 出口。

## 二、声明式验收（`soul.py checks`）

- skill 在 frontmatter 里声明自己的验收项，`soul` 直接读、直接跑，改 skill 时不再每次手写尺子：

```yaml
metadata:
  hermes:
    checks:
      - name: ops-and-project
        argv: [python3, scripts/check_ops.py]
        cwd: .            # . = 在 compare 的对照副本里跑（读得到候选版 SKILL.md）；其余写绝对路径
```

- 新增：`declared_checks()` / `checks_command()` / `soul.py checks --target … [--run]`；`compare` 与 `propose` 的 `--check` 变成可选，**省略就只认声明**。
- 三个 skill 各自落了尺子：`exam-lookup-ops/scripts/check_ops.py`（局域网准入闸 + 失效卡出口 + by_bank → 再跑项目侧 webui/plugins selftest）、`local-data-browser-ui/scripts/check_ui.py`（自我递归守卫 + 六个 tab + pre-wrap）、`soul-evolve/scripts/check_evolve.py`（纪律 + soul 自检）。
- 三次改动全部走新机制、**没有传 `--check`**：`cand-dc18e6922b` / `cand-0c081ed425` / `cand-8d00c12e85`，均 `base=[1,1] cand=[0,0]` → promote。
- 每日自检 cron 升级：跑所有声明了 checks 的 skill，**只报红的**；全绿时零输出（安静模式不变）。
- `check_skill.py` 修一处误报：文档里写 `scripts/x.py` 时它只在 skill 根目录找 → 现在也认 `scripts/references/templates/assets`。

## 三、导出出口（`soul.py export`）

- `soul.py export --out AGENTS.md`：把**已转正**的候选落成一段可粘贴的片段，只搬「哪条 skill / 什么约定 / 哪天转正」，规则正文留在 skill 里。
- **幂等**：只替换 `<!-- soul:begin … -->` 与 `<!-- soul:end -->` 之间的内容，块外文字一个字不动（重跑 `changed:false`）。
- 滤噪：早年 demo 那种只有 `4` / `1` 的占位 lesson 不导出，重复条目去重（14 条原始 → 8 条有效）。
- 落地：`~/x_videos-dowloads/AGENTS.md` 尾部追加 13 行方块（32 → 45 行）；`CLAUDE.md` 头部加 `@AGENTS.md`（Claude Code 只读 CLAUDE.md，靠这行吃到同一份）。

## 四、自检

- `soul.py selftest`：新增「声明式 checks 解析」（临时 frontmatter → 两项、`cwd .` 原样、绝对路径加 `cd`、缺文件返回空）与「export 幂等」（块外两行保留、旧块消失、重跑 `changed:false`）→ 全过。
- 三个 skill 的 `soul.py checks --run` 全绿；每日自检静默；`check_skill --all` 95 个 skill 0 不合格 / 34 提示。
- `soul-evolve` 1.2.3 → **1.4.0**（两次转正：声明式 checks、export 用法）。

## 五、未闭合

- 多算法插拔（`--strategy gepa|skillopt|…`）没做：现有节奏是「一次改一条、人盯结果」，接批量进化需要一轮跑 N 次评估的预算，等真要批量再说。
- `CLAUDE.md` 工作区里还有上一轮别人留下的未提交改动（`cargo build` 块换成 `cargo verify` 指引），不是本轮产生，提交时一并带上并在 body 里注明。
- 技能目录并发写仍无锁：soul 的 `base_sha` 能挡「基线已变」，挡不住两个会话各自读→各自写。

## 六、refs

- 代码：`~/.hermes/tools/soul/soul.py`（`declared_checks`/`checks_command`/`export`/`checks` 子命令/两处 selftest）
- 尺子：`~/.hermes/skills/{exam-lookup-ops,local-data-browser-ui,soul-evolve}/scripts/check_*.py`
- 产物：`~/.hermes/soul/AGENTS.generated.md`、`~/x_videos-dowloads/AGENTS.md`
- 上游参考：`https://github.com/Infini-AI-Lab/hermit`（只借鉴接口形状，未安装）
