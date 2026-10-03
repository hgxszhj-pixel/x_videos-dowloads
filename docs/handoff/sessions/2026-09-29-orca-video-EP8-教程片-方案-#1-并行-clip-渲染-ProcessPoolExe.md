# 2026-09-29 — orca-video EP8 教程片：方案 #1 并行 clip 渲染 (ProcessPoolExecutor) + EP8 v3 A/B 重渲（进行中）


### 1. 触发
EP8 v2 串行版 1313.07s / 84.7 MB / 30 场景 / workers=1（=21.9 min）是基线，肉眼痛：clip 渲染占绝大多数壁时且无共享状态可并行。
方案 #1 = `ProcessPoolExecutor` 包 `build_video.py` 的 `for scene in scenes` 循环；烟测 `/tmp/bv_smoke.py` 4 clip × 2 档：`workers=1 wall=27.7s (6.93s/clip)` → `workers=4 wall=15.4s (3.86s/clip)`，**1.80×**。用户「重渲 EP8 作为 A/B (Recommended)」 → bg `b82ms2k3m` 起 `python3.10-intel64 -u build_video.py --scenes scenes_train_ep8.py --name "EP8_v3_设置·隐私·排错_1080p" --workers 6 --force-clips`。
边界：只动 `build_video.py` 的并行骨架；不动 clip 内容、配音、配色、转场、混音（v3 期望产物时长 ~1313s = v2 同源场景）。

### 2. 交付物（都在 `~/video-doc/orca-video/`，另一棵树、未提交 git）

| 交付物 | 路径 | 说明 |
|---|---|---|
| 并行骨架（surgical） | `build_video.py` `_render_scene_in_worker` (line 49–58) | 入口首句 `S.SHOT_DIR = job["shot_dir"]` — macOS `spawn` 模式下 worker 全量 re-import，模块全局不传，必须显式带 |
| Job dict（surgical） | `build_video.py` (line 397–409) | 增 `shot_dir=SHOTS` 一项 |
| argparse | `build_video.py` (line 288 旁) | `--workers N`（默认 1，保持向后兼容） + `--force-clips` 复用既有 `--force` 语义 |
| 烟测脚本 | `/tmp/bv_smoke.py` | 4 个非镜头场景 × 2 档 workers（1/4），输出 wall + per-clip wall |
| 烟测日志 | `/tmp/smoke.log` | 记录 workers=1 vs workers=4 两轮 wall + 文件清单 |
| EP8 v3（bg 进行中） | `out/train/EP8_v3_设置·隐私·排错_1080p.mp4` | 待 v3 落地，期望 ~1313s / 壁时 8–12 min |

### 3. 验证（真跑的）
- 烟测 wall：workers=1 → 27.7s（6.93/clip），workers=4 → 15.4s（3.86/clip），**1.80× speedup**，实测单进程 CPU 100% × 4 core 满。
- BPv3 bg PID 93273 alive（11:22 下午起，TTS 30 条全部命中缓存，总 TTS 阶段仅 ~30s）。
- v3 阶段进度：TTS 全 30 命中缓存 + `[plan] 30 场景, 成片 1313.1s (21.9 分钟)` + `[trans] circleclose×1 ... 硬切占比 55%` + clip 阶段 8/30（at +1m wall）→ 并行渲染进行中。
- `python3.10-intel64 -c "import numpy, requests, PIL"` → numpy 1.23.3 / requests 2.28.1 / PIL 10.4.0 全在，确认 v3 解释器无 BGM step 崩溃前提。

### 4. 真坑（都有实证）
1. **`bgm_segmented → synth_bgm → import numpy` 在 pyenv 3.14 直接 `ModuleNotFoundError`**（`lib/pipeline.py:171`）。根因：numpy 1.23.3 只在 `/usr/local/bin/python3.10-intel64` 的 site-packages；`/usr/local/bin/python3` = pyenv 3.14，无 numpy。v2 用 3.10-intel64、v3 第一跑用了 3.14 → BGM step 崩。**修法**：脚本 shebang/wrapper 显式用 3.10-intel64，本机 zsh PATH 自动选 pyenv 3.14 是坑。
2. **Python stdout 走 `> log 2>&1` 时块缓冲，进程消失无声**：bg task `b8cwpx2od` 死时 log 空、EXIT 空 → 完全看不出在哪步。**修法**：`python -u`（unbuffered） + `tee` 替代 `>`，实时可见。
3. **僵尸 Python 持文件 → `rm -rf .work/ep8_v3` 「Directory not empty」**：zombie procs 87614 / 87615 仍持 `slides/*.png`。**修法**：`kill -9 PID PID` → 再 `rm -rf` 成功；**先 `--** 再 rm** 是死规矩**。
4. **`spawn` 模式（macOS 默认）worker 全量 re-import，模块全局丢失**：第一次漏传 `lib.slides.SHOT_DIR` → 第二次工作路径正确但 `dist` 步 write png 到默认 cwd → 污染根目录。**修法**：必须显式把 `SHOT_DIR` 通过 job dict 传入 worker 入口首句重设（这是 `process.start_method == 'spawn'` 的硬要求）。
5. **`clip_for(force=True)` 永远 re-render**：本项目 clip 阶段没有 per-clip skip，v3 = `--force-clips` 必走全量；好处 = A/B 干净，坏处 = 任何参数变化都吃全量重渲。

### 5. 决策
- **仅 clip 阶段并行**：TTS 全缓存命中 ~30s 单进程足以；concat+bgm+encode 顺序强（`xfade` `filter_complex` 必须按时序），无差别；`BgmSegmentedWorker` 串行更稳。
- **不预编译/不缓存跨 worker 的 ImageFont**：每个 worker 各自打开 `Theme` 自带路径绑定，避免 fork 后字体对象失效。
- **解释器约束**：wrapper 一律 `/usr/local/bin/python3.10-intel64`（pyenv 3.14 缺 numpy + 部分 C 扩展），加 `python -u` 防止缓冲黑洞。
- **GPU 加速已在 concat 链路**（既有代码，本轮未改）：`lib/pipeline.py:434` 全片 concat 编码走 `hevc_videotoolbox` (macOS HW HEVC)，同函数注释 (line 351–352) 注明 **VideoToolbox 解码 + Metal/VideoComposition 做 dissolve + hevc_videotoolbox 编码**；`lib/pipeline.py:559` 成片烧字幕走 `h264_videotoolbox`（1080p 静态画面与 libx264 ultrafast 速度相当，选 HW 节能）。clip 渲染仍 CPU Pillow + libx264 软件编码；TTS/BGM 无关。净效果 = concat+bgm+encode 阶段 ~5–8× 加速 vs libx264，也解释了当前 bg 卡在 ffmpeg 是合理的（30 clip × xfade VideoToolbox 链路）。

### 6. 已知 follow-up
- **v3 实际壁时待定**：bg `b82ms2k3m` 仍在跑，clip 阶段 **30/30 完成**（最后 `e5-99_50.19s`，log mtime 23:28:45 = ~7 min 前最后一次写入），现进入 **concat+bgm+encode** 阶段（`filter_complex` 30 clip × 29 xfade，ffmpeg 静默跑是常态）；预计 +3–6 min 出 `[OK]` + `EP8_v3_*.mp4` → stat size + tail `[OK]` 拿真实 A/B → mark Task #13 completed。最新监控见 `/private/tmp/claude-501/-Users-hgxszhj-x-videos-dowloads/38daa949-19a3-462b-bde0-fe81725f6836/tasks/b82ms2k3m.output`。
- **`b82ms2k3m` 输出**：实时 `/private/tmp/claude-501/-Users-hgxszhj-x-videos-dowloads/38daa949-19a3-462b-bde0-fe81725f6836/tasks/b82ms2k3m.output`。
- **v3 完成后**：若 A/B 1.5×+ → 留作方案 #1 基线，进 `tasks/方案#1-EP8-并行.md`；若 < 1.2× → 看是否受 GIL/CPU 限频，反推是否方案 #2 (subprocess 拆分)。
- **未提交 git**：HANDOFF + 全套改动（与上轮一致）。

---

