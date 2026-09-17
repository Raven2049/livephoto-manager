# 阶段 1：最小 Tauri 骨架 + `lpm://` 协议 + 虚拟滚动网格 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 搭起 Tauri 2 + Vue 3 + TS 的最小应用，打通自定义协议 `lpm://`（带 Range），并用**自写虚拟滚动**渲染一个本地目录里的上万张图片网格，验证设计文档 §8 的前端性能方案。**不碰设备、不建索引、不碰 ffmpeg。**

**Architecture:** Rust 侧只做两件事——(a) 自定义 `lpm://` 协议按需流式读取磁盘文件（支持 Range），(b) 一个"扫描目录"命令返回图片清单。前端拿到清单后用 `shallowRef` 持有、自写虚拟滚动只挂可见瓦片、`<img>` 的 src 走 `lpm://`（绝不走 IPC）。

**Tech Stack:** Tauri 2.x / Rust 2021 / `tauri` + `tauri-build` + `tauri-plugin-dialog` + `http-range` / Vue 3 + TypeScript + Vite + Pinia

**为什么先做这个（设计文档 §13 第 2 步）：** 设计假设"自写虚拟滚动 + 自定义协议"能扛住万级条目。这个假设**未被任何代码验证过**。在往里面塞 SQLite、WPD 传输、ffmpeg 之前，先用真实的大目录把前端性能方案跑通；若扛不住，前面所有架构都要改。

---

## 环境现状（2026-09-17 在**本开发机**实测）

> 与计划 1 同样的告诫：这是某一台机器的快照，换机需重新核对。

| 项 | 状态 |
|---|---|
| Rust | 1.98.1，default `stable-x86_64-pc-windows-msvc` |
| MSVC | VS BuildTools 2022，`cl.exe` 14.44.35207 |
| Windows SDK | 10.0.26100.0 |
| WebView2 运行时 | 已装 |
| Node | v24.19.0 |
| git | 2.55 |
| 系统 | Windows 10（本机） |

**核对命令：**

```powershell
rustc --version; node --version
Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\Include" | ForEach-Object { $_.Name }
Test-Path "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
```

---

## 关键事实（2026-09-17 查证 Tauri 官方文档 / 源码，勿凭记忆改）

1. **自定义协议的 URL 形态跨平台不同。** 前端构造 URL 必须用 `convertFileSrc(path, "lpm")`（`@tauri-apps/api/core`），不要手拼协议串。
   - Windows / Android：`http://lpm.localhost/<path>`
   - macOS / iOS / Linux：`lpm://localhost/<path>`
   - 无论哪种，**处理函数收到的 `request.uri()` 都已被 wry 归一化**成 `lpm://localhost/<path>`，按统一形态解析即可。
2. **协议注册 API：** `tauri::Builder::register_asynchronous_uri_scheme_protocol(scheme, handler)`，处理函数签名：
   ```rust
   Fn(UriSchemeContext<'_, R>, http::Request<Vec<u8>>, UriSchemeResponder)
   ```
   在独立线程里干活后调 `responder.respond(http::Response<...>)`。
3. **Range 处理有官方参考实现：** Tauri 自带 `crates/tauri/src/protocol/asset.rs` 与 `examples/streaming/main.rs`。本计划的协议实现是它们的最小化改写，用 `http-range` crate 解析 Range。
4. **若设置了 CSP，必须显式放行协议。** Tauri 只在 `tauri.conf.json` 里配了 `app.security.csp` 时才启用 CSP；启用后要把 `lpm:` 与 `http://lpm.localhost` 加进 `default-src` / `img-src` / `media-src` / `connect-src`。
5. **目录结构：** `src-tauri/` 是标准 Cargo 项目；桌面入口 `src-tauri/src/main.rs` 调 `lib.rs` 的 `run()`；`capabilities/default.json` 是权限清单。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. `convertFileSrc` 对自定义协议（非内置 `asset`）在 Windows 上生成的确切 URL——先写个临时页面打印出来核对。
2. `tauri-plugin-dialog` 的权限标识符确切名字（`dialog:default` 还是 `dialog:allow-open`）——以插件 README 为准。
3. 应用自定义命令（`#[tauri::command]`）是否需要写进 capabilities——倾向**不需要**（权限系统管的是插件/核心命令），但必须实测确认。
4. Tauri / 插件的具体版本号——**以 `create-tauri-app` 实际生成的为准**，不要手填。

---

## 文件结构

```
livephoto-manager/
├── Cargo.toml                       修改：workspace members 加入 "src-tauri"
├── package.json                     新增：Vue3 + Vite + Tauri CLI
├── index.html                       新增
├── vite.config.ts                   新增
├── tsconfig.json / tsconfig.node.json 新增
├── src/                             新增：前端
│   ├── main.ts
│   ├── App.vue
│   ├── lib/lpm.ts                   convertFileSrc 封装
│   ├── stores/library.ts            Pinia，shallowRef 持有清单
│   ├── composables/useZoom.ts       离散档位 + DPR 上限 + 锚点跟随
│   └── components/PhotoGrid.vue     自写虚拟滚动网格
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/                       （由 create-tauri-app 生成）
│   └── src/
│       ├── main.rs                  桌面入口（不改）
│       ├── lib.rs                   Builder：注册协议 + 命令 + 状态
│       ├── protocol.rs              lpm:// 实现（Range / MIME / 允许根）
│       └── scanner.rs               扫描目录的纯逻辑 + 命令
└── docs/superpowers/plans/2026-09-17-stage1-tauri-skeleton.md   （本文件）
```

**边界说明：**
- `scanner.rs` 的目录遍历与过滤是**纯逻辑**（不依赖 Tauri），可单测。
- `protocol.rs` 的 Range 解析与响应构造抽成纯函数 `build_response(path, range_header, allowed_root) -> http::Response<Vec<u8>>`，可单测（用临时文件）。
- `probe` crate 完全不受影响；workspace 的 `cargo test -p probe` 必须继续全绿。

---

## Task 0: 环境核对 + 安装 Tauri CLI

**Files:** 无（环境变更）

- [ ] **Step 1: 核对工具链**

Run:
```powershell
rustc --version; node --version; npm --version
```
Expected: Rust 1.9x、Node v22 以上。

- [ ] **Step 2: 安装 JS 侧脚手架（不装全局，走 npx）**

Run:
```powershell
npx --yes create-tauri-app@latest --help
```
Expected: 打印 create-tauri-app 的帮助信息。若网络失败，记录错误后停止（不要改用镜像，先确认网络）。

---

## Task 1: 生成 Tauri 基线并移植进仓库

**Files:**
- Create: `package.json`、`index.html`、`vite.config.ts`、`tsconfig*.json`、`src/`、`src-tauri/`

**为什么在临时目录生成：** 本仓库根目录非空，`create-tauri-app` 会拒绝直接生成。先在临时目录生成一份**与当前版本匹配的**基线，再移植，避免手写版本号写错。

- [ ] **Step 1: 在临时目录生成 Vue+TS 模板**

Run:
```powershell
$tmp = "$env:TEMP\lpm-scaffold"
Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $tmp | Out-Null
cd $tmp
npx --yes create-tauri-app@latest lpm --template vue-ts --manager npm --yes
```
Expected: 生成 `$tmp\lpm\`，含 `src/`、`src-tauri/`、`package.json`、`vite.config.ts`、`tauri.conf.json`。

> 若 `--template vue-ts` / `--yes` 参数名不对，以 `--help` 输出为准。**不要跳过这一步手写模板。**

- [ ] **Step 2: 把基线的版本号抄下来**

Run:
```powershell
Get-Content "$env:TEMP\lpm-scaffold\lpm\package.json"
Get-Content "$env:TEMP\lpm-scaffold\lpm\src-tauri\Cargo.toml"
```
把 `@tauri-apps/cli`、`@tauri-apps/api`、`tauri`、`tauri-build` 的版本记在下方（后续所有依赖以这些为准）：

```
（实测，2026-09-17）@tauri-apps/cli ^2、@tauri-apps/api ^2、vue ^3.5.13、vite ^8.0.16、
typescript ~6.0.3、vue-tsc ^3.3.5；tauri 2.11.5、tauri-build 2.6.3。
另加：tauri-plugin-dialog 2.7.3（npm ^2.7.3）、pinia ^4.0.3、http-range 0.1.5、percent-encoding 2.3.2。
```

- [ ] **Step 3: 移植到仓库**

把基线里除 `node_modules`、`target`、`.git` 外的内容复制到仓库根，再删掉临时目录：

```powershell
$src = "$env:TEMP\lpm-scaffold\lpm"
$dst = "C:\Users\Raven\livephoto-manager"
robocopy $src $dst /E /XD node_modules target .git | Out-Null
Remove-Item -Recurse -Force "$env:TEMP\lpm-scaffold"
```

核对：仓库根出现 `src/`、`src-tauri/`、`package.json`。

- [ ] **Step 4: 调整 `package.json` 的应用名**

把 `"name"` 改为 `"liveporter"`（保持 `"private": true`）。**不要**填 `license` 字段（设计文档附录 A 决策 6 未定）。

- [ ] **Step 5: 改 `tauri.conf.json` 的 `productName` / `identifier`**

`productName`: `LivePorter`；`identifier`: `com.liveporter.app`（或用户偏好的反域名）。其余保持基线不动。

- [ ] **Step 6: 安装依赖并跑起来**

Run:
```powershell
cd C:\Users\Raven\livephoto-manager
npm install
npm run tauri dev
```
Expected: 首次编译数分钟；随后弹出一个 Tauri 窗口，显示模板默认页面。按 `Ctrl+Shift+I` 能开 DevTools。

> 若 `link.exe` 找不到或 WebView2 报错，回到计划 1 Task 1/2 的环境修复步骤。

- [ ] **Step 7: 提交**

```bash
git add package.json package-lock.json index.html vite.config.ts tsconfig.json tsconfig.node.json src src-tauri .gitignore
# .gitignore 需已忽略 node_modules 与 src-tauri/target（基线自带则跳过）
git commit -m "chore: scaffold tauri 2 + vue 3 + vite app"
```

---

## Task 2: 把 `src-tauri` 纳入 cargo workspace

**Files:**
- Modify: `Cargo.toml`
- Modify: `.gitignore`

**为什么：** 仓库根已有 `[workspace]`（计划 1 建的）。`src-tauri` 落在 workspace 根之下但不在 `members` 里，`cargo build` 会报 "current package believes it's in a workspace when it's not"。

- [ ] **Step 1: 加入 members**

Modify 根 `Cargo.toml`：
```toml
[workspace]
resolver = "2"
members = ["crates/probe", "src-tauri"]
```

- [ ] **Step 2: 确认 probe 未受影响**

Run:
```powershell
cargo test -p probe
cargo clippy -p probe --all-targets -- -D warnings
cargo fmt --check
```
Expected: 25 个测试仍全绿、clippy 干净、fmt 通过。

- [ ] **Step 3: 确认 tauri 构建正常**

Run:
```powershell
cargo check -p liveporter
```
（包名以 `src-tauri/Cargo.toml` 的 `[package].name` 为准，通常是 `liveporter` 或 `app`。）

Expected: 编译通过。

- [ ] **Step 4: 忽略 tauri 的产物**

确认 `.gitignore` 含：
```
target/
node_modules/
dist/
```
`src-tauri/target/` 已被 `target/` 覆盖（若 Cargo 把它放在 workspace 根的 `target/`）。核对 `git status` 里不出现 `target/`、`node_modules/`。

- [ ] **Step 5: 提交**

```bash
git add Cargo.toml Cargo.lock .gitignore
git commit -m "chore: include src-tauri in cargo workspace"
```

---

## Task 3: `lpm://` 自定义协议（Range + MIME + 允许根）

**Files:**
- Create: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`（加 `http-range`）
- Modify: `src-tauri/tauri.conf.json`（CSP）

**安全底线：** 协议只允许读取**已登记的库根目录**下的文件。请求路径 canonicalize 后必须落在允许根内，否则 403。

- [ ] **Step 1: 加依赖**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 加：
```toml
http-range = "0.1"
```
（版本以 `cargo add http-range` 实际写入为准。）

- [ ] **Step 2: 写失败的测试（纯函数部分）**

Create `src-tauri/src/protocol.rs`：
```rust
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use tauri::http::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE};
use tauri::http::status::StatusCode;
use tauri::http::{Request, Response};

/// 常见图片/视频扩展名到 MIME。只做最小映射，够探针与网格用。
pub fn mime_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "heic" | "heif" => "image/heic",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        _ => "application/octet-stream",
    }
}

/// 把请求路径解析成允许根内的真实路径。越界或不存在返回 None。
pub fn resolve_allowed(raw_path: &str, allowed_root: &Path) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(raw_path).decode_utf8_lossy();
    // 去掉前导 '/'，兼容 Windows 盘符（/C:/...）
    let trimmed = decoded.trim_start_matches('/');
    let candidate = PathBuf::from(trimmed);

    let canonical = std::fs::canonicalize(&candidate).ok()?;
    let root = std::fs::canonicalize(allowed_root).ok()?;
    if canonical.starts_with(&root) {
        Some(canonical)
    } else {
        None
    }
}

/// 构造响应：不做 Range 时全量返回，做 Range 时返回 206。
pub fn build_response(
    path: &Path,
    range_header: Option<&str>,
) -> std::io::Result<Response<Vec<u8>>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mime = mime_from_path(path);

    let builder = Response::builder().header(CONTENT_TYPE, mime);

    let Some(range_header) = range_header else {
        let mut buf = Vec::with_capacity(len as usize);
        file.read_to_end(&mut buf)?;
        return Ok(builder
            .header(CONTENT_LENGTH, len)
            .body(buf)
            .expect("static headers are valid"));
    };

    let ranges = match http_range::HttpRange::parse(range_header, len) {
        Ok(r) => r,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(CONTENT_RANGE, format!("bytes */{len}"))
                .body(Vec::new())
                .expect("static headers are valid"));
        }
    };

    // 单区间是浏览器 <img>/<video> 的常见情形；多区间直接拒绝（网格场景用不到）。
    if ranges.len() != 1 {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .expect("static headers are valid"));
    }

    let r = ranges[0];
    let start = r.start;
    let end = start + r.length - 1;
    if start >= len || end >= len || end < start {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .expect("static headers are valid"));
    }

    let nbytes = end + 1 - start;
    let mut buf = vec![0u8; nbytes as usize];
    file.seek(SeekFrom::Start(start))?;
    file.read_exact(&mut buf)?;

    Ok(builder
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
        .header(CONTENT_LENGTH, nbytes)
        .status(StatusCode::PARTIAL_CONTENT)
        .body(buf)
        .expect("static headers are valid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_maps_known_extensions() {
        assert_eq!(mime_from_path(Path::new("a.JPG")), "image/jpeg");
        assert_eq!(mime_from_path(Path::new("a.mov")), "video/quicktime");
        assert_eq!(mime_from_path(Path::new("a.bin")), "application/octet-stream");
    }

    #[test]
    fn full_response_has_length_and_mime() {
        let dir = std::env::temp_dir().join("lpm_proto_test");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.png");
        std::fs::write(&f, b"hello").unwrap();

        let resp = build_response(&f, None).unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.body().as_slice(), b"hello");
    }

    #[test]
    fn range_response_is_partial() {
        let dir = std::env::temp_dir().join("lpm_proto_test");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("y.bin");
        std::fs::write(&f, b"0123456789").unwrap();

        let resp = build_response(&f, Some("bytes=2-4")).unwrap();
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(resp.body().as_slice(), b"234");
        assert_eq!(
            resp.headers().get(CONTENT_RANGE).unwrap(),
            "bytes 2-4/10"
        );
    }

    #[test]
    fn resolve_rejects_path_outside_root() {
        let root = std::env::temp_dir().join("lpm_proto_root");
        std::fs::create_dir_all(&root).unwrap();
        let outside = std::env::temp_dir().join("lpm_proto_outside.txt");
        std::fs::write(&outside, b"x").unwrap();

        let escaped = format!("/{}", outside.to_string_lossy().replace('\\', "/"));
        assert!(resolve_allowed(&escaped, &root).is_none());
    }
}
```

`percent_encoding` 需要依赖：
```toml
percent-encoding = "2"
```
（版本以 `cargo add percent-encoding` 写入为准。）

- [ ] **Step 3: 运行测试，确认失败**

在 `lib.rs` 加 `mod protocol;` 之前，`cargo test -p liveporter protocol` 会因找不到模块而失败——这是预期的第一步。加上 `mod protocol;` 后应转为编译通过。

- [ ] **Step 4: 运行测试，确认通过**

Run:
```powershell
cargo test -p liveporter protocol
```
Expected: 4 个测试全绿。

- [ ] **Step 5: 在 Builder 里注册协议**

Modify `src-tauri/src/lib.rs`（保留模板原有的 `greet` 命令或删掉，二选一，别留半截）：
```rust
mod protocol;
mod scanner;
mod state;

use std::path::PathBuf;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .register_asynchronous_uri_scheme_protocol("lpm", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // 协议处理函数是同步的：把读盘放到独立线程，读完再 respond。
            std::thread::spawn(move || {
                let state = app.state::<state::AppState>();
                let root = state.allowed_root();
                let response = match root {
                    Some(root) => {
                        let raw = request.uri().path().to_string();
                        match protocol::resolve_allowed(&raw, &root) {
                            Some(path) => {
                                let range = request
                                    .headers()
                                    .get(tauri::http::header::RANGE)
                                    .and_then(|v| v.to_str().ok())
                                    .map(|s| s.to_string());
                                protocol::build_response(&path, range.as_deref())
                                    .unwrap_or_else(|e| internal_error(&e.to_string()))
                            }
                            None => forbidden(),
                        }
                    }
                    None => forbidden(),
                };
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![scanner::scan_dir])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn forbidden() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(tauri::http::StatusCode::FORBIDDEN)
        .body(Vec::new())
        .expect("static headers are valid")
}

fn internal_error(msg: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(tauri::http::StatusCode::INTERNAL_SERVER_ERROR)
        .header(tauri::http::header::CONTENT_TYPE, "text/plain")
        .body(msg.as_bytes().to_vec())
        .expect("static headers are valid")
}
```

> `state::AppState` 与 `scanner::scan_dir` 在 Task 4 建。若想先跑通协议，可临时用 `static` 根目录，但正式版必须是用户选定的根。

- [ ] **Step 6: 配置 CSP 放行协议**

Modify `src-tauri/tauri.conf.json` 的 `app.security.csp`（若基线未设 csp 则新增）：
```json
"csp": {
  "default-src": "'self' lpm: http://lpm.localhost",
  "img-src": "'self' lpm: http://lpm.localhost data: blob:",
  "media-src": "'self' lpm: http://lpm.localhost blob:",
  "connect-src": "ipc: http://ipc.localhost"
}
```

- [ ] **Step 7: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/src/protocol.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat(app): add lpm:// protocol with range support"
```

---

## Task 4: 目录扫描命令 + 应用状态

**Files:**
- Create: `src-tauri/src/scanner.rs`
- Create: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`（注册已有）

- [ ] **Step 1: 写纯逻辑 + 失败的测试**

Create `src-tauri/src/scanner.rs`：
```rust
use std::path::{Path, PathBuf};

use serde::Serialize;

const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImageItem {
    pub path: String,
    pub name: String,
    pub size: u64,
}

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 递归收集图片，按路径排序（稳定顺序，虚拟滚动的锚点依赖它）。
pub fn collect_images(root: &Path) -> Vec<ImageItem> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(dir: &Path, out: &mut Vec<ImageItem>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if is_image(&path) {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(ImageItem {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                size,
            });
        }
    }
}

#[tauri::command]
pub async fn scan_dir(path: String, state: tauri::State<'_, crate::state::AppState>) -> Result<Vec<ImageItem>, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("不是目录: {path}"));
    }
    state.set_allowed_root(root.clone());
    // 大目录遍历放到阻塞线程，别卡住 async 运行时。
    tauri::async_runtime::spawn_blocking(move || collect_images(&root))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_non_images() {
        assert!(is_image(Path::new("a.JPG")));
        assert!(is_image(Path::new("a.heic")));
        assert!(!is_image(Path::new("a.mov")));
        assert!(!is_image(Path::new("a.txt")));
        assert!(!is_image(Path::new("noext")));
    }

    #[test]
    fn collects_and_sorts_recursively() {
        let dir = std::env::temp_dir().join("lpm_scan_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("b.png"), b"b").unwrap();
        std::fs::write(dir.join("sub/a.jpg"), b"aa").unwrap();
        std::fs::write(dir.join("skip.mov"), b"x").unwrap();

        let items = collect_images(&dir);
        assert_eq!(items.len(), 2);
        assert!(items[0].path.ends_with("a.jpg"));
        assert!(items[1].path.ends_with("b.png"));
    }
}
```

Create `src-tauri/src/state.rs`：
```rust
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Default)]
pub struct AppState {
    allowed_root: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn set_allowed_root(&self, path: PathBuf) {
        *self.allowed_root.lock().expect("poisoned") = Some(path);
    }

    pub fn allowed_root(&self) -> Option<PathBuf> {
        self.allowed_root.lock().expect("poisoned").clone()
    }
}
```

- [ ] **Step 2: 运行测试，确认通过**

Run:
```powershell
cargo test -p liveporter
```
Expected: protocol 4 个 + scanner 2 个全绿。

- [ ] **Step 3: 加 dialog 插件依赖**

Run:
```powershell
cd src-tauri
cargo add tauri-plugin-dialog
```
（版本以实际写入为准。）并在 `lib.rs` 保留 `.plugin(tauri_plugin_dialog::init())`。

- [ ] **Step 4: 前端侧插件**

Run:
```powershell
cd C:\Users\Raven\livephoto-manager
npm install @tauri-apps/plugin-dialog
```
并在 `src-tauri/capabilities/default.json` 的 `permissions` 里加入 dialog 权限。**权限标识符以插件 README 为准**（可能是 `dialog:default` 或 `dialog:allow-open`），改完必须实测能弹出目录选择框。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/scanner.rs src-tauri/src/state.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/capabilities package.json package-lock.json
git commit -m "feat(app): add directory scanner command and app state"
```

---

## Task 5: 前端数据层 + `lpm://` URL 封装

**Files:**
- Create: `src/lib/lpm.ts`
- Create: `src/stores/library.ts`
- Modify: `src/main.ts`（装 Pinia）
- Create: `src/vite-env.d.ts`（若基线没有）

- [ ] **Step 1: 封装 URL**

Create `src/lib/lpm.ts`：
```ts
import { convertFileSrc } from "@tauri-apps/api/core";

/** 把绝对路径转成 lpm:// 协议 URL。Windows 上实际是 http://lpm.localhost/...，
 *  但必须走 convertFileSrc，不能手拼。 */
export function lpmUrl(absPath: string): string {
  return convertFileSrc(absPath, "lpm");
}
```

- [ ] **Step 2: 核对 convertFileSrc 的实际输出（待实测项 1）**

在 `App.vue` 里临时加一行 `console.log(lpmUrl("C:\\Windows\\Web\\Wallpaper\\Windows\\img0.jpg"))`，`npm run tauri dev` 打开 DevTools 看输出，确认是 `http://lpm.localhost/...` 形态，并把结果记在下方：

```
（已查证，2026-09-17，读 tauri 2.11.5 的 scripts/core.js，无需运行时即可确定）
Windows: http://lpm.localhost/<encodeURIComponent(绝对路径)>
例：C:\Users\Raven\a.jpg → http://lpm.localhost/C%3A%5CUsers%5CRaven%5Ca.jpg
协议处理函数收到的 request.uri().path() 为 /C%3A%5CUsers%5CRaven%5Ca.jpg，
经 percent-decode 后即 C:\Users\Raven\a.jpg，与 protocol::resolve_allowed 的实现一致。
```

- [ ] **Step 3: Pinia store（必须 shallowRef）**

Create `src/stores/library.ts`：
```ts
import { defineStore } from "pinia";
import { shallowRef, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface ImageItem {
  path: string;
  name: string;
  size: number;
}

export const useLibrary = defineStore("library", () => {
  // 设计 §8.3：上万条对象绝不能被 Vue 深层代理，必须 shallowRef。
  const items = shallowRef<ImageItem[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load(dir: string) {
    loading.value = true;
    error.value = null;
    try {
      items.value = await invoke<ImageItem[]>("scan_dir", { path: dir });
    } catch (e) {
      error.value = String(e);
      items.value = [];
    } finally {
      loading.value = false;
    }
  }

  return { items, loading, error, load };
});
```

- [ ] **Step 4: 装 Pinia**

`src/main.ts`：
```ts
import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

createApp(App).use(createPinia()).mount("#app");
```

- [ ] **Step 5: 提交**

```bash
git add src/lib/lpm.ts src/stores/library.ts src/main.ts package.json package-lock.json
git commit -m "feat(ui): add library store and lpm url helper"
```

---

## Task 6: 自写虚拟滚动网格

**Files:**
- Create: `src/components/PhotoGrid.vue`
- Modify: `src/App.vue`

**约束（设计 §8.1 决定 2）：** 固定尺寸瓦片，只算该渲染哪几行、绝对定位摆放。不引第三方虚拟滚动库。

- [ ] **Step 1: 写组件**

Create `src/components/PhotoGrid.vue`：
```vue
<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { lpmUrl } from "../lib/lpm";
import type { ImageItem } from "../stores/library";

const props = defineProps<{
  items: ImageItem[];
  columns: number;
  gap: number;
}>();

const scroller = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportH = ref(0);
const viewportW = ref(0);

// 瓦片边长由列数与容器宽算出（固定尺寸，便于虚拟化）
const tileW = computed(() => {
  const cols = Math.max(1, props.columns);
  return Math.floor((viewportW.value - props.gap * (cols + 1)) / cols);
});
const tileH = computed(() => tileW.value);

const rows = computed(() => {
  const cols = Math.max(1, props.columns);
  return Math.ceil(props.items.length / cols);
});
const rowStride = computed(() => tileH.value + props.gap);
const totalH = computed(() => rows.value * rowStride.value + props.gap);

const startRow = computed(() =>
  Math.max(0, Math.floor(scrollTop.value / rowStride.value) - 1),
);
const endRow = computed(() => {
  const visible = Math.ceil(viewportH.value / rowStride.value) + 2;
  return Math.min(rows.value, startRow.value + visible);
});

interface Tile {
  item: ImageItem;
  index: number;
  x: number;
  y: number;
  size: number;
}

const visible = computed<Tile[]>(() => {
  const cols = Math.max(1, props.columns);
  const out: Tile[] = [];
  for (let r = startRow.value; r < endRow.value; r++) {
    for (let c = 0; c < cols; c++) {
      const index = r * cols + c;
      if (index >= props.items.length) break;
      out.push({
        item: props.items[index],
        index,
        x: props.gap + c * (tileW.value + props.gap),
        y: props.gap + r * rowStride.value,
        size: tileW.value,
      });
    }
  }
  return out;
});

function onScroll() {
  scrollTop.value = scroller.value?.scrollTop ?? 0;
}

let ro: ResizeObserver | null = null;
onMounted(() => {
  const el = scroller.value;
  if (!el) return;
  ro = new ResizeObserver(() => {
    viewportH.value = el.clientHeight;
    viewportW.value = el.clientWidth;
  });
  ro.observe(el);
  viewportH.value = el.clientHeight;
  viewportW.value = el.clientWidth;
});
onBeforeUnmount(() => ro?.disconnect());

defineExpose({ scroller });
</script>

<template>
  <div ref="scroller" class="scroller" @scroll.passive="onScroll">
    <div class="canvas" :style="{ height: totalH + 'px' }">
      <div
        v-for="t in visible"
        :key="t.index"
        class="tile"
        :style="{
          width: t.size + 'px',
          height: t.size + 'px',
          transform: `translate(${t.x}px, ${t.y}px)`,
        }"
      >
        <img :src="lpmUrl(t.item.path)" :alt="t.item.name" loading="lazy" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.scroller {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  overflow-x: hidden;
  background: #111;
}
.canvas {
  position: relative;
  width: 100%;
}
.tile {
  position: absolute;
  top: 0;
  left: 0;
  overflow: hidden;
  background: #1c1c1c;
  will-change: transform;
}
.tile img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
</style>
```

- [ ] **Step 2: App.vue 接线（先固定列数）**

Modify `src/App.vue`：
```vue
<script setup lang="ts">
import { ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary } from "./stores/library";
import PhotoGrid from "./components/PhotoGrid.vue";

const lib = useLibrary();
const columns = ref(5);

async function pick() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === "string") await lib.load(dir);
}
</script>

<template>
  <div class="app">
    <header>
      <button @click="pick">选择文件夹</button>
      <span v-if="lib.loading">扫描中…</span>
      <span v-else-if="lib.error">{{ lib.error }}</span>
      <span v-else>{{ lib.items.length }} 张</span>
    </header>
    <main>
      <PhotoGrid :items="lib.items" :columns="columns" :gap="8" />
    </main>
  </div>
</template>

<style>
html, body, #app { height: 100%; margin: 0; }
.app { display: flex; flex-direction: column; height: 100%; background: #111; color: #eee; }
header { padding: 8px; display: flex; gap: 12px; align-items: center; }
main { position: relative; flex: 1; }
</style>
```

- [ ] **Step 3: 实测**

Run:
```powershell
npm run tauri dev
```
选一个图片较多的本地目录，Expected: 网格显示图片；滚动流畅；DevTools 里 `document.querySelectorAll('img').length` 远小于总条目数（证明虚拟化生效）。

- [ ] **Step 4: 提交**

```bash
git add src/components/PhotoGrid.vue src/App.vue
git commit -m "feat(ui): self-written virtual scrolling photo grid"
```

---

## Task 7: Ctrl+滚轮离散缩放 + DPR 上限 + 锚点跟随

**Files:**
- Create: `src/composables/useZoom.ts`
- Modify: `src/App.vue`

**设计依据：** §8.2（拦截 wheel、离散档位、锚点跟随）与 §7.4（最大放大档位按 DPR 限制，保证缩略图不糊）。

- [ ] **Step 1: 写 composable**

Create `src/composables/useZoom.ts`：
```ts
import { ref } from "vue";

// 离散档位：列数越多 = 瓦片越小 = 越"缩小"
const LEVELS = [3, 5, 7, 10, 14] as const;

/** 最大放大档位（最小的列数）受 DPR 限制：
 *  瓦片 CSS 边长 * dpr 不应超过缩略图边长 THUMB_PX（设计 §7.4，缩略图 512px）。
 *  目前没有缩略图、直接吃原图，故上限按 512 仍保守成立。 */
export const THUMB_PX = 512;

export function maxColumnsForDpr(viewportCssWidth: number, dpr: number): number {
  // 允许的最大瓦片边长（CSS 像素）
  const maxTileCss = THUMB_PX / Math.max(1, dpr);
  // 最少需要多少列才能让瓦片不超过该边长
  return Math.ceil(viewportCssWidth / maxTileCss);
}

export function useZoom(initial: number) {
  const columns = ref(initial);

  function allowedLevels(viewportCssWidth: number, dpr: number): number[] {
    const minCols = maxColumnsForDpr(viewportCssWidth, dpr);
    return LEVELS.filter((c) => c >= minCols);
  }

  /** 返回下一次缩放后的列数；到达边界时返回原值。delta<0 表示放大（列数变少）。 */
  function next(delta: number, viewportCssWidth: number, dpr: number): number {
    const levels = allowedLevels(viewportCssWidth, dpr);
    if (levels.length === 0) return LEVELS[LEVELS.length - 1];
    const cur = columns.value;
    if (delta < 0) {
      // 放大：选比当前小的最大档
      const smaller = [...levels].reverse().filter((c) => c < cur);
      return smaller[0] ?? levels[0];
    } else {
      // 缩小：选比当前大的最小档
      const bigger = levels.filter((c) => c > cur);
      return bigger[0] ?? levels[levels.length - 1];
    }
  }

  return { columns, next, allowedLevels };
}
```

- [ ] **Step 2: 在网格上做锚点跟随**

`PhotoGrid.vue` 增加一个方法，在列数变化前记录光标下的瓦片序号与相对位置，变化后调整 `scrollTop`：
```ts
// 在 PhotoGrid.vue 的 <script setup> 内追加
function captureAnchor(clientX: number, clientY: number) {
  const el = scroller.value;
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  const localX = clientX - rect.left + el.scrollLeft;
  const localY = clientY - rect.top + el.scrollTop;
  const cols = Math.max(1, props.columns);
  const row = Math.floor(localY / rowStride.value);
  const col = Math.floor((localX - props.gap) / (tileW.value + props.gap));
  const index = row * cols + Math.max(0, col);
  return { index, fracY: localY - (props.gap + row * rowStride.value) };
}

function restoreAnchor(anchor: { index: number; fracY: number } | null) {
  const el = scroller.value;
  if (!el || !anchor) return;
  const cols = Math.max(1, props.columns);
  const row = Math.floor(anchor.index / cols);
  el.scrollTop = Math.max(0, props.gap + row * rowStride.value + anchor.fracY);
}
```
并在组件 `defineExpose` 里导出 `captureAnchor` / `restoreAnchor`。

- [ ] **Step 3: App.vue 拦截 wheel**

在 `App.vue` 里对 `main` 加 `wheel` 监听（`{ passive: false }`），`preventDefault`，按 `e.deltaY` 符号调 `next()`，缩放前后调用网格的 `captureAnchor`/`restoreAnchor`：
```ts
import { nextTick } from "vue";
import { useZoom } from "./composables/useZoom";

const zoom = useZoom(5);
const grid = ref<InstanceType<typeof PhotoGrid> | null>(null);

async function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return; // 只有 Ctrl+滚轮才缩放
  e.preventDefault();
  const anchor = grid.value?.captureAnchor(e.clientX, e.clientY) ?? null;
  const dpr = window.devicePixelRatio || 1;
  const vw = window.innerWidth;
  zoom.columns.value = zoom.next(e.deltaY, vw, dpr);
  await nextTick();
  grid.value?.restoreAnchor(anchor);
}
```
`main` 元素上用 `@wheel="onWheel"`；由于需要 `passive:false`，用模板修饰符不够，改为 `onMounted` 里 `el.addEventListener("wheel", onWheel, { passive: false })`。

- [ ] **Step 4: 实测**

Expected:
- 不按 Ctrl 滚轮 → 正常滚动，不缩放。
- 按住 Ctrl 滚轮 → 列数在 3/5/7/10/14 间跳变，**光标下的那张图基本不动**（锚点跟随生效）。
- 在高 DPI 屏上，最大放大档受限制（不会出现 3 列糊图）。

- [ ] **Step 5: 提交**

```bash
git add src/composables/useZoom.ts src/components/PhotoGrid.vue src/App.vue
git commit -m "feat(ui): ctrl+wheel discrete zoom with dpr cap and anchor follow"
```

---

## Task 8: 大目录性能实测

**Files:**
- Create: `docs/superpowers/notes/2026-09-17-grid-perf.md`（实测记录）

- [ ] **Step 1: 准备一个大目录**

用真实的大图片目录（本机 `D:\QQ\` 下有足够量的图片）。记录条目数。

- [ ] **Step 2: 量四个指标**

打开 DevTools 的 Performance / Memory，记录：

| 指标 | 记录 |
|---|---|
| 条目总数 | （待填） |
| 首次扫描耗时（点按钮到出网格） | （待填） |
| 滚动帧率（快速滚动 5 秒，是否掉帧） | （待填） |
| JS 堆内存（滚动 30 秒后） | （待填） |
| 同时存在的 `<img>` 数量 | （待填） |

- [ ] **Step 3: 判定**

- 若滚动明显掉帧或内存随滚动持续增长 → **前端方案需调整**（例如瓦片用 `content-visibility`、限制 `img` 并发加载、降低同时挂载数量）。把结论写进设计文档 §8。
- 若通过 → 把数据写进上面的 notes 文件，并在设计文档 §8 标注"已实测"。

- [ ] **Step 4: 提交**

```bash
git add docs/superpowers/notes/2026-09-17-grid-perf.md docs/superpowers/specs/2026-09-17-liveporter-design.md
git commit -m "docs: record grid virtualization performance measurements"
```

---

## Task 9: 收尾验证

- [ ] **Step 1: Rust 侧**

Run:
```powershell
cargo test -p probe
cargo test -p liveporter
cargo clippy -p probe -p liveporter --all-targets -- -D warnings
cargo fmt --check
```
Expected: 全绿、clippy 干净、fmt 通过。

- [ ] **Step 2: 前端侧**

Run:
```powershell
npx vue-tsc --noEmit
npm run build
```
Expected: 类型检查通过、构建成功。

- [ ] **Step 3: 更新 AGENTS.md 进度**

把"计划 2"标记为完成，写下一步（计划 3：SQLite 索引 + 库目录管理）。

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "chore: stage 1 verification and progress update"
```

---

## 完成判据

本计划在以下全部成立时才算完成：

- [ ] `npm run tauri dev` 能起窗口；选择文件夹后渲染出图片网格
- [ ] `lpm://` 协议支持 Range（视频 `<video>` 或 `curl -r` 能拿到 206）
- [ ] 协议拒绝允许根之外的路径（有单测）
- [ ] Ctrl+滚轮离散缩放且锚点跟随，不按 Ctrl 时正常滚动
- [ ] 最大放大档位受 DPR 限制
- [ ] 网格同时挂载的 `<img>` 数远小于总条目数
- [ ] 大目录实测数据已记录
- [ ] `cargo test / clippy / fmt`、`vue-tsc`、`npm run build` 全绿
- [ ] **未引入** SQLite、WPD、ffmpeg 相关代码

## 明确不在本计划内

- 不建 SQLite 索引、不做库目录管理（计划 3）
- 不碰 WPD/设备（计划 3/4）
- 不接 ffmpeg、不生成缩略图/预览片（计划 3/4）
- 不做悬停播放（需要预览片，计划 4）
- 不做时间线分组、搜索筛选、多选导出、删除（计划 4）
- 不处理 HEIC（Chromium 解不了，等 ffmpeg）

---

## 自查记录

**规格覆盖：** 对应设计文档 §13 第 2 步与 §8（前端架构与性能）的三条硬性决定——唯一 `<video>` 未涉及（留计划 4）、自写虚拟滚动（Task 6）、`lpm://` 不走 IPC（Task 3/5）。§7.4 的 DPR 上限在 Task 7。

**类型一致性：** `ImageItem`（Rust `scanner.rs` 与 TS `stores/library.ts`）字段名对齐：`path/name/size`。`lpmUrl` 只接受绝对路径字符串。

**已知不确定点（实现时必须查证，不得凭记忆写）：**

1. `convertFileSrc` 对自定义协议在 Windows 上的确切输出（Task 5 Step 2 有实测步骤）
2. `tauri-plugin-dialog` 的权限标识符（Task 4 Step 4）
3. 应用自定义命令是否需要 capabilities 条目（关键事实第 3 条，实现时实测）
4. 各依赖的确切版本号——一律以 `create-tauri-app` / `cargo add` / `npm install` 实际写入为准
5. `tauri::http` 是否完整重导出 `http` 与 `Response::builder`——以编译结果为准；若不足则直接依赖 `http` crate

**对计划 1 的承接：** 本计划不修改 `crates/probe`；Task 2 明确要求 probe 的验证保持全绿。
