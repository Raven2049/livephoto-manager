# 阶段 8：打包分发（绿色版）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把应用打成 **解压即用、免管理员、不写注册表** 的 Windows 绿色版 zip（设计 §11、附录 A 决策 20）：
`LivePorter.exe` + 同目录必要 DLL + `resources/`（裁剪版 `ffmpeg.exe` / `ffprobe.exe`）+ 许可证与说明。
并落实设计 §13 第 9 步里的「打包分发」部分与附录 B 第 5 项（ffmpeg 裁剪构建）。

**Architecture:** 不引入新的安装器框架。构建走 `tauri build --no-bundle` 拿裸 exe，剩余全部由仓库内一个
PowerShell 组装脚本完成：收集运行时 DLL、放入 `resources/`、写 README、打 zip、算 SHA256。
ffmpeg 用 MSYS2/MinGW 从源码裁剪编译（`--disable-everything` + 白名单组件），随 zip 分发。

**Tech Stack:** Tauri 2 CLI / PowerShell 5.1 / MSYS2 MinGW-w64（FFmpeg 源码构建）/ `Compress-Archive`

**为什么现在做（设计 §13 第 9 步、§11）：** 功能已到 v1（阶段 0~7）。没有绿色版发布流程，就没有可交付物；
ffmpeg 完整构建约 80 MB，与「轻量软件」定位冲突，必须在发布前裁剪到 15~25 MB（设计 §11）。

---

## 关键事实（2026-09-18 查证，勿凭记忆改）

1. **绿色版布局**（设计 §11/决策 20）：解压目录内是裸 `exe` + 同目录 DLL + `resources/`。
   **不支持单文件 exe**（WebView2 加载器与资源需同目录）。
2. **现有 ffmpeg 定位逻辑已兼容便携布局**（`src-tauri/src/ffmpeg.rs:12`）：
   `LIVEPORTER_FFMPEG` → exe 同目录 `ffmpeg.exe` → exe 同目录 `resources\ffmpeg.exe`。
   开发期 `resources/` 与 `ffmpeg.exe` 都被 `.gitignore` 忽略（根 `.gitignore`），**不进仓库**，由组装脚本填充。
3. **许可证**：GPL-3.0-or-later，捆绑 GPL 版 ffmpeg（`libx264`）。zip 内必须含 `LICENSE` 与
   `THIRD_PARTY_NOTICES.md`（仓库根已有）。
4. **分发渠道 = GitHub Releases**（设计 §11）。
5. **发布构建 profile 已优化体积**（`Cargo.toml`）：`lto=true`、`codegen-units=1`、`panic="abort"`、`strip=true`。
6. **WebView2 不捆绑**（设计 §11）：Win10/11 一般自带，缺失时白屏；用 README 说明，不引入 bootstrapper。
7. **未签名会被 SmartScreen 拦**（设计 §11）：首次运行需「更多信息 → 仍要运行」，README 说明。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. **`tauri build --no-bundle` 的产物到底有哪些**：裸 exe 之外是否还需要同目录 DLL（尤其
   `WebView2Loader.dll`、VC 运行时 `vcruntime140*.dll`/`msvcp140*.dll`）？用 `dumpbin /dependents` 或
   `Dependencies.exe` 实测列出全部非系统依赖。**Task 0。**
2. **`--no-bundle` 是否会自动把 `bundle.resources` 拷到 exe 目录**（倾向「不会」，由脚本手工放）。Task 0。
3. **Tauri 的 `resource_dir()` 在便携布局下指向哪里**（决定是否要用 `BaseDirectory::Resource`）。
   Task 0 打印确认；现有 exe-relative 逻辑可能已够用。
4. **裁剪 ffmpeg 的最小组件集与体积**（附录 B 5）：configure 白名单需对着真实 HEIC/MOV 迭代实测（Task 1）。
5. **MSYS2 环境与静态 `libx264`/`libwebp` 的可得性**（Task 1 先跑通再裁剪）。
6. **最低 Windows 版本**：**已定 Windows 10 1809+**（WebView2 广泛自带）。实现时在 README 写死，
   并避免使用更高版本独有的系统 API。

---

## 文件结构

```
scripts/
├── build-ffmpeg.sh           新增：MSYS2 下的裁剪 ffmpeg 构建（白名单 configure）
├── package-portable.ps1      新增：组装绿色版 zip（exe + DLL + resources + 文档 + SHA256）
└── deps-check.ps1            新增（可选）：列出 exe 的非系统 DLL 依赖
docs/superpowers/notes/
└── 2026-09-18-packaging-smoke.md   新增：打包实测（布局/DLL/体积/干净环境运行）
README.txt（打包进 zip，仓库内可放 docs/portable-readme.txt）  新增：用户使用说明
src-tauri/tauri.conf.json     可能修改：bundle.resources / version
AGENTS.md                     修改：进度与下一步
```

---

## Task 0: 闸门——摸清 `--no-bundle` 产物与依赖

**Files:** Create `docs/superpowers/notes/2026-09-18-packaging-smoke.md`（先建骨架）

- [ ] **Step 1: 出一次 release 裸构建**

```powershell
$env:LIVEPORTER_FFMPEG = "<本机 ffmpeg.exe>"
npm run build
cargo tauri build --no-bundle --config src-tauri/tauri.conf.json
```
（若 `cargo tauri` 子命令不可用，用 `npx tauri build --no-bundle`。）
Expected: 产出 `target/release/liveporter.exe`（路径以实际输出为准，记录下来）。

- [ ] **Step 2: 列出 exe 目录与依赖**

```powershell
Get-ChildItem target/release | Select-Object Name, Length
```
用 VS 自带 `dumpbin /dependents target/release/liveporter.exe`（或 `Dependencies.exe`）列出 DLL 依赖。
**把「非系统 DLL」清单写进 notes**（VC 运行时、WebView2 加载器等）。

- [ ] **Step 3: 验证资源解析路径**

手工摆一个临时便携目录：`<tmp>\LivePorter\LivePorter.exe` + `<tmp>\LivePorter\resources\ffmpeg.exe`（用现成
ffmpeg，同时把 `ffprobe.exe` 放进去），**不设** `LIVEPORTER_FFMPEG`，运行该 exe，确认缩略图/预览能生成。
在代码里临时 `eprintln!("resource_dir={:?}", app.path().resource_dir())`（或写进「导出诊断」报告）确认
`resource_dir()` 指向，用完删除。**把结论写进 notes**（决定 Task 3 是否需要改代码）。

- [ ] **Step 4: 记录并提交 notes 骨架**

```bash
git add docs/superpowers/notes/2026-09-18-packaging-smoke.md
git commit -m "docs: probe tauri no-bundle output and dll dependencies"
```

---

## Task 1: 裁剪 ffmpeg 构建（附录 B 第 5 项）

**Files:** Create `scripts/build-ffmpeg.sh`

- [ ] **Step 1: 搭 MSYS2 环境**

```powershell
winget install MSYS2.MSYS2
```
在 MSYS2 MinGW64 shell 里：
```bash
pacman -S --needed mingw-w64-x86_64-toolchain mingw-w64-x86_64-nasm \
  mingw-w64-x86_64-pkg-config mingw-w64-x86_64-x264 mingw-w64-x86_64-libwebp
```
> `x264`/`libwebp` 是否提供可用的**静态** `.a` 需实测；若只有 shared，改为源码编静态库。**先记录下来再继续。**

- [ ] **Step 2: 写构建脚本（候选白名单，必须迭代）**

```bash
#!/usr/bin/env bash
# 在 MSYS2 MinGW64 shell 中运行：bash scripts/build-ffmpeg.sh <ffmpeg-源码目录> <输出目录>
set -euo pipefail
SRC="${1:?ffmpeg source dir}"
OUT="${2:?output dir}"
cd "$SRC"

./configure \
  --prefix="$OUT" \
  --arch=x86_64 --target-os=mingw32 \
  --enable-static --disable-shared \
  --enable-small --disable-debug --disable-doc --disable-network --disable-autodetect \
  --disable-everything \
  --enable-demuxer=mov,image2 \
  --enable-decoder=hevc,h264,mjpeg,png,webp \
  --enable-parser=hevc,h264 \
  --enable-encoder=libwebp,libx264 \
  --enable-muxer=webp,mp4 \
  --enable-filter=scale,format,null \
  --enable-protocol=file,pipe \
  --enable-libx264 --enable-libwebp \
  --enable-ffmpeg --enable-ffprobe --disable-ffplay \
  --pkg-config-flags=--static --extra-ldflags=-static

make -j"$(nproc)"
make install
ls -la "$OUT/bin"
```
> 这套 `--enable-*` 是**候选**，不是定论。每加/减一项都要重编并用 Task 1 Step 3 的三条真实路径回归。

- [ ] **Step 3: 用真实素材回归三条路径（关键）**

设 `LIVEPORTER_FFMPEG=<OUT>/bin/ffmpeg.exe` 后：
```powershell
cargo test -p liveporter -- --ignored --nocapture
```
至少覆盖：HEIC→缩略图、MOV→缩略图、MOV→预览片、`ffprobe` 读 ContentIdentifier。
三路径全过才算白名单成立；失败就把缺失组件加回去，**把最终 flags 原样写进 notes**。

- [ ] **Step 4: 体积与许可证**

记录 `ffmpeg.exe` + `ffprobe.exe` 合计体积（目标 15~25 MB）。在 `THIRD_PARTY_NOTICES.md` 补充裁剪构建的
configure 摘要与 GPL 说明（若尚未覆盖）。

- [ ] **Step 5: 提交**

```bash
git add scripts/build-ffmpeg.sh THIRD_PARTY_NOTICES.md
git commit -m "build: trimmed static ffmpeg build script"
```

> **退路（若裁剪短期不可行）：** 先用 gyan.dev essentials 构建出绿色版，notes 如实记录体积偏差，
> 把裁剪列为后续；不要因为裁剪卡住整个发布流程。**是否接受退路由过审决定。**

---

## Task 2: 运行时依赖收集（DLL）

> **Task 0 实测结论（2026-09-18）：** release exe 只依赖系统 DLL + UCRT，
> **无 VC++ 运行时 / WebView2Loader 依赖**，因此**不需要捆绑任何 DLL**。
> 本任务改为「保留依赖检查脚本 + 在每次发布前跑一遍」，防止将来引入依赖后漏带。

**Files:** Create `scripts/deps-check.ps1`

- [ ] **Step 1: 依赖检查脚本**

```powershell
param([Parameter(Mandatory=$true)][string]$Exe)
# 优先 dumpbin（VS 自带），退回 Dependencies.exe；打印非系统 DLL。
$dumpbin = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
if ($dumpbin) { & $dumpbin.Source /dependents $Exe }
else { Write-Error "未找到 dumpbin.exe（装 VS C++ 工具或改用 Dependencies.exe）" }
```

- [ ] **Step 2: 决定捆绑清单**

Task 0 实测当前无任何非系统 DLL 依赖 → **捆绑清单为空**。脚本只负责在发布前把「非系统 DLL」
打印出来；**只有出现非系统 DLL 时才需要加入组装清单**（并记录来源/许可）。
系统 DLL（kernel32/user32/…）与 `api-ms-win-*`（UCRT 转发）一律不拷。

- [ ] **Step 3: 提交**

```bash
git add scripts/deps-check.ps1
git commit -m "build: add portable dll dependency check script"
```

---

## Task 3: 资源解析加固（默认不改，按 Task 0 结论）

**Files:** Modify `src-tauri/src/ffmpeg.rs`（仅当 Task 0 证明 exe-relative 不够）

- [ ] **Step 1: 若 `resources/` 不在 exe 同目录**

在 Tauri `setup` 钩子里解析 `app.path().resource_dir()`，若其下存在 `ffmpeg.exe`，则设为进程环境变量
`LIVEPORTER_FFMPEG`（`find_ffmpeg` 已优先读它）：
```rust
.setup(|app| {
    if std::env::var_os("LIVEPORTER_FFMPEG").is_none() {
        if let Ok(dir) = app.path().resource_dir() {
            let ff = dir.join("ffmpeg.exe");
            if ff.is_file() {
                std::env::set_var("LIVEPORTER_FFMPEG", &ff);
            }
        }
    }
    Ok(())
})
```
> `set_var` 在 edition 2021 下安全；只在启动早期调用，且 `find_ffmpeg` 已兼容。
> **不重构** `find_ffmpeg` 签名（几十处调用点不值得动）。

- [ ] **Step 2: 单测/回归**

Run: `cargo test -p liveporter ffmpeg`
Expected: 通过。

- [ ] **Step 3: 提交（仅当有改动）**

```bash
git add src-tauri/src/lib.rs src-tauri/src/ffmpeg.rs
git commit -m "fix(app): resolve bundled ffmpeg from tauri resource dir"
```

---

## Task 4: 绿色版组装脚本

**Files:** Create `scripts/package-portable.ps1`

- [ ] **Step 1: 写脚本**

要点：
1. 读 `package.json` / `tauri.conf.json` 的版本号；**首发前先把三处版本同步为 `1.0.0`**
   （`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`）。
2. `npm run build`；`npx tauri build --no-bundle`。
3. 建临时目录 `dist-portable/LivePorter/`：
   - 拷 `target/release/liveporter.exe` → `LivePorter.exe`（名字与 `productName` 一致）；
   - 拷 Task 2 决定的运行时 DLL；
   - 建 `resources/`，放入 `ffmpeg.exe`、`ffprobe.exe`（来自 `LIVEPORTER_FFMPEG` 同目录或 `-FfmpegDir` 参数）；
   - 拷 `LICENSE`、`THIRD_PARTY_NOTICES.md`、`README.txt`。
4. `Compress-Archive` 成 `LivePorter-<version>-windows-x64.zip`，`Get-FileHash -Algorithm SHA256` 输出校验和到 `.sha256` 文件。
5. 脚本参数化路径，失败即 `exit 1`；**不删用户的 `resources/` 源**。

- [ ] **Step 2: 本地跑通**

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-portable.ps1 -FfmpegDir "<裁剪产物 bin 目录>"
```
Expected: 生成 zip 与 `.sha256`；解开检查目录结构。

- [ ] **Step 3: 提交**

```bash
git add scripts/package-portable.ps1
git commit -m "build: assemble portable zip with bundled ffmpeg"
```

---

## Task 5: zip 内说明文件 README.txt

**Files:** Create `docs/portable-readme.txt`（组装时重命名为 `README.txt`）

内容必须覆盖（设计 §11）：
- **系统要求：Windows 10 1809 及以上（64 位）**，需要 WebView2 运行时。
- 解压即用、免安装、免管理员；**不要单独移动 exe**，整个文件夹一起。
- **首次运行 SmartScreen**：「更多信息 → 仍要运行」（未签名）。
- **WebView2 运行时缺失会白屏**：给出官方下载入口（Microsoft Edge WebView2 Runtime），并说明 Win10/11 一般自带。
- **iPhone 必须设为「保留原件」**（设置 → 照片 → 传输到 Mac 或 PC），否则慢约 17 倍且格式被转码（见
  `notes/2026-09-17-import-smoke.md`）。
- **iCloud「优化 iPhone 储存空间」**：v1 只标记疑似项、不自动阻止；如何手动开启「原件可能不在手机」开关。
- 库目录 = 用户选择的文件夹，`.lpm/` 可删（只是索引/缓存）。
- 许可证 GPL-3.0-or-later 与 ffmpeg 声明位置。

- [ ] **Step 1: 写 `docs/portable-readme.txt`**，在组装脚本里拷为 `README.txt`。
- [ ] **Step 2: 提交**

```bash
git add docs/portable-readme.txt scripts/package-portable.ps1
git commit -m "docs: portable readme bundled in the zip"
```

---

## Task 6: 打包实测（干净环境）

**Files:** Update `docs/superpowers/notes/2026-09-18-packaging-smoke.md`

- [ ] **Step 1: 干净目录解压运行**

把 zip 解压到不含任何开发文件、**不设** `LIVEPORTER_FFMPEG` 的目录（最好另一台机器/新用户），
做一次完整链路：
- 打开库 → 从 iPhone 导入（含 HEIC 实况）→ 缩略图 → 悬停预览 → 校验标识 → 导出诊断。

- [ ] **Step 2: 逐项核对**

| 检查项 | 期望 |
|---|---|
| 无需管理员、不写注册表 | 进程无 UAC 提权；注册表无新增（抽查） |
| 无 `LIVEPORTER_FFMPEG` 也能工作 | 命中 `resources/ffmpeg.exe` |
| ffmpeg/ffprobe 体积 | 15~25 MB（记录实际） |
| zip 体积 | 记录 |
| 依赖 | 干净机上无缺 DLL 报错 |
| WebView2 缺失 | README 说明可用（难以在本机复现，如实标注「未复现」） |
| SHA256 | 与 `.sha256` 一致 |

- [ ] **Step 3: 记录并提交**

```bash
git add docs/superpowers/notes/2026-09-18-packaging-smoke.md
git commit -m "docs: record portable packaging smoke test"
```

---

## Task 7: 发布到 GitHub Releases（流程文档）

**Files:** Create `docs/releasing.md`

- [ ] **Step 1: 写发布步骤**：`main` 分支、打 tag（`v<version>`）、`gh release create v<version> <zip> <sha256> --notes-file ...`、
  在 notes 里贴 SmartScreen 与 WebView2 说明。
- [ ] **Step 2: 首次发布前与用户确认**（版本号、是否附 NSIS 安装包）。
- [ ] **Step 3: 提交**

```bash
git add docs/releasing.md
git commit -m "docs: release process for portable builds"
```

---

## Task 8: 收尾

- [ ] **Step 1:** 全量验证

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test -p probe -p liveporter
cargo clippy -p probe -p liveporter --all-targets -- -D warnings
cargo fmt --check
npx vue-tsc --noEmit; npm run build
```

- [ ] **Step 2:** 更新 `AGENTS.md`（计划 9 完成、下一步）与设计文档附录 B 第 5 项（标注实测结论）。

---

## 完成判据

- [ ] `scripts/package-portable.ps1` 一键产出 `LivePorter-<version>-windows-x64.zip` + SHA256
- [ ] zip 内布局 = 裸 exe + 必要 DLL + `resources/ffmpeg.exe` + `resources/ffprobe.exe` + 文档
- [ ] 干净目录、无环境变量下完整链路可跑（导入/缩略图/预览/校验）
- [ ] 裁剪 ffmpeg 合计 15~25 MB，三条真实路径回归通过（或如实记录退路与体积）
- [ ] README.txt 覆盖 SmartScreen / WebView2 / 保留原件 / iCloud / 许可证
- [ ] 发布流程文档可照做
- [ ] `cargo test / clippy / fmt`、`vue-tsc`、`npm run build` 全绿

## 明确不在本计划内

- **代码签名**（是否买证书待定，设计 §11）——只影响首次运行体验
- **NSIS 安装包**作为可选后续（本计划只做绿色版）
- **iCloud 自动检测**（设计 §6.4 第二步）
- **in-app 首次引导向导**——本计划只用 README.txt；若要向导另开计划
- ffmpeg 的自动下载/更新机制

---

## 已确认决策（2026-09-18 过审）

| # | 议题 | 决策 |
|---|---|---|
| 1 | 裁剪 ffmpeg | **本计划就做**（MSYS2 源码裁剪，目标 15~25 MB） |
| 2 | VC 运行时 DLL | 原定「捆绑」；**Task 0 实测：exe 无 `vcruntime140`/`msvcp140` 依赖（UCRT 属系统组件），故不捆绑**，仅保留依赖检查脚本 |
| 3 | 版本号 | **首发定为 1.0.0**（`package.json` / `tauri.conf.json` / `Cargo.toml` 同步） |
| 4 | 资源解析 | **默认不改** exe-relative 逻辑；仅 Task 0 实测不成立才加 setup 钩子 |
| 5 | CRT 链接 | **动态 CRT + 捆绑运行时 DLL** |
| 6 | 组装脚本 | **PowerShell 5.1** |
| 7 | 最低 Windows | **Windows 10 1809+**（README 写明） |

## 剩余风险与注意

1. **裁剪 ffmpeg 组件需逐项迭代**；每轮都必须用真实素材回归三条路径（缩略图/预览/`ffprobe`）。
   退路仍保留：若短期不可行，先发 essentials 版并如实记录体积偏差，不因裁剪阻塞发布。
2. **VC 运行时 DLL 的来源与许可**要写进 notes 与 `THIRD_PARTY_NOTICES.md`。
3. **`resource_dir()` 行为在 Task 0 实测**；只有 exe-relative 不成立才改代码（Task 3）。
4. **目标机需 WebView2**：不捆绑，README 给官方下载入口。
5. **未签名 → SmartScreen**：README 说明「更多信息 → 仍要运行」。
6. **zip 体积与 SHA256** 需记录到 notes。

## 自查记录

**规格覆盖：** 设计 §11（绿色版、WebView2、SmartScreen、GPL、ffmpeg sidecar 与体积目标、GitHub Releases、
纯净性）、附录 A 决策 20、§13 第 9 步中的「打包分发」；附录 B 第 5 项（ffmpeg 裁剪）。

**对既有实现的承接：** 不重构 `ffmpeg::find_ffmpeg`（仅按实测决定是否加 setup 钩子）；
不改库数据布局；`.gitignore` 已忽略 `resources/` 与 `ffmpeg.exe`，与「二进制不进仓库」一致。

**已知不确定点（实现时必须查证）：** 见文首「待实测 1~6」。
