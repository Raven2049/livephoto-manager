# LivePorter

一个自用的小工具：把 iPhone 的**实况照片**（Live Photo = HEIC/JPG 静态图 + 同名 MOV 短视频）
搬到 Windows 硬盘上，并且能像相册一样浏览。

本来只是自己用，写着写着觉得还行，就顺手开源了，希望能帮到有同样需求的人。

## 起因（先说这个）

我有些 iPhone 实况照片想存到电脑。用资源管理器（MTP）直接拖拽拷贝时特别慢，
一度以为是 iPhone 走 Windows 就这速度，差点认了。

后来才发现，**问题在手机设置**：

> 设置 → 照片 → 「传输到 Mac 或 PC」

如果这里是「**自动**」，iOS 会在传输时**即席把 HEIC 转成 JPG、并处理视频**——又慢、
又改格式。改成「**保留原件**」之后，我这边的实测传输速度大约快了 **17 倍**，格式也不再被转码。

也就是说：慢的不是拷贝方式，是这个设置。这个工具是在这之后写的，走 WPD 直连来配对搬运，
并在 Windows 上补一个像相册的浏览界面。

## 用之前请先改手机设置

设置 → 照片 → 「传输到 Mac 或 PC」= **保留原件（Keep Originals）**。改完**拔插重连**才生效。

另外，iCloud「优化 iPhone 储存空间」会让原件留在云端、手机只剩低清占位，USB 通道看不到这些原件；
需要先在手机上改成「下载并保留原件」再导入。

## 它能做点什么

- 从 iPhone 导入：静态图与同名 MOV 一起搬，按字节数校验完整性，可断点续传。
- 打开任意照片目录当「库」直接浏览（递归扫描），按拍摄时间分成 年 / 月 / 天。
- 网格浏览，`Ctrl+滚轮` 缩放，悬停播放实况/视频，单张查看。
- 搜索、按类型/日期筛选、多选、导出、删除到回收站。
- 缩略图、预览片、索引都放在库内隐藏的 `.lpm/` 目录，删掉可重建，不动你的原文件。

## 使用

绿色版解压即用，双击 `LivePorter.exe`。选一个硬盘目录当库 → 插上 iPhone → 点导入。
细节见随包的 `README.txt`（`docs/portable-readme.txt`）。

## 从源码跑（开发者）

需要：Rust stable（`x86_64-pc-windows-msvc`）、MSVC「C++ 生成工具」、Windows SDK、Node ≥ 22、ffmpeg。

```powershell
npm install
$env:LIVEPORTER_FFMPEG = "<ffmpeg.exe 的绝对路径>"
npm run tauri dev
```

打包绿色版见 `docs/releasing.md`；开发约定见 `AGENTS.md`。

## 说明

- 仅支持 Windows（macOS 有系统方案，这个主要是补 Windows 的空白）。
- 个人自用项目，能力有限，不保证什么，有问题欢迎提 issue。
- 许可证 **GPL-3.0-or-later**（因为捆绑了 GPL 版 ffmpeg），见 `LICENSE` 与 `THIRD_PARTY_NOTICES.md`。
