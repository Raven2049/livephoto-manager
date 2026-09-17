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
| 计划 1 | `docs/superpowers/plans/2026-09-17-stage0-wpd-probe.md` | 阶段 0 只读探测程序（10 个任务） |
| 计划 2/3/4 | 尚未编写 | 2=Tauri 骨架+前端网格；3=导入管道；4=浏览体验与 UI 功能 |

## 当前进度

- [x] 设计完成并过审
- [x] 计划 1 编写完成
- [ ] **计划 1 尚未开始实施**

**下一步动作（换机器后第一件事）：跑计划 1 的 Task 0。**

那是一个 5 分钟的一次性 PowerShell **只读**脚本（代码在计划文档里，不依赖仓库任何文件），插上 iPhone 看 `DCIM` 里到底有没有 `.MOV` 文件。

**这是整个项目的 go/no-go 闸门**：若 `.MOV` 未被 MTP 暴露，USB 直连通道不可行，计划 2/3/4 全部作废，需要改评估「解析本地 iTunes 备份」或「iCloud 私有 API」。**在 Task 0 出结论前不要写任何生产代码、不要装 Rust 工具链。**

## 待定 / 未验证 —— 不要臆断

1. **许可证未定**（倾向 GPL-3.0，但**尚未决定**）。因此：
   - 不要往 `Cargo.toml` / `package.json` 里填 `license` 字段
   - 不要创建 `LICENSE` 文件
   - 原因是 ffmpeg 是 GPL 的，许可证选择会决定打包方式
2. **iCloud「优化 iPhone 储存空间」的检测手段未确定** —— 设计文档 §6.4 只定了行为策略，检测方式需实测
3. **WPD 并发流数量与传输速率的关系未实测** —— 默认按 1 来，不要假定并发能提速
4. **Windows 上访问 iPhone 相册只有 WPD 一条路**（底层是 iOS 的 PTP 实现）。换语言、换库都不会更快，因为大家都在调同一套 Windows 驱动栈。不要提议引入 `mtp-rs` 之类的第三方封装来"提升性能"——它官方只验证过 Android

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
