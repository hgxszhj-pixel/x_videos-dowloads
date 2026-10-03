# 2026-09-30 — x-video-downloader：x.com 直链下载策略修复 + 单段跳过合并（Rust 下载器首次真上场）


### 1. 触发

用户给了一条 x.com 视频链接（`https://x.com/nadin_8oyih/status/2105047944844201993`）要求实测下载。真跑 `./target/release/x-video-downloader <url> -o <dir>` 后，日志自相矛盾：`strategy_ytdlp.rs:55` 打印「检测到直接视频URL，使用 **Rust** 下载器」，紧接着 `:74` 打印 `download_strategy = YtDlp`，`downloader/mod.rs:512` 真去调 yt-dlp 子进程。

根因：FxTwitter 返回的是 `video.twimg.com` 直链（`resolver_chain.rs:115-118`），但 `DownloadRequest` 的策略字段是构造函数默认值 `DownloadStrategy::YtDlp`（`types.rs:296` / `:366`），无人改成 `Native` → `downloader/mod.rs:116` 把请求转给 yt-dlp，Rust 直连分支（`parallel_download` + `calculate_backoff` 退避）从未被执行过。

### 2. 交付物

| 交付物 | 路径 | 说明 |
|---|---|---|
| 直链策略修复 | `src/commands/download/strategy_ytdlp.rs:73` | `.with_strategy(DownloadStrategy::Native)`；`:74` 调试日志改打 `request.download_strategy` |
| 单段跳过合并 | `src/downloader/parallel.rs:324-368` | `segments == 1` 时 `rename(part_0.tmp → 输出)`；尺寸不足 / 跨卷 EXDEV 退回 `merge_segments`；日志 `合并完成` → `最终文件` |
| show-me skill（仓库外，不属本仓库） | `~/.hermes/skills/show-me/SKILL.md` | HumanLayer MIT skill，官方 `npx skills add humanlayer/skills --skill show-me -g -y --copy`；本地 2 处补丁（description 123→58 字符、`disable-model-invocation` 落成正文触发门控）；重放说明见 `~/.hermes/skills-staging/show-me/LOCAL-PATCHES.md` |

### 3. 验证（真跑的）

| 项 | 证据 |
|---|---|
| 同一 URL 三次 E2E | 改前 yt-dlp 8.07s（transfer 6.25s）/ Native+合并 10.19s（transfer 6.49s + HEAD 1.35s + 合并 50ms）/ Native 跳合并 6.26s（transfer 3.61s + HEAD 0.95s） |
| 产物一致性 | 三次 sha256 全等 `bc70965a8bf7dc3d…`；`ffprobe` h264 1280x720 + aac、370.033197s、38,718,248 bytes |
| 策略真生效 | 日志出现 `DEBUG: download_strategy = Native`、`开始分段并行下载 … (1 线程)`、`最终文件: 38718248 bytes (期望: 38718248 bytes)`；跳合并版**无** `开始合并` 行 |
| 编译 | `cargo build --release` 6m11s（首次含拉依赖）/ 5m59s |
| 未跑 | 单段跳过合并的两个回退分支（EXDEV、单段不完整）无单测；网络速度未受控，三次耗时不可横向比较 |

### 4. 坑 / 更正

- **我报错过一次并当场更正**：先说「`history.json` 仍是 6 条没变」，那是查了 app-support 下的陈旧副本；真正生效的是 `~/Movies/history.json`（`History::new(&config.output_dir)`，`commands/download/mod.rs:36`），已被三次测试写成 **6 → 9**，新条目插在头部、`file_path` 指向 scratch 目录。
- **两处 config 打架**：`~/.config/x-video-downloader/config.toml` 写 `output_dir = "~/Videos"`，实际生效的是 app-support 那份 `/Users/hgxszhj/Movies`；`-o` 只改输出目录，历史目录不跟着走。
- **进度字节翻倍（已修）**：`parallel.rs:272` 分段任务已通过 `td` 逐块累加（`core/segment.rs:75`），收尾又 `total_downloaded.fetch_add(size)` 一次 → 末行打印 73.8 MB 而文件只有 36.9MB。删掉那次重复累加，并让收尾的 `bytes_downloaded` 直接取 `total_size`（累加器在分段重试时会重复计入，不再用它汇报），超限时 warn。验收脚本实测进度末值 36.9 MB。
- **单段也合并**（已修）：`parallel.rs:325` 原为无条件调 `merge_segments`，1 段 = 整文件再拷一遍；x.com 的网站特征是 `max_threads=1`，所以分段并行在 x.com 上恒等于「单线程 + 一次多余拷贝」。
- Native 路径比 yt-dlp 多一次 HEAD（`get_content_length`）拿文件大小，两次实测 1.35s / 0.95s。
- 网站特征参数（threads / threshold / segments）只有 Native 分支消费，yt-dlp 分支不读 → 日志「应用网站特征」对 yt-dlp 路径是装饰。
- `npx skills add … -g` 一次写进**所有**被检测到的 agent 目录，实测 50 份副本（含 `~/.claude/skills/`），已按「sha256 与上游一致才删」清掉 49 份并 rmdir 空父目录。

### 5. 决策

- 装上游 skill 原样 + 两处最小本地补丁，不整份改写：description 压进 Hermes 的 60 字符上限（上游 123 会被索引截断）；`disable-model-invocation` / `allow_implicit_invocation` 两个字段 Hermes 都不读（grep 只在列举 Claude 侧字段的测试里出现），改用正文一段落表达「只在显式请求时用」。
- 直链分支策略写死 `Native`，不再透传 resolver 默认值。
- 单段跳过合并保留回退：EXDEV 或尺寸不足时走原 `merge_segments`，行为与改前一致。

### 6. 已知 follow-up

- **已提交**：`8829ab8`（`src/commands/download/strategy_ytdlp.rs`，+5/−2）、`6211667`（`src/downloader/parallel.rs`，+34/−10）；两条都用 pathspec 绑定单文件，HANDOFF.md 未随行。
- 进度字节翻倍已修（见第 4 节），`scripts/check_download_artifact.sh` 断言通过。**本轮这两个文件未提交**：`src/downloader/parallel.rs`（进度修复）、`scripts/check_download_artifact.sh`（新增闸门）。
- 单段跳过合并的两个回退分支无单测（要先抽一个可注入 path 的 helper）。
- `~/Movies/history.json` 里 3 条测试记录（`Wataa`）。
- x.com 全链仍硬依赖 yt-dlp 子进程存在（`commands/download/mod.rs:41` 无 yt-dlp 直接 bail）；website_profiles 的并行参数对 yt-dlp 分支无效。

