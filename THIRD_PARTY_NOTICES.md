# 第三方组件与许可

LivePorter 本体以 **GPL-3.0-or-later** 发布（见根目录 `LICENSE`）。下面列出随分发包一起提供的第三方组件及其许可与来源。

## FFmpeg

- **用途**：解码 HEIC（HEVC 静帧）、HEVC 视频，生成 H.264 预览片（通过子进程调用）。
- **许可**：**GPL-3.0-or-later**（所用静态构建包含 `libx264` 等 GPL 组件）。
- **来源与构建**：由本项目从官方源码裁剪构建，脚本见 `scripts/build-ffmpeg.sh`。
  - 源码：FFmpeg **9.0.1**，<https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz>
  - 工具链：MSYS2 MinGW-w64（`mingw-w64-x86_64-gcc` / `binutils` / `nasm`）
  - 依赖：`libx264`（GPL）、`libwebp`（BSD）
  - configure 摘要（`--disable-everything` + 白名单）：
    `--enable-gpl`，demuxer `mov,image2`；decoder `hevc,h264,mjpeg,png,webp`；
    parser `hevc,h264`；encoder `libx264,libwebp`；muxer `mp4,webp`；
    filter `scale,format,null,xstack`；protocol `file,pipe`；
    `--enable-static --enable-small`。
- **体积**：`ffmpeg.exe` 约 6.31 MB + `ffprobe.exe` 约 6.15 MB。
- **合规要求**：分发时必须随附 FFmpeg 的许可证文本（GPL），并按 GPL 提供其对应源码或获取途径：
  - FFmpeg 源码：<https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz>
  - 构建脚本：`scripts/build-ffmpeg.sh`（本项目仓库内）
  - `libx264`：<https://www.videolan.org/developers/x264.html>（GPL）
  - `libwebp`：<https://chromium.googlesource.com/webm/libwebp>（BSD）

> 状态：**已在阶段 8 用真实 iPhone 素材（HEIC + MOV）实测通过**（缩略图 / 预览片 / `ffprobe` 读 ContentIdentifier）。
