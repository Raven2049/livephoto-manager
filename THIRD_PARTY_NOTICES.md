# 第三方组件与许可

LivePorter 本体以 **GPL-3.0-or-later** 发布（见根目录 `LICENSE`）。下面列出随分发包一起提供的第三方组件及其许可与来源。

## FFmpeg

- **用途**：解码 HEIC（HEVC 静帧）、HEVC 视频，生成 H.264 预览片（通过子进程调用）。
- **许可**：**GPL-3.0-or-later**（所用静态构建包含 `libx264` 等 GPL 组件）。
- **来源**：<https://ffmpeg.org/>；Windows 静态构建取自 <https://www.gyan.dev/ffmpeg/builds/> 或 <https://github.com/BtbN/FFmpeg-Builds>。
- **合规要求**：分发时必须随附 FFmpeg 的许可证文本（GPL-3.0），并按 GPL 提供其对应源码或获取途径。
  - FFmpeg 源码：<https://ffmpeg.org/download.html>（请与所捆绑构建的版本号对应）。
  - 构建脚本/来源：见上述构建提供方页面。

> 状态：**尚未捆绑**。计划 5 会在开发阶段先使用完整静态构建跑通；正式打包（后续的打包计划）再决定裁剪配置与随附方式。
