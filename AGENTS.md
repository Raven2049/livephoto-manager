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
| 计划 5+ | 尚未编写 | ffmpeg 缩略图/预览片；UUID 校验与诊断报告；iCloud 检测；浏览体验与 UI |

## 当前进度

- [x] 设计完成并过审
- [x] 计划 1 编写完成
- [x] **Task 0 闸门通过：MTP 确实暴露 `.MOV`，通道可行**（详见设计文档 §2.4 / §2.5）
- [x] **计划 1（阶段 0 WPD 只读探测程序）实现完成**：`crates/probe`，25 个单元测试全绿，已实测 iPhone
- [x] **计划 2（阶段 1 Tauri 骨架 + `lpm://` 协议 + 虚拟滚动网格）编写并实现完成**：`src-tauri` + `src`；滚轮与拖动滚动条均实测流畅
- [x] **计划 3（阶段 2 SQLite 索引 + 库目录管理）编写并实现完成**：打开库 → 重建索引 → 统计/网格均实测符合预期
- [x] **计划 4（阶段 3 设备导入）编写并实现完成**：`liveporter` 30 个单元测试全绿；实机 60/60 文件传输成功、字节数校验零错位
- [ ] 计划 5+ 尚未编写

**下一步：编写计划 5（ffmpeg 缩略图/预览片 + ContentIdentifier 校验 + 异常分类 + 诊断报告）。**对应设计文档 §13 第 5、6 步。

**计划 4 遗留（动手前必读 `notes/2026-09-17-import-smoke.md`）：**
- **iPhone 必须设为「保留原件」**：设置 → 照片 → 「传输到 Mac 或 PC」= **保留原件（Keep Originals）**。若设为「自动」，iOS 会即席把 HEIC 转 JPG、处理视频，导致传输慢约 17 倍（1.65 → 28.6 MB/s）且格式被转码。**产品应检测并提示**（信号：设备上静态图全是 `.JPG`、无 `.HEIC`）。改完需**拔插重连**才生效。
- **iOS 对象句柄易失**：必须用 `WPD_OBJECT_PERSISTENT_UNIQUE_ID` 重解析后再取流，否则会**静默写入错误对象的数据**（曾实测错位 2 个）。传输类改动务必校验字节数（已在 `importer::run_tasks` 加了校验）。
- **会话会被强杀卡死**：只能物理拔插恢复；产品需加"会话失效请重插"引导。
- **并发无收益**：2 线程有 `ERROR_BUSY` 失败、4 线程更差；维持并发 1。
- **待办**：`taken_at` 解析未成功（年份目录全是 `unknown`）——`WPD_OBJECT_DATE_CREATED` 的 `VT_DATE` 转换需查证。
- **历史警告**：修复前的界面导入曾在 `D:\图片\Pictures\iPhone` 留下 66 个内容错误的条目；该库**已清理**（目录为空），可重导。

实测关键结论（动手前先读设计文档 §2.5）：
- WPD 直调能看全整机（1987 个文件），资源管理器只暴露一部分（193 个）——**必须走 WPD 直调**。
- 顶层没有 `DCIM`，遍历要「递归整卷、按内容类型/扩展名过滤」。
- 自造宽字符串传给 COM 前必须补结尾 NUL，否则 `Open` 返回 `E_POINTER`。
- 系统可能有多个 WPD 设备（含把本机磁盘映射成 WPD 的），须按友好名过滤出 iPhone。

## 待定 / 未验证 —— 不要臆断

1. **许可证未定**（倾向 GPL-3.0，但**尚未决定**）。因此：
   - 不要往 `Cargo.toml` / `package.json` 里填 `license` 字段
   - 不要创建 `LICENSE` 文件
   - 原因是 ffmpeg 是 GPL 的，许可证选择会决定打包方式
2. **iCloud「优化 iPhone 储存空间」的检测手段未确定** —— 设计文档 §6.4 只定了行为策略，检测方式需实测
3. **WPD 并发流数量与传输速率的关系未实测** —— 默认按 1 来，不要假定并发能提速
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
| Rust stable (`x86_64-pc-windows-msvc`) | 安装见计划 1 Task 1 |
| MSVC 编译器 | Visual Studio 的「C++ 生成工具」工作负载，提供 `cl.exe` / `link.exe` |
| **Windows SDK** | **必须**。`PortableDevice.h` 是 WPD 的 `PROPERTYKEY` 常量唯一的权威来源，凭记忆写 GUID 会静默出错 |
| WebView2 运行时 | Win10/11 一般自带；计划 2 起需要 |
| Node ≥ 22、git | |

## 需要安装的 skills（全局，不在仓库内）

本项目的开发依赖以下 4 个 agent skill。它们装在用户级目录（Windows 上是 `~\.agents\skills\`），**换机器需要重装**：

```bash
npx skills add apollographql/skills@rust-best-practices -g -y
npx skills add wshobson/agents@rust-async-patterns -g -y
npx skills add digitalsamba/claude-code-video-toolkit@ffmpeg -g -y
npx skills add nodnarbnitram/claude-code-extensions@tauri-v2 -g -y
```

其中 `tauri-v2` 的来源仓库 star 数很少，**只当参考，不当事实来源**；Tauri 2 的 API 以官方文档为准。

## 目录结构（规划，尚未创建）

```
livephoto-manager/
├── Cargo.toml                    workspace 根
├── crates/probe/                 计划 1：阶段 0 探测程序
├── src-tauri/                    计划 2 起：Tauri 核心
├── src/                          计划 2 起：Vue 前端
└── docs/superpowers/             设计文档与计划
```
