# 阶段 8 打包分发实测记录（2026-09-18）

## Task 0：`--no-bundle` 产物、依赖与资源解析（本机实测）

环境：`npx tauri build --no-bundle`，release profile（lto / strip）。

### 产物

`target/release/` 里只有：

| 文件 | 体积 |
|---|---|
| `liveporter.exe` | **6.05 MB** |
| `liveporter.pdb` | 3.11 MB（调试符号，不随 zip） |

**没有任何需要同目录的 DLL**（无 `WebView2Loader.dll`）。

### DLL 依赖（`dumpbin /dependents`）

全部为系统组件：

```
kernel32 comctl32 oleaut32 ole32 shell32 combase shlwapi propsys ntdll
bcryptprimitives api-ms-win-core-synch-l1-2-0 user32 gdi32 dwmapi advapi32
api-ms-win-crt-{string,math,heap,utility,time,runtime,convert,stdio,locale}-l1-1-0
```

- **没有 `vcruntime140.dll` / `msvcp140.dll`**：Rust MSVC 构建未引入 VC++ 运行时依赖；
  `api-ms-win-crt-*` 的 UCRT 是 Windows 10+ 自带的系统组件。
  → **结论：先前「捆绑 VC++ 运行时 DLL」的决策不必要**（以实测为准，见 plans 决策 2 的更新）。
- **没有 `WebView2Loader.dll`**：Tauri 2 已静态/内嵌加载器。
- 说明：`dumpbin /dependents` 只列直接导入；`LoadLibrary` 动态加载的库不会出现。故另做了下方启动实测。

### 资源解析实测

临时在 `run()` 的 `setup` 里打印（测完已删除）：

```
[lpm-probe] resource_dir=Ok("\\?\C:\Users\Raven\livephoto-manager\target\debug")
[lpm-probe] find_ffmpeg=Ok("C:\...\target\debug\resources\ffmpeg.exe")
```

- 非捆绑构建下 **`resource_dir()` = exe 所在目录**。
- `find_ffmpeg()`（`src-tauri/src/ffmpeg.rs:12`）命中 `exe 同目录/resources/ffmpeg.exe`。
- 把 `ffmpeg.exe` / `ffprobe.exe` 放进 `target/debug/resources/` 后，exe 正常启动并保持运行 6s
  （无缺 DLL 崩溃）。
- **结论：便携布局用现有 exe-relative 逻辑即可，Task 3 无需改代码。**

### 本机 ffmpeg（顺带记录）

本机装的是 `Gyan.FFmpeg.Essentials`（**不是** AGENTS 快照里的 full_build）：

| 文件 | 体积 |
|---|---|
| `ffmpeg.exe` | **98.09 MB** |
| `ffprobe.exe` | **97.9 MB** |

两者合计近 200 MB，**裁剪必要性得到实测确认**（目标 15~25 MB）。

## Task 1：裁剪 ffmpeg（**完成**）

环境：MSYS2 装在 `C:\msys64`（GitHub 下载被墙，改用清华 TUNA 镜像：
`https://mirrors.tuna.tsinghua.edu.cn/msys2/distrib/msys2-x86_64-latest.exe`）。
包：`mingw-w64-x86_64-gcc 16.2.0`、`binutils 2.47`、`nasm 3.02`、`x264`、`libwebp 1.6.0`、
`pkgconf`（静态库 `libx264.a` / `libwebp.a` 均在，pkg-config `--static` 可解析）。
源码：FFmpeg 9.0.1（ffmpeg.org 直连可下，但常在中途停顿，用 `curl -C -` 续传完成）。

**踩坑（重要）**：HEIC 解码需要 **`xstack` 滤镜**。只用 `scale,format,null` 时，
打开 HEIC 会报 `No such filter: 'xstack'`（iPhone HEIC 多图像流，ffmpeg 内部构图需要它）。
最终白名单已加入 `xstack`，写入 `scripts/build-ffmpeg.sh`。

**成果**（真实 iPhone 素材 `IMG_1717` 回归，全部通过）：

| 路径 | 结果 |
|---|---|
| HEIC → 512 webp 缩略图 | 10,526 字节 |
| MOV → 512 webp 缩略图 | 6,360 字节 |
| MOV → 480p H.264 预览片 | 20,255 字节 |
| `ffprobe` ContentIdentifier | `4700E756-B41E-4303-A840-DDD273FE90D4`（与阶段 6 记录一致）|
| `cargo test real_thumb_smoke / real_preview_smoke / real_movie_content_id_smoke`（指向裁剪版）| 全过 |

体积：`ffmpeg.exe` **6.31 MB** + `ffprobe.exe` **6.15 MB** = **约 12.46 MB**
（优于 15~25 MB 目标；对比 essentials 版近 200 MB）。

裁剪产物路径（本机）：`C:\Users\Raven\ffmpeg-build\out\bin`（仓库外）。

## Task 2：依赖检查脚本

`scripts/deps-check.ps1` 已写并实测，对 `target/release/liveporter.exe` 输出
`OK: no non-system DLL dependency, nothing to bundle.` → **捆绑清单为空**。

## Task 4/5：组装脚本与 README

- 版本已同步为 `1.0.0`（`package.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml`）。
- `scripts/package-portable.ps1` 用假 ffmpeg 冒烟：产出
  `dist-portable/LivePorter-1.0.0-windows-x64.zip` + `.sha256`，zip 内结构正确：
  `LivePorter/{LivePorter.exe, LICENSE, THIRD_PARTY_NOTICES.md, README.txt, resources/{ffmpeg,ffprobe}.exe}`。
- `docs/portable-readme.txt` 已写（UTF-8 BOM，便于记事本显示中文）。
- `dist-portable/` 已加入 `.gitignore`。

**真实打包（Task 1 完成后）**：

- `LivePorter-1.0.0-windows-x64.zip`，体积 **7.98 MB**。
- zip 内：`LivePorter.exe` 6.34 MB；`resources/ffmpeg.exe` 6.62 MB、`resources/ffprobe.exe` 6.45 MB；
  `LICENSE` / `THIRD_PARTY_NOTICES.md` / `README.txt`。
- SHA256：`250E3E547D2546191F0FCCBDF5611D524DBE20E5C2D7B9E1A362265CC4AAEA63`。

> 注意（实测踩坑）：**PowerShell 5.1 读取无 BOM 的 UTF-8 `.ps1` 会按 ANSI 解码**，
> 中文注释会把脚本解析坏。`scripts/*.ps1` 因此**只用 ASCII**。

## Task 6：干净环境运行（完成）

- 已做（essentials 阶段）：release exe + `resources/{ffmpeg,ffprobe}.exe` 摆到临时目录、**不设**
  `LIVEPORTER_FFMPEG`，启动后保持运行 6s（无缺 DLL/资源崩溃）。
- 已做（裁剪版）：解压真实 zip 到 `%TEMP%\lpm_pkg_test\LivePorter\`，**不设** `LIVEPORTER_FFMPEG`
  启动 `LivePorter.exe`（打包版启动正常，裁剪 ffmpeg 被正确解析）。
- **已由用户在真实库 `D:\图片\Pictures\iPhone` 上实测打包版：功能全部 OK。**
- 验收中发现并修复两个 bug（详见 `2026-09-18-browse-smoke.md`）：
  1. 子进程弹黑框（`ffmpeg::command` 加 `CREATE_NO_WINDOW`）；
  2. 重扫因设备行重复导致「未知日期」（`db::device_id_for_folder` 复用已有设备）。
- 修复版重新打包：`LivePorter-1.0.0-windows-x64.zip`，SHA256
  `5F5C3053A2984B5CC76A2849809A860542E555A47C9790DA682D7D444F470DD9`。
- 未做：注册表无写入抽查（未单独验证）。

## Task 7：发布流程

`docs/releasing.md` 已写（版本同步、构建裁剪 ffmpeg、依赖检查、打包、干净机验收、`gh release`）。
