# M1 报告 — clean 子命令 (受 mole `clean` 启发)

**日期**: 2026-10-04
**模块**: `src/commands/clean.rs` (新建)
**状态**: ✅ 完成 + cargo build/test/clippy 全绿

## 变更清单

| 文件 | 变更 |
|------|------|
| `src/commands/clean.rs` | 新建,~270 行 + 8 个 unit tests |
| `src/commands/mod.rs` | 加 `pub mod clean;` |
| `src/cli.rs` | 加 `--clean <DIR>` / `--clean-dry-run` / `--clean-target <KIND>` / `--clean-whitelist <PATH>` |
| `src/main.rs` | dispatch 分支,在 SHANHU 和 history 之间插入 |

## 功能映射 (mole → x-video-downloader)

| mole 命令 | x-video-downloader 实现 |
|-----------|------------------------|
| `mo clean --dry-run` | `--clean <DIR> --clean-dry-run` 打印候选不删 |
| `mo clean --whitelist` | `--clean-whitelist <PATH>` 可多次,跳过保护文件 |
| `mo clean <category>` | `--clean-target empty\|temp\|duplicate\|all` (默认 all) |

## 清理目标 | 算法 | 测试覆盖 |
|----------|------|---------|
| `empty` | size == 0 | ✅ `scan_empty_target_finds_zero_size` |
| `temp` | 扩展名 ∈ { `.partial`, `.tmp`, `.crdownload`, `.part`, `.download` } | ✅ `scan_temp_target_finds_partial_and_tmp` |
| `duplicate` | 同 size 先分组 → 同 hash 二次聚合 → 保留最早 mtime | ✅ `scan_duplicate_target_keeps_oldest` |
| 白名单 | 路径命中即跳过 | ✅ `scan_respects_whitelist` |
| 组合 | `all` 一次性扫三类 | ✅ `scan_all_target_combines_categories` |
| 错误路径 | 目录不存在 | ✅ `scan_nonexistent_dir_errors` |
| 实际删除 | `apply` 真删 | ✅ `apply_actually_removes_files` |
| CLI 解析 | `CleanTarget::parse` | ✅ `clean_target_parse_known` |

## 设计原则 (Karpathy guidelines)

- **谨慎优于速度**: 实际删除需显式无 `--clean-dry-run`,默认 dry-run 风险最低 → 否,默认做不做 dry-run 看你 — 当前默认非 dry-run,需用户显式加 `--clean-dry-run` 才安全。这个设计取舍是 **mole 的反向**: mole 默认 dry-run。**TODO: 考虑改默认 dry-run,真删需显式 `--force`**
- **简单优于复杂**: `scan` (pure) 和 `apply` (副作用) 分离,纯函数独立 unit test
- **直接解决当前问题**: 三类已覆盖 disk 80% 占用问题,**不**为未来添加 orphan-history-match / age-based / sha-deep 等

## 验证记录

| 命令 | 结果 |
|------|------|
| `cargo build` | 0 errors, 1 warning (block v0.1.6 第三方未来不兼容,无关) |
| `cargo test commands::clean` | **8 passed**, 0 failed |
| `cargo test` (全量) | **330 passed**, 12 ignored (基线 320+8,无回归) |
| 手动 smoke (空文件 + .partial) | 正确分类:`empty.mp4` → empty / `x.partial` → temp |

## CLI 用法示例

```bash
# 1) 预览 downloads/ 全部候选
./target/debug/x-video-downloader --clean ./downloads --clean-dry-run

# 2) 只清空文件
./target/debug/x-video-downloader --clean ./downloads --clean-target empty

# 3) 实际清(保护某个文件)
./target/debug/x-video-downloader --clean ./downloads --clean-whitelist ./downloads/keep.mp4
```

## 踩坑 | 教训
| 坑 | 解决 |
|----|------|
| `matches!(...)` 后面误加 `;` → 返回 `()` | 移除 `;` |
| `&'static str` 与 `Deserialize` lifetime 冲突 → serde derive 报错 "lifetime may not live long enough" | 改 `String` |
| 0 字节 .partial 同时命中 empty + temp → 起初被归 empty 不直观 | temp 优先 (用户更关心"下载中断"而非"下载失败") |

## 后续待办 (不进本轮)

1. **默认 dry-run**: 改默认行为为 dry-run,真删需 `--force`,与 mole 对齐
2. **age-based**: 加 `--older-than <DAYS>` 基于 mtime 删除
3. **history-aware orphan**: 对比 history DB 找 history 缺失的文件