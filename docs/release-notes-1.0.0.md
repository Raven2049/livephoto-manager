# LivePorter 1.0.0

把 iPhone 的**实况照片**（HEIC/JPG 静态图 + 同名 MOV 短视频）**配对无损**地批量搬运到 Windows 硬盘，
并提供接近 iOS 相册的浏览界面（网格、悬停播放动态、Ctrl+滚轮缩放）。

- 平台：Windows 10 1809+（x64）
- 形态：绿色版，解压即用，免管理员、不写注册表
- 许可：GPL-3.0-or-later（捆绑 GPL 版 ffmpeg，见包内 `THIRD_PARTY_NOTICES.md`）

## 开始前：把 iPhone 设为「保留原件」

设置 → 照片 → 「传输到 Mac 或 PC」= **保留原件（Keep Originals）**。

若设为「自动」，iOS 会在传输时即席把 HEIC 转成 JPG、并处理视频：**传输慢约 17 倍，且格式被转码**。
改完需拔插重连才生效。（本版尚未自动检测该设置，请手动确认。）

## 首次运行

- **未签名**：首次运行会被 SmartScreen 拦截，点「更多信息 → 仍要运行」。
- **WebView2**：Windows 10/11 一般自带；若缺失会白屏，请安装 Microsoft Edge WebView2 Runtime。
- 打开资料库后，插上 iPhone 并解锁、信任此电脑，点侧栏「从 iPhone 导入」。

## 校验

- 随包附 `.sha256`；用 `Get-FileHash <zip> -Algorithm SHA256` 对照即可。
- 安装包体积与哈希见 Release 附件。

## 已知事项（1.0.0）

- 本版**不会自动检测**「保留原件」或 iCloud「优化 iPhone 储存空间」，只会用侧栏开关
  「原件可能不在手机」对明显偏小的条目标记为「疑似非原件」。
- 实况照片的预览为静音 H.264 代理（裁剪版 ffmpeg 不含 AAC 编码器）。
- 查看器中的大图为 2000px WebP 代理，非原始 HEIC（WebView2 无法解码 HEIC/HEVC）。
