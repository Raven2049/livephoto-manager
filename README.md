# LivePorter

> 把 iPhone 的**实况照片**（Live Photo = HEIC/JPG 静态图 + 同名 MOV 短视频）**配对无损地**批量搬运到
> Windows 硬盘，并提供接近 iOS 相册的浏览界面。

- 平台：**仅 Windows 10 1809+ / 11（x64）**（macOS 有系统方案，本工具专攻 Windows 这个空白）
- 形态：Tauri 2（Rust 核心 + WebView2 前端）+ Vue 3 + TypeScript
- 分发：绿色版，解压即用、免管理员、不写注册表
- 许可：GPL-3.0-or-later（因捆绑 GPL 版 ffmpeg）

## 特性

- **从 iPhone 无损导入**：走 WPD/USB 直连，静态图与同名 MOV 一起搬运；落地后读取 Apple
  `ContentIdentifier` 校验配对，按字节数校验传输完整性（断点续传）。
- **直接浏览任意照片目录**：把已有相册目录当作「库」，递归扫描、按**拍摄时间**分组为
  年/月/天时间线；导入的照片平铺进库根，不改动、不搬迁你已有的文件。
- **近似 iOS 相册的浏览**：虚拟滚动网格、`Ctrl+滚轮` 缩放（1/3/5/7/10/14 列，最大档为单列大图流）、
  悬停播放实况/视频、单张查看（滚轮锚点缩放、拖拽平移）。
- **检索与整理**：文件名搜索、按类型/完整性/日期筛选、多选、导出（原样拷贝配对文件）、
  删除到 Windows 回收站。
- **缓存不外露**：缩略图、预览片、大图、索引都放在库内**隐藏**的 `.lpm/` 目录，删掉即可重建。
- **诊断报告**：环境/统计/异常分布，不含照片内容，序列号打码。

## 使用（用户）

绿色版随包 `README.txt`（`docs/portable-readme.txt`）。要点：

1. 解压整个文件夹，双击 `LivePorter.exe`（免安装）。
2. 「打开资料库」选一个硬盘目录作为库。
3. iPhone：设置 → 照片 → 「传输到 Mac 或 PC」= **保留原件**。
   若设为「自动」，iOS 会即席把 HEIC 转 JPG 并处理视频，**慢约 17 倍且格式被转码**；改完需拔插重连。
4. 插上 iPhone →「从 iPhone 导入」。

> iCloud「优化 iPhone 储存空间」会让手机只保留低清占位、原件在云端，USB 通道看不到这些原件。
> 请先在手机设为「下载并保留原件」再导入。

## 从源码构建（开发者）

前置：Rust stable（`x86_64-pc-windows-msvc`）、MSVC「C++ 生成工具」、Windows SDK、
Node ≥ 22、ffmpeg（开发期用完整构建即可）。

```powershell
npm install
$env:LIVEPORTER_FFMPEG = "<ffmpeg.exe 的绝对路径>"
npm run tauri dev
```

验证与打包：

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
npm run build
# 绿色版打包见 docs/releasing.md
```

更多开发约定（分支、UI 需先过 HIG 审查、环境快照等）见 **`AGENTS.md`**。

## 文档

| 文档 | 位置 |
|---|---|
| 设计文档（唯一真相） | `docs/superpowers/specs/2026-09-17-liveporter-design.md` |
| 库模型（平铺库）重构设计 | `docs/superpowers/specs/2026-09-19-flat-library-refactor.md` |
| 开发说明 / 约定 | `AGENTS.md` |
| 发布流程 | `docs/releasing.md` |
| 第三方许可 | `THIRD_PARTY_NOTICES.md` |

## 许可证

**GPL-3.0-or-later**，见 `LICENSE`。内置 ffmpeg 为 GPL 构建（含 `libx264`），第三方声明与许可文本见
`THIRD_PARTY_NOTICES.md`。
