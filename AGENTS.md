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
| 计划 7 | `docs/superpowers/plans/2026-09-18-stage6-integrity-diagnostics.md` | 阶段 6：UUID 校验 + 异常分类 + iCloud 手动开关 + 诊断报告（**尚未实施**） |
| 计划 8+ | 尚未编写 | 浏览体验与 UI（时间线/搜索/导出/删除）；打包分发 |

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
- [ ] 计划 7+ 尚未编写

**下一步：计划 7（阶段 6：UUID 校验 + 异常分类 + iCloud 手动开关 + 诊断报告）已写好，等待过审。**过审后从 Task 0（真机确认 content identifier 两条读取路径）开始。三处需确认的取舍见计划文末「已知风险与取舍」。

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

## 工作流要求

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

本项目的开发依赖以下 4 个 agent skill。它们装在用户级目录（Windows 上是 `~\.agents\skills\`），**换机器需要重装**：

```bash
npx skills add apollographql/skills@rust-best-practices -g -y
npx skills add wshobson/agents@rust-async-patterns -g -y
npx skills add digitalsamba/claude-code-video-toolkit@ffmpeg -g -y
npx skills add nodnarbnitram/claude-code-extensions@tauri-v2 -g -y
```

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
│       ├── commands.rs           Tauri 命令
│       └── state.rs              应用状态
├── src/                          阶段 1 起：Vue 前端
│   ├── components/PhotoGrid.vue  虚拟滚动网格
│   ├── composables/useZoom.ts    Ctrl+滚轮缩放
│   ├── lib/lpm.ts                lpm:// 协议封装
│   └── stores/{import,library}.ts
└── docs/superpowers/             设计文档、计划、实测记录
```
