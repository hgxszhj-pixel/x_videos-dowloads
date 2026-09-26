# Practice Page Overrides

> **PROJECT:** exam-lookup
> **Generated:** 2026-09-22 16:13:31
> **Page Type:** General

> ⚠️ **IMPORTANT:** Rules in this file **override** the Master file (`design-system/MASTER.md`).
> Only deviations from the Master are documented here. For all other rules, refer to the Master.

---

## Page-Specific Rules

### Layout Overrides

- **Max Width:** 1200px (standard)
- **Layout:** Full-width sections, centered content

### Spacing Overrides

- No overrides — use Master spacing

### Typography Overrides

- No overrides — use Master typography

### Color Overrides

- No overrides — use Master colors

### Component Overrides

- Avoid: No indication of progress
- Avoid: Unoptimized full-size images
- Avoid: No visual feedback on current location

---

## Page-Specific Components

- No unique components for this page

---

## Recommendations

- Effects: Haptic feedback (vibration), voice guidance, focus indicators (4px+ ring), motion options, alt content, semantic
- Feedback: Step indicators or progress bar
- Performance: Use appropriate size and format (WebP)
- Navigation: Highlight active nav item with color/underline


---

## 本项目取舍（人手补写，覆盖上面自动生成的建议）

MASTER.md 是 `ui-ux-pro-max` 的 `--design-system --persist` 生成的通用建议。落到这个单文件、
零依赖、中文、键盘驱动的本地工具上，逐条对齐如下：

| 建议 | 采纳 | 说明 |
|---|---|---|
| 风格：极简 / 瑞士网格 | ✅ | 中性色阶 + 1px 描边 + 12px 圆角网格，无渐变无装饰 |
| 主色 #475569 + CTA 蓝 #2563EB | ⚠️ 改 | 用户点名以 shadcn/ui 为准：主按钮＝高对比中性色（深色下白底黑字、浅色下黑底白字），蓝色不作 CTA |
| 边框 #E2E8F0 / 浅底 #F8FAFC | ✅ 近似 | 取 shadcn 的 `#e4e4e7` / `#ffffff`（同一档中性阶，避免两套灰） |
| Ring #475569 | ❌ 改 | 焦点环用 `#d4d4d8`（深）`#a1a1aa`（浅）：焦点环要「看得见但不抢主色」 |
| 字体 Atkinson Hyperlegible | ❌ 不适用 | 它只覆盖拉丁字母；中文界面走系统 `PingFang SC / Microsoft YaHei`，装 webfont 与「离线单文件」冲突 |
| 动效：滚动揭示 300–400ms | ❌ 不适用 | 这是高密度工具页，无滚动叙事；改为「状态切换 100ms / hover 150ms / 浮层 180ms」，且只动 transform、opacity |
| 交付清单：不许用 emoji 当图标 | ✅ | ✅/❌/⏱/⚠ 全换成内联 SVG（`.ico`），跨系统字形不再跳 |
| 对比度 ≥4.5:1 | ✅ 实测 | 浅色次要文字 `#71717a` 在徽标底 `#f4f4f5` 上只有 **4.40** → 提到 `#6b6b74` = **4.80**；其余 11 组实测 5.02–19.90 |
| 关键焦点可见 / 尊重 reduced-motion / 触控 cursor | ✅ | `:focus-visible` 2px 环、`@media (prefers-reduced-motion: reduce)` 全关、按钮 `cursor:pointer` + `touch-action:manipulation` |
| 避免：导航不清、没有搜索 | ✅ | 顶栏就两个页签；查题页本身就是搜索 |

对不上的两个字：这套建议把产品当成「FAQ/文档落地页」，我们更像「练习台」——
落地页那套（hero + 分类卡片 + 折叠 FAQ）没有采纳，只取它的风格/色/对比度/清单部分。
