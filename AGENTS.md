# LivePorter —— 项目说明（供 AI 助手阅读）

## 这是什么

把 iPhone 上的**实况照片**（Live Photo = HEIC/JPG 静态图 + 同名 MOV 短视频）**配对无损地**批量搬运到 Windows 硬盘，并提供类似 iOS 相册的浏览界面（网格、悬停播放动态、Ctrl+滚轮缩放）。

- 平台：**仅 Windows**（macOS 有系统方案，本工具专攻 Windows 这个空白）
- 形态：Tauri 2（Rust 核心 + WebView2 前端）+ Vue 3 + TypeScript
- 分发：完全开源、免费
- 仓库：`git@github.com:Raven2049/livephoto-manager.git`

## 文档索引

| 文档 | 位置 | 说明 |
|---|---|---|
| 设计文档 | `docs/superpowers/specs/2026-09-17-liveporter-design.md` | **唯一真相**。19 项决策、数据模型、管道设计、风险登记都在这里。动手前先读它 |
| 计划 1 | `docs/superpowers/plans/2026-09-17-stage0-wpd-probe.md` | 阶段 0 只读探测程序（10 个任务，已完成） |
| 计划 2 | `docs/superpowers/plans/2026-09-17-stage1-tauri-skeleton.md` | 阶段 1：Tauri 骨架 + `lpm://` 协议 + 虚拟滚动网格（10 个任务，**已完成**） |
| 性能记录 | `docs/superpowers/notes/2026-09-17-grid-perf.md` | 阶段 1 网格性能实测（含两处必要修正） |
| 计划 3 | `docs/superpowers/plans/2026-09-17-stage2-index-library.md` | 阶段 2：SQLite 索引 + 库目录管理（10 个任务，**已完成**） |
| 索引实测 | `docs/superpowers/notes/2026-09-17-index-smoke.md` | 阶段 2 索引冒烟实测 |
| 计划 4 | `docs/superpowers/plans/2026-09-17-stage3-device-import.md` | 阶段 3：设备导入（WPD 传输 + 状态机 + 断点续传）（**已完成，带已知残留**） |
| 导入实测 | `docs/superpowers/notes/2026-09-17-import-smoke.md` | 阶段 3 实机导入实测（含一个严重 bug 的发现与修复） |
| 计划 5 | `docs/superpowers/plans/2026-09-17-stage4-thumbnails.md` | 阶段 4：ffmpeg 集成 + 缩略图（8 个任务，**已完成**） |
| 缩略图实测 | `docs/superpowers/notes/2026-09-17-thumbs-smoke.md` | 阶段 4 缩略图实测（含 `VT_DATE` 与 `-filter_complex` 两个修复） |
| 计划 6 | `docs/superpowers/plans/2026-09-18-stage5-previews.md` | 阶段 5：预览片 + 悬停播放（**已完成**） |
| 预览实测 | `docs/superpowers/notes/2026-09-18-preview-smoke.md` | 阶段 5 合成库实测（**本轮用合成素材，真实 iPhone 素材待补**） |
| 计划 7 | `docs/superpowers/plans/2026-09-18-stage6-integrity-diagnostics.md` | 阶段 6：UUID 校验 + 异常分类 + iCloud 手动开关 + 诊断报告（**已完成**） |
| 校验实测 | `docs/superpowers/notes/2026-09-18-integrity-smoke.md` | 阶段 6 真机实测（含 content-id 探测记录同目录） |
| 计划 8 | `docs/superpowers/plans/2026-09-18-stage7-browse-ui.md` | 阶段 7：浏览体验与 UI（时间线分组/搜索筛选/多选导出/从库删除）（**已实现**） |
| 实测记录 | `docs/superpowers/notes/2026-09-18-browse-smoke.md` | 阶段 7 自动化验证（GUI 交互实测待补） |
| 计划 9 | `docs/superpowers/plans/2026-09-18-stage8-portable-packaging.md` | 阶段 8：打包分发（绿色版：组装脚本 + 裁剪 ffmpeg + README + 发布流程）（**已实现并验收**；仅剩实际发 GitHub Release） |
| 打包实测 | `docs/superpowers/notes/2026-09-18-packaging-smoke.md` | 阶段 8 打包实测（no-bundle 产物、依赖、组装、启动） |
| 计划 10 | `docs/superpowers/plans/2026-09-18-stage9-ui-redesign.md` | 阶段 9：界面改版（经典 HIG + 左右布局 + 跟随系统深浅）（**已实现**） |
| 界面参考稿 | `docs/mockups/index.html` | 静态 HTML 参考稿 A/B/C/D（非构建产物） |

## 当前进度

- [x] 设计完成并过审
- [x] 计划 1 编写完成
- [x] **Task 0 闸门通过：MTP 确实暴露 `.MOV`，通道可行**（详见设计文档 §2.4 / §2.5）
- [x] **计划 1（阶段 0 WPD 只读探测程序）实现完成**：`crates/probe`，25 个单元测试全绿，已实测 iPhone
- [x] **计划 2（阶段 1 Tauri 骨架 + `lpm://` 协议 + 虚拟滚动网格）编写并实现完成**：`src-tauri` + `src`；滚轮与拖动滚动条均实测流畅
- [x] **计划 3（阶段 2 SQLite 索引 + 库目录管理）编写并实现完成**：打开库 → 重建索引 → 统计/网格均实测符合预期
- [x] **计划 4（阶段 3 设备导入）编写并实现完成**：实机 60/60 文件传输成功、字节数校验零错位
- [x] **计划 5（阶段 4 ffmpeg 集成 + 缩略图）编写并实现完成**：HEIC→512px WebP、导入时生成、状态推进到 `transcoded`、网格显示缩略图（已用户确认）；顺带修复 `taken_at`（`VT_DATE` 解析）
- [x] **计划 6（阶段 5 预览片 + 悬停播放）编写并实现完成**：后端 `ensure_preview` + 前端单例 `<video>`；
  悬停播放、快速划过不触发、滚动/缩放不触发、DevTools 确认只有一个 `<video>`，均已实测通过
- [x] **计划 7（阶段 6 UUID 校验 + 异常分类 + iCloud 手动开关 + 诊断报告）编写并实现完成**：真实 iPhone 上
  `content_id` 两侧一致、`integrity=0`；回填命令与诊断导出均实测通过
- [x] **计划 8（阶段 7 浏览体验与 UI）编写并实现完成**：年/月/天多级时间线、搜索筛选（文字/类型/日期/完整性）、
  分页增量加载、多选导出（\`trash\` 回收站删除）、从库删除与孤儿缓存清理。自动化验证全绿
  （liveporter 63 + probe 25 单测、clippy -D warnings、fmt、vue-tsc、vite build）。
  GUI 经用户 dev 冒烟确认无明显问题；**逐项清单仍待完整复验**，见
  `docs/superpowers/notes/2026-09-18-browse-smoke.md`
- [x] **计划 9（阶段 8 打包分发，绿色版）编写完成**：`tauri build --no-bundle` 组装脚本、
  裁剪版 ffmpeg（15~25 MB）、DLL 依赖收集、README.txt、GitHub Release 流程。
  文档见 `docs/superpowers/plans/2026-09-18-stage8-portable-packaging.md`，**尚待实现**

计划 9 的 7 项取舍已过审：裁剪 ffmpeg 本计划就做、捆绑 VC 运行时 DLL、首发版本 1.0.0、
资源解析默认不改、动态 CRT、组装脚本用 PowerShell 5.1、最低 Windows 10 1809+。

**计划 9 进展（2026-09-18）：**
- [x] **Task 0** `--no-bundle` 产物实测：只有单个 `liveporter.exe`（6.05 MB），依赖全是系统组件，
  **无 VC++ 运行时 / WebView2Loader 依赖**（否决了「捆绑 DLL」的原决策）；`resource_dir()` = exe 目录，
  `resources/ffmpeg.exe` 解析成功；Task 3 无需改代码
- [x] **Task 1** 裁剪 ffmpeg：MSYS2 + MinGW-w64 + 源码裁剪完成，`scripts/build-ffmpeg.sh`；
  `ffmpeg.exe` 6.31 MB + `ffprobe.exe` 6.15 MB（≈12.46 MB）。真实 iPhone 素材三路径 + 三个 Rust ignored
  冒烟全过。**关键点：HEIC 需要 `xstack` 滤镜**（否则报 `No such filter: 'xstack'`）。产物在
  `C:\Users\Raven\ffmpeg-build\out\bin`（仓库外）
- [x] **Task 2** `scripts/deps-check.ps1`（本机输出：无非系统 DLL）
- [x] **Task 4** 版本同步为 `1.0.0`；`scripts/package-portable.ps1`（假 ffmpeg 冒烟通过，zip 结构正确）
- [x] **Task 5** `docs/portable-readme.txt`（UTF-8 BOM）
- [x] **Task 6** 干净环境人工验收：用户在真实库上实测打包版，**功能全部 OK**。验收中修复两个 bug：
  (a) 子进程弹黑框 → `ffmpeg::command()` 加 `CREATE_NO_WINDOW`；
  (b) 重扫产生重复行/「未知日期」→ `db::device_id_for_folder()` 让 indexer 复用已有设备行
- [x] **Task 7** `docs/releasing.md`
- 实测记录：`docs/superpowers/notes/2026-09-18-packaging-smoke.md`

- [x] **计划 10（阶段 9 界面改版）实现完成**：原生 Vue+CSS，左侧栏 + 主区、跟随系统深浅、
  toast/确认框/空状态/进度；按 `apple-hig` 经典 HIG，排除 iOS 26+ Liquid Glass（见「已定的关键约束」）。
  验证：`vue-tsc`、`npm run build`、`tauri dev` 用户确认「看着还不错」
- [x] **改版后打磨（UI/UX 审查 P0，2026-09-18）**：欢迎/最近库启动页、单列大图流、方角网格 + 2px 间隔、
  按行预载、滚轮平滑滚动、拖拽涂抹（可回拖撤销）、单击取消、键盘导航（Tab/方向键/Space/Delete）、
  右键菜单、页脚加载态修正、导入取消提示、空结果「清空筛选」、最近库持久化（exe 同目录 `recent.json`）、
  重建索引非阻塞进度 + 取消、打开库自动重建/补缩略图、`larges/` 按需高清大图（`ensure_large`）
  - **P1/P2/P3 待办**见 `docs/superpowers/plans/2026-09-18-stage9-ui-redesign.md` 末尾与本文「UI 待打磨」
  - 已知取舍：日期头不吸附；单列用 2000px WebP（比 512 缩略图清晰，非原图）；`recent.json` 放只读目录时不记忆

### UI 打磨记录（均已按 HIG 完成，2026-09-18）

- **P1 反馈**：统一进度区（侧栏一处，可取消）、导入按钮内活动指示、`校验标识` 进度+取消、
  去掉琐碎成功 toast、删除确认仅批量 ≥10 才弹 —— 全部完成
- **P2 视觉/收敛**：筛选气泡（日期/完整性 + 角标）、粒度进「更多」（带 ✓/键盘导航）、悬停改内描边、
  勾选徽标按尺寸缩放 —— 全部完成
- **P3 清理**：死代码（`.action`/`.empty-logo`/`--r-tile`/未用图标）—— 完成
- **HIG 高优先**：网格加实况 `LIVE` 徽标 / 视频播放徽标（`live-photos.md`）；查看器可播放实况/视频
  （按需 H.264 代理，`ensure_view_video`）
- **HIG 中优先**：筛选气泡带箭头（`popovers.md`）、查看器加载转圈、分段条改半透明材质带
- **功能缺口**：单击看大图 + 滚轮锚点缩放/拖拽（设计 §7.1）、顶部固定分段条 —— 完成

**HIG 审查结论：不做自绘标题栏**（2026-09-19）
- 依据：`windows.md`「**No custom window UI** — system windows look and behave as people expect.
  Custom frames or controls that imperfectly match system look/behavior make the app feel broken.」
  `going-full-screen.md`「Use the system-provided full-screen experience… Avoid custom window-mode menus.」
- 自绘标题栏还会丢掉 Windows 原生行为：贴边分屏（Aero Snap）、悬停最大化的 Snap Layouts、
  拖动到屏幕顶最大化、标题栏右键系统菜单、高对比度/系统主题、DPI 缩放、键盘可达性。
- 结论：**保留原生窗口边框与系统标题栏**。若只是想弱化标题栏观感，可走低风险折中：
  设置窗口主题/标题栏颜色（DWM），而不是自绘控件。

**有意取舍（不再改，除非有需求）**：
- 搜索防抖 250ms（非 `search-fields.md` 的逐键搜索；本地 SQLite 查询足够快，防抖避免频繁重查）
- 删除确认框「删除」仍用红色 destructive 样式（`alerts.md` 说主动删除可不标红；保留更稳妥）
- 日期头用「顶部固定分段条」而非真·滚动吸附（虚拟化下更低风险）
- 单列大图用 2000px WebP（非原图；原片 HEIC/HEVC WebView2 解不了）
- 查看器播放代理静音（裁剪版 ffmpeg 无 aac 编码器）
- 实况用「悬停播放」而非 iOS 的「按住播放」（桌面鼠标类比）

**下一步：按 `docs/releasing.md` 做首次发布（打 tag + `gh release`）。可选后续：NSIS 安装包、代码签名。**

### 真机验证（已在 2026-09-18 补齐）

原开发机的库（`D:\图片\Pictures\iPhone`）不在写这段时的机器上，当时阶段 5 实测用的是
**ffmpeg 合成的库**（`F:\project\github\livephoto-testlib`，在仓库之外）。以下三条**已用真实 iPhone
补齐**（见 `notes/2026-09-18-preview-smoke.md` 的「真机复验」节）：

1. **真实 iPhone HEIC 多流怪癖** —— 已复验：真实 HEIC 生成缩略图成功，`-filter_complex` 修复成立。
2. **真实实况预览片体积与耗时** —— 已实测：20~135 KB、转码 0.13~0.26 s。
3. **真实素材悬停手感** —— 由转码耗时推断无问题（仍建议人工确认一次）。

## 阶段 5 关键实现要点（改动前先看）

- **全局只有一个 `<video>`**，用 `v-show` 而**不是** `v-if` 控制可见性。用 `v-if` 会让元素随每次悬停
  销毁重建，丢失解码器状态，等于每个格子都换一个新 video——正好毁掉这个设计的全部意义。
- 预览片由浏览界面**按需生成**（`ensure_preview`），**不要**把它塞回导入管道。
- 预览片与缩略图都用**内容哈希**命名，内容相同的条目自动复用同一个文件（实测生效）。
- `previews/` 与 `thumbs/` 都在库内、走 `lpm://`（库根已作为允许根）。
- 前端有 `token` 机制丢弃过期的异步结果：悬停 A 未完成就切到 B 时，A 的结果不能覆盖 B 的画面。

**计划 4 遗留（动手前必读 `notes/2026-09-17-import-smoke.md`）：**
- **iPhone 必须设为「保留原件」**：设置 → 照片 → 「传输到 Mac 或 PC」= **保留原件（Keep Originals）**。若设为「自动」，iOS 会即席把 HEIC 转 JPG、处理视频，导致传输慢约 17 倍（1.65 → 28.6 MB/s）且格式被转码。**产品应检测并提示**（信号：设备上静态图全是 `.JPG`、无 `.HEIC`）。改完需**拔插重连**才生效。
- **iOS 对象句柄易失**：必须用 `WPD_OBJECT_PERSISTENT_UNIQUE_ID` 重解析后再取流，否则会**静默写入错误对象的数据**（曾实测错位 2 个）。传输类改动务必校验字节数（已在 `importer::run_tasks` 加了校验）。
- **会话会被强杀卡死**：只能物理拔插恢复；产品需加"会话失效请重插"引导。
- **并发无收益（已终测）**：根因修复并移除无谓的 250ms 释放延迟后，1 线程 **26 MB/s**、2 线程 21.9、4 线程 23.6（`ERROR_BUSY`）。**维持并发 1**；`transfer.rs` 里不要再加回释放延迟。
- **`taken_at` 解析**：`WPD_OBJECT_DATE_CREATED` 是 `VT_DATE`，`PropVariantToDouble` 实测失败；已改为「先试 double，失败则解析 `PropVariantToBSTR` 的字符串」。修复前导入到 `unknown/` 的库需清空重导。
- **历史警告**：修复前的界面导入曾在 `D:\图片\Pictures\iPhone` 留下 66 个内容错误的条目；该库**已清理**（目录为空），可重导。

实测关键结论（动手前先读设计文档 §2.5）：
- WPD 直调能看全整机（1987 个文件），资源管理器只暴露一部分（193 个）——**必须走 WPD 直调**。
- 顶层没有 `DCIM`，遍历要「递归整卷、按内容类型/扩展名过滤」。
- 自造宽字符串传给 COM 前必须补结尾 NUL，否则 `Open` 返回 `E_POINTER`。
- 系统可能有多个 WPD 设备（含把本机磁盘映射成 WPD 的），须按友好名过滤出 iPhone。

## 待定 / 未验证 —— 不要臆断

1. ~~许可证未定~~ → **已定 GPL-3.0-or-later**（设计文档附录 A 决策 6）。`LICENSE` 已建，`Cargo.toml`/`package.json` 的 `license` 字段已填。原因：捆绑 GPL 版 ffmpeg（H.264 编码依赖 `libx264`）。第三方声明见根 `THIRD_PARTY_NOTICES.md`。
2. **iCloud「优化 iPhone 储存空间」的检测手段未确定** —— 设计文档 §6.4 只定了行为策略，检测方式需实测
3. **WPD 并发流数量与传输速率的关系** —— **已实测（阶段 3）：并发无收益，维持 1**（见上文计划 4 遗留）
4. **Windows 上访问 iPhone 相册只有 WPD 一条路**（底层是 iOS 的 PTP 实现）。换语言、换库都不会更快，因为大家都在调同一套 Windows 驱动栈。不要提议引入 `mtp-rs` 之类的第三方封装来"提升性能"——它官方只验证过 Android

## 已定但尚未落地到计划

1. **分发形态 = 绿色版（免安装）** —— 解压即用、免管理员、不写注册表；不追求单文件 exe。见设计文档 §11 与附录 A 决策 20。**后续的打包计划（对应设计 §13 第 9 步）必须落实：** `tauri build --no-bundle` 后组装 zip（裸 exe + 同目录 DLL + `resources/`）、ffmpeg sidecar 放入 `resources/` 并用 `BaseDirectory::Resource` 解析、WebView2 缺失说明、SmartScreen 未签名提示。

## 教训（踩过的坑，别再犯）

- **改任何小功能点，先审视整体性再动手**：列出「这块还有谁会读/写同一状态、同一 DOM、同一事件」，
  确认改动不会破坏既有路径后再改。别只盯着眼前那一处——多数 bug 不是新代码错，而是**与既有逻辑叠加**才错。
  - 动手前问三句：① 这个状态还有谁在用？② 这个 DOM 还有谁在监听？③ 这个事件还有谁在拦截？
  - 改完**必须实测受影响的相邻功能**（不是只测你改的那个），再跑全量验证。
  - 宁可多花五分钟读一遍相关文件，也不要引入回归。
- **滚动相关改动必须实机验证三种交互**：滚轮、拖动滚动条、点击空白。`vue-tsc`/`build` 抓不到滚动竞态。
- **不要自建滚动动画状态机**。曾同时存在「原生滚动 / 自定义缓动动画 / `focus()` 滚动」三方写 `scrollTop`，
  靠标志位互相猜，反复出 bug（点击跳、拖滚动条回弹、动画自我掐停）。现统一用原生
  `scroll-behavior: smooth` + `scrollTo({behavior})`，见 `PhotoGrid.vue`。
- **不要给滚动容器加 `tabindex` + `@focus` 去转发焦点**：点击空白/拖滚动条会触发 `focus()` 把视图滚回去。
  键盘可达性用「漫游 tabindex」（首个可见瓦片为 `tabindex=0`）实现。
- **`focus()` / `scrollIntoView()` 会滚动容器**：只在明确的键盘导航里用，且先手动把目标滚入视口。

## 工作流要求

- **所有 UI/UX 改动必须先过 HIG 审查**（用全局 `apple-hig` skill）：先明确引用到哪几条规范、结论是什么、
  有无取舍，**过审后再动手**；未过审不要直接改界面。
- **分支**：开发在 `dev` 分支；`main` 只收已完成的里程碑
- **复杂改动先给方案**（改哪些文件、为什么、有无更小方案），确认后再动手
- **改完必跑验证并如实报告**：`cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`
- **不要擅自 `git commit` / `git push`**，除非用户明确要求
- **提交信息用英文**，代码与变量名用英文
- **最小改动**：不动无关代码，不做顺手重构、不批量格式化
- **不确定就说不确定**，不要编造 API、版本行为、路径。查不到就说"需实测"
- 引用代码位置用 `文件路径:行号`

## 已定的关键约束（详见设计文档，此处只列最容易踩的）

- **导入阶段只生成缩略图**，预览片是浏览时按需生成、带 350ms 防抖。别把预览片塞回导入管道
- **Chromium 解不了 HEIC，也解不了 HEVC**。所以所有派生资源都必须由 ffmpeg 生成，不能指望 WebView2 直接读原文件
- **全局只允许有一个 `<video>` 元素**（悬停播放复用同一个）。一万个格子放一万个 video 会直接崩
- **图片和视频绝不走 Tauri IPC**，走自定义协议 `lpm://`
- **配对依据**：先用主文件名快速配对，文件落地后再读 `ContentIdentifier` UUID 校验
- **WPD 相关代码必须只读**（探测阶段尤其），计划里有专门的守卫测试
- **UI 不使用 iOS 26+ 的 Liquid Glass 设计语言**（用户 2026-09-18 决定）。参考全局 skill
  `apple-hig`（`~\.agents\skills\apple-hig`），但**明确跳过** Liquid Glass 相关段落：
  `materials.md` 的 Liquid Glass 章节（只用 Standard Materials）、`color.md` 的 Liquid Glass Color、
  `layout.md`/`sidebars.md`/`tab-bars.md` 的 Liquid Glass 浮层、`app-icons.md` 的 Icon Composer。
  其余经典 HIG 照用（系统语义色、字号层级、8pt 栅格、同心圆角、标准材质毛玻璃、44pt 命中区、
  Reduce Motion）。主题**跟随系统** `prefers-color-scheme`，不做应用内主题开关。

## 环境要求（每台开发机都需要）

| 依赖 | 说明 |
|---|---|
| Rust stable (`x86_64-pc-windows-msvc`) | 需要 `--profile default`（含 clippy / rustfmt），`minimal` 不够 |
| MSVC 编译器 | Visual Studio 的「C++ 生成工具」工作负载，提供 `cl.exe` / `link.exe` |
| **Windows SDK** | **必须**。`PortableDevice.h` 是 WPD 的 `PROPERTYKEY` 常量唯一的权威来源，凭记忆写 GUID 会静默出错。装完 SDK 后 Rust 也能自动探测到非标准路径安装的 VS（实测：`F:\vs2019` 无需手动加载 vcvars） |
| WebView2 运行时 | Win10/11 一般自带 |
| Node ≥ 22、git | |
| ffmpeg | 开发期用完整构建即可（`winget install Gyan.FFmpeg`）。**版本要与既有的实测记录对齐**，见下方 |

### 本机环境快照（2026-09-18 就绪）

> **MSYS2（裁剪 ffmpeg 用）**：装在 `C:\msys64`（winget 下载被墙，用清华镜像
> `https://mirrors.tuna.tsinghua.edu.cn/msys2/distrib/msys2-x86_64-latest.exe` 装的）。
> 已装 `mingw-w64-x86_64-gcc/binutils/nasm/x264/libwebp/pkgconf`。
> 裁剪产物：`C:\Users\Raven\ffmpeg-build\out\bin`（仓库外）。**HEIC 解码必须带 `xstack` 滤镜**。

> 下面是某一台具体机器的状态，**换机后必须重新核对**，不要直接采信。

- Rust `1.98.1`（`cargo` / `clippy 0.1.98` / `rustfmt 1.9.0`）
- Windows SDK `10.0.26100`，头文件在
  `C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um\PortableDevice.h`
- ffmpeg `9.0.1-full_build`（gyan.dev），路径：
  `%LOCALAPPDATA%\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-9.0.1-full_build\bin\ffmpeg.exe`
- 跑冒烟测试**和** `npm run tauri dev` 前都必须设 `$env:LIVEPORTER_FFMPEG` 指向上面这个路径，
  否则生成缩略图/预览片会报「未找到 ffmpeg.exe」（`ffmpeg::find_ffmpeg` 只查环境变量、
  主程序同目录、同目录 `resources/`，**不查 PATH**）

**注意**：阶段 4 的实测记录用的是 `9.0.1-**essentials**_build`，本机装的是 `full_build`（超集）。
两者都含 `libwebp` / `libx264` / `hevc`，已实测冒烟通过。

## 需要安装的 skills（全局，不在仓库内）

本项目的开发依赖以下 6 个 agent skill。它们装在用户级目录（Windows 上是 `~\.agents\skills\`），**换机器需要重装**：

```bash
npx skills add apollographql/skills@rust-best-practices -g -y
npx skills add wshobson/agents@rust-async-patterns -g -y
npx skills add digitalsamba/claude-code-video-toolkit@ffmpeg -g -y
npx skills add nodnarbnitram/claude-code-extensions@tauri-v2 -g -y
npx skills add justinwetch/higagentskills@apple-hig -g -y
npx skills add jkc66/custom-icons-skill@custom-icons -g -y
```

其中 `apple-hig` 用于界面设计规范；**本项目只取其经典 HIG 部分，明确排除 iOS 26+ 的 Liquid Glass**
（见「已定的关键约束」）。`custom-icons` 用于应用图标设计（见下）。第三方小仓库**只当参考**。

## 应用图标

- **源文件**：`src-tauri/app-icon.svg`（一笔连笔箭头：起笔 → 打圈 → 甩出 → V 形箭头；浅底墨线）。
  设计遵 `custom-icons` skill 的 native-vector 分支 + `apple-hig` 的 `app-icons.md`（1024²、满幅、无文字）。
- **生成图标集**：把 SVG 渲染成 1024² 透明 PNG（本机用无头 Edge：
  `msedge --headless=new --default-background-color=00000000 --window-size=1024,1024 --screenshot=...`，
  见 workspace 里的 `render.ps1`），再 `npx tauri icon src-tauri/app-icon.png`。
  **PNG 与 `icons/` 均已提交**，正常构建不需要再渲染。
- **备选方案**：`docs/mockups/icons/`（`n-loop-dark` 等；当前用 M 浅底）。
- 校验脚本：`custom-icons` 的 `scripts/validate_icon.py`（SVG 结构 + PNG 透明圆角）。

其中 `tauri-v2` 的来源仓库 star 数很少，**只当参考，不当事实来源**；Tauri 2 的 API 以官方文档为准。

## 目录结构（实际）

```
livephoto-manager/
├── Cargo.toml                    workspace 根
├── LICENSE                       GPL-3.0-or-later
├── THIRD_PARTY_NOTICES.md        第三方声明（ffmpeg 等）
├── crates/probe/                 阶段 0：WPD 只读探测程序
├── src-tauri/                    阶段 1 起：Tauri 核心（Rust）
│   └── src/
│       ├── db.rs                 SQLite 索引
│       ├── library.rs            库目录结构与路径
│       ├── indexer.rs            扫描 originals/ 建索引
│       ├── importer.rs           导入管道（状态机 + 断点续传）
│       ├── pairing.rs            主名配对
│       ├── device/               WPD 访问（keys.rs / transfer.rs）
│       ├── ffmpeg.rs             ffmpeg 定位与参数构造
│       ├── thumb.rs              缩略图 / 预览片生成
│       ├── protocol.rs           lpm:// 自定义协议
│       ├── export.rs             多选导出（原样拷贝配对文件）
│       ├── delete.rs             回收站删除 + 孤儿缓存清理
│       ├── commands.rs           Tauri 命令（含 recent/scan/ensure_large）
│       └── state.rs              应用状态
├── src/                          阶段 1 起：Vue 前端
│   ├── components/PhotoGrid.vue  虚拟滚动网格（长按/拖拽选择、键盘、右键）
│   ├── components/FilterBar.vue  工具栏内联筛选
│   ├── components/AppIcon.vue    内联 SVG 图标
│   ├── composables/useZoom.ts    Ctrl+滚轮缩放（1/3/5/7/10/14 档）
│   ├── composables/usePreview.ts 悬停预览（全局唯一 <video>）
│   ├── lib/{lpm,timeline,gridLayout}.ts
│   ├── styles/app.css            HIG 设计令牌（跟随系统深浅）
│   └── stores/{import,library}.ts
├── scripts/                      绿色版打包（build-ffmpeg / package-portable / deps-check）
└── docs/mockups/                 静态 UI 参考稿（非构建产物）
└── docs/superpowers/             设计文档、计划、实测记录
```
