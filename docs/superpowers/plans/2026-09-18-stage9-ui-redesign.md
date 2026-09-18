# 阶段 9：界面改版（经典 HIG · 左右布局 · 跟随系统深浅）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把「能用但简陋」的界面重做成有设计语言的桌面应用：**左侧栏 + 主网格区**、**跟随系统深浅自动切换**、
信息层次清楚（资料库/导入/维护分区）、控件有统一的交互态与反馈（toast/确认框/空状态/进度）。

**Architecture:** 不引任何 UI 库，纯原生 Vue 3 + CSS。设计令牌集中在 `src/styles/app.css`
（语义色变量 + 组件类），主题用 `prefers-color-scheme` 自动切换。图标用内联 SVG 组件 `AppIcon.vue`。
`App.vue` 负责壳与编排，`FilterBar.vue` 变为工具栏内联控件，`PhotoGrid.vue` 只做质感升级。

**Tech Stack:** Vue 3 + TypeScript + 原生 CSS（无第三方 UI 依赖）

**依据：** 全局 skill `apple-hig`（`~\.agents\skills\apple-hig`，快照 `apple-hig-2026-06-09`，
标注 “Updated for OS 27”）。**按用户决定排除 iOS 26+ 的 Liquid Glass**，只取经典 HIG：
系统语义色、字号层级、8pt 栅格、同心圆角、标准材质（毛玻璃）、44pt 命中区、Reduce Motion。

---

## 已确认决策（2026-09-18）

| # | 议题 | 决策 |
|---|---|---|
| 1 | UI 实现方式 | **原生 Vue + CSS 手搓**，不引 UI 库（曾试装 naive-ui 后撤回） |
| 2 | 布局 | **左右布局**：左侧栏（资料库/导入/维护）+ 主区（工具栏 + 网格） |
| 3 | 主题 | **跟随系统** `prefers-color-scheme`，**不做**应用内主题开关（HIG `dark-mode.md`） |
| 4 | iOS 26+ | **排除 Liquid Glass**：跳过 `materials.md` 的 Liquid Glass 章节、`color.md` 的 Liquid Glass Color、`layout.md`/`sidebars.md`/`tab-bars.md` 的浮层、`app-icons.md` 的 Icon Composer。只保留 Standard Materials。 |
| 5 | 图标 | 内联 SVG（`AppIcon.vue`），不引图标库 |
| 6 | 参考稿 | `docs/mockups/` 静态 HTML；最终采用 `d-hig-auto.html` |

---

## 实施结果（已完成）

| 文件 | 说明 |
|---|---|
| `src/styles/app.css` | **新增**：HIG 令牌（浅/深两套语义色）、基础重置、按钮/分段/芯片/输入/开关/进度/卡片/滚动条/焦点环/Reduce Motion |
| `src/components/AppIcon.vue` | **新增**：内联 SVG 图标（search/folder/iphone/refresh/image/shield/doc/trash/export/check/x/stop/warn） |
| `src/App.vue` | **重写**：侧栏（品牌/资料库卡+统计/导入+进度/维护）+ 主区（工具栏/网格/空状态）+ 选中操作条 + toast + 自绘确认框；替换 `window.confirm` |
| `src/components/FilterBar.vue` | **重写**：改为工具栏内联控件（搜索框/分段控件/日期/完整性芯片/分段粒度/清空） |
| `src/components/PhotoGrid.vue` | 瓦片改圆角 12、悬停微放大 + 阴影、选中环 + 24px 勾选徽标；日期头加张数、配色随主题 |
| `src/lib/gridLayout.ts` | 日期头行增加 `count`（显示「· N 张」） |
| `src/main.ts` | 引入 `./styles/app.css` |
| `docs/mockups/` | 静态参考稿 A/B/C/D + `index.html`（未引入构建） |

参考稿：方案 D 的浅/深两套配色即最终稿；`prefers-color-scheme` 下自动切换。

## 验证

- [x] `npx vue-tsc --noEmit` 通过
- [x] `npm run build` 通过（CSS ~10.3 kB / JS ~96 kB / gzip 37.5 kB）
- [x] `npm run tauri dev` 实机启动，用户确认「看着还不错」
- [x] 未新增任何运行期依赖（`package.json` 仅 vue/pinia/@tauri-apps）

## 未做 / 后续

- 日期头真正「sticky」：当前随内容滚动（虚拟行绝对定位，做吸附成本高），需要时再单独做
- 单击看大图（设计 §7.1「大图」）：仍为后续项
- 首次引导向导、WebView2/SmartScreen 说明：在 README/打包计划内
- 深浅以外的外观设置：按 HIG 不提供（跟随系统）

## 自查记录

**规格覆盖：** 设计 §9 浏览界面观感、§8.1（唯一 `<video>` 未动）、§7.3（悬停抑制未动）。
**对既有实现的承接：** 网格虚拟化与悬停预览逻辑未改，只改样式与壳；命令/存储层不受影响。
**已知取舍：** 桌面 WebView2 上毛玻璃有一定性能成本，但只用于工具栏/侧栏/浮层，内容层不使用。
