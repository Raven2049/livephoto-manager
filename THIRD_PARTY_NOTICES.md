# 第三方组件与许可

LivePorter 本体以 **GPL-3.0-or-later** 发布（完整许可文本见根目录 `LICENSE`）。下面列出随分发包一起提供的第三方组件及其许可、来源与合规要求。

## FFmpeg

- **用途**：解码 HEIC（HEVC 静帧）、HEVC 视频，生成 H.264 预览片（通过子进程调用）。
- **许可**：**GPL-3.0-or-later**（所用静态构建以 `--enable-gpl` 构建，并链接 GPL 的 `libx264`）。
- **来源与构建**：由本项目从官方源码裁剪构建，脚本见 `scripts/build-ffmpeg.sh`。
  - 源码：FFmpeg **9.0.1**，<https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz>
  - 工具链：MSYS2 MinGW-w64（`mingw-w64-x86_64-gcc` / `binutils` / `nasm`）
  - 依赖：`libx264`（GPL-2.0-or-later）、`libwebp`（BSD-3-Clause）
  - configure 摘要（`--disable-everything` + 白名单）：
    `--enable-gpl`，demuxer `mov,image2`；decoder `hevc,h264,mjpeg,png,webp`；
    parser `hevc,h264`；encoder `libx264,libwebp`；muxer `mp4,webp`；
    filter `scale,format,null,xstack`；protocol `file,pipe`；
    `--enable-static --enable-small`。
- **体积**：`ffmpeg.exe` 约 6.31 MB + `ffprobe.exe` 约 6.15 MB。
- **版本/哈希锁定**：随分发的裁剪版二进制由 `scripts/ffmpeg.sha256` 记录预期 SHA-256，
  `scripts/package-portable.ps1` 在打包前强校验（不一致即失败）。重建 ffmpeg 后用
  `-RecordFfmpegHash` 重新生成该清单。
- **合规要求**：分发时必须随附 FFmpeg 的许可证文本（GPL），并按 GPL 提供其对应源码或获取途径：
  - FFmpeg 源码：<https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz>
  - 构建脚本：`scripts/build-ffmpeg.sh`（本项目仓库内）
  - `libx264`：<https://www.videolan.org/developers/x264.html>（GPL-2.0-or-later）
  - `libwebp`：<https://chromium.googlesource.com/webm/libwebp>（BSD-3-Clause，文本见下）

> 状态：**已在阶段 8 用真实 iPhone 素材（HEIC + MOV）实测通过**（缩略图 / 预览片 / `ffprobe` 读 ContentIdentifier）。

## 组件许可文本

### libx264 — GPL-2.0-or-later

`libx264` 以 **GPL-2.0-or-later** 授权。因其为「2.0 或更高版本」，本项目将其纳入并以
**GPL-3.0-or-later** 对外提供（两者兼容），故完整的 GPL-3.0 文本见根目录 `LICENSE` 即已覆盖本组件。
GPL-2.0 的规范文本见 <https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt>。

### libwebp — BSD-3-Clause

```
Copyright (c) 2010, Google Inc. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.
  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in the
    documentation and/or other materials provided with the distribution.
  * Neither the name of Google nor the names of its contributors may be
    used to endorse or promote products derived from this software
    without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```
