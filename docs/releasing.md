# LivePorter 发布流程（绿色版）

面向维护者。目标：从 `dev` 分支产出一个可在 Windows 10 1809+ 解压即用的 zip，并发到 GitHub Releases。

## 0. 前置

- 已合入 `main` 的功能全部验证通过（见各阶段 notes）。
- 本机具备：Rust / Node / VS C++ 生成工具 / Windows SDK / MSYS2（裁剪 ffmpeg 用）。
- 许可证与 `THIRD_PARTY_NOTICES.md` 为最新。

## 1. 确定版本号

把三处版本同步为同一个值（例如 `1.0.0`）：

- `package.json` → `version`
- `src-tauri/tauri.conf.json` → `version`
- `src-tauri/Cargo.toml` → `[package] version`

## 2. 构建裁剪版 ffmpeg（首次或需要更新时）

```bash
# MSYS2 MinGW64 shell
bash scripts/build-ffmpeg.sh <ffmpeg-源码目录> <输出目录>
```

- 目标体积：`ffmpeg.exe` + `ffprobe.exe` 合计 **15~25 MB**。
- 必须用真实素材回归三条路径（HEIC/MOV 缩略图、MOV 预览片、`ffprobe` 读 ContentIdentifier）：
  ```powershell
  $env:LIVEPORTER_FFMPEG = "<输出目录>\bin\ffmpeg.exe"
  cargo test -p liveporter -- --ignored --nocapture
  ```
- **重建后必须重新生成哈希清单**（否则打包会在校验处失败）：
  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts/package-portable.ps1 `
      -RecordFfmpegHash -FfmpegDir "<输出目录>\bin" -SkipBuild -SkipDepsCheck
  ```

## 3. 检查组装的 ffmpeg

确认使用的目录里同时有 `ffmpeg.exe` 与 `ffprobe.exe`，并记录体积。
预期 SHA-256 记录在 `scripts/ffmpeg.sha256`，打包脚本会强校验；不一致会直接失败。

## 4. 依赖检查

打包脚本已内置该门禁（等价于下面命令，`-FailOnFound` 时发现非系统 DLL 会以非 0 退出）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/deps-check.ps1 target/release/liveporter.exe -FailOnFound
```

- 当前预期输出：`OK: no non-system DLL dependency, nothing to bundle.`
- 若列出非系统 DLL：把文件名加入 `scripts/package-portable.ps1` 的 `$runtimeDlls`，
  并把 DLL 放到 `-FfmpegDir` 或另行指定来源（记录来源与许可）。

## 5. 打包

```powershell
$env:LIVEPORTER_FFMPEG = "<裁剪产物>\bin\ffmpeg.exe"
powershell -ExecutionPolicy Bypass -File scripts/package-portable.ps1
```

产出：

- `dist-portable/LivePorter-<version>-windows-x64.zip`
- 同名 `.sha256` 校验和文件

## 6. 干净环境验收

把 zip 解压到一台**没有开发环境、未设 `LIVEPORTER_FFMPEG`** 的机器（或新用户目录），确认：

- [ ] 双击 `LivePorter.exe` 能启动，不弹 UAC。
- [ ] 打开库 → 从 iPhone 导入 → 缩略图 → 悬停预览 → 校验标识 全流程可用。
- [ ] 注册表无新增写入（抽查）。
- [ ] `Get-FileHash` 与 `.sha256` 一致。

## 7. 发布到 GitHub Releases

```bash
git tag v1.0.0
git push origin v1.0.0

gh release create v1.0.0 \
  "dist-portable/LivePorter-1.0.0-windows-x64.zip" \
  "dist-portable/LivePorter-1.0.0-windows-x64.zip.sha256" \
  --title "LivePorter 1.0.0" \
  --notes-file docs/release-notes-1.0.0.md
```

发布说明里务必包含：

- **未签名**：首次运行会被 SmartScreen 拦，点「更多信息 → 仍要运行」。
- **WebView2**：缺失会白屏，给官方下载入口（Win10/11 一般自带）。
- **iPhone 设置**：「传输到 Mac 或 PC」=「保留原件」。
- 校验和与体积。

## 8. 收尾

- 把本次实测写入 `docs/superpowers/notes/`。
- 更新 `AGENTS.md` 的进度与下一步。
