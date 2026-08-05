# 已归档设计文档

> 本目录收录**已完成或被取代**的设计文档，仅作历史参考。

## 归档原则

- 文档描述的功能已上线，且有 commit/代码佐证（如 `slint-to-dioxus-migration.md` → Dioxus 已全面替换 Slint）
- 文档被新版完全取代（如 `performance-bottleneck-report-v1/v2.md` 被 v3 取代）
- 候选方案已落幕（如 `2026-05-13-solution-{a,b,c}` 中最终采纳 b 之外的 a/c 已无需查阅）
- 旧版 plan 被新版取代（如 `gui-improvement-plan.md` 被 v2 取代）

## 归档时间

2026-08-05 — 一次清理，避免 `docs/plans/` 误导新协作者以为"待实施"。

## 当前生效文档

活跃列表见 `../`（排除本目录）：

- `2026-07-27-tuling-course-download-pool-design.md` — ✅ 已实施
- `2026-06-06-filename-alias-design.md` — sanitize_filename 已落地（`src/types.rs:643`）
- `performance-improvement-plan.md` — 仍有未结评审项（Architecture/Performance NEEDS_REWORK/IMPROVEMENT），保留备查
- `2026-05-21-improvement-plan.md` — 项目早期改进清单，大部分已无对应
- `2026-05-25-performance-bottleneck-report-v3.md` — 最新一份性能报告
- 其余 `performance-*.md` 为细分专题报告