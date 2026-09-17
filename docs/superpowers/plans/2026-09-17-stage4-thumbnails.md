# 阶段 4：ffmpeg 集成 + 缩略图 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 接入 ffmpeg（子进程），在导入完成后为每个条目生成 512px WebP 缩略图写进 `thumbs/`，网格改用缩略图显示。**直接解决当前 HEIC 在 WebView2 里破图的问题**（Chromium 解不了 HEIC）。

**Architecture:** 沿用设计 §6 的管道：传输完成（`status=copied`）后，紧接着生成缩略图，成功后状态推进到 `transcoded`。ffmpeg 调用封装在 `ffmpeg.rs`（只负责定位二进制、拼参数、执行），缩略图逻辑在 `thumb.rs`（命名、取源、落盘），两者都可脱离设备单测（参数构造、哈希、假执行器）。**预览片不在本计划内**（计划 6）。

**Tech Stack:** Rust 2021 / `sha2`（缩略图内容哈希）/ Tauri 2（已有）/ ffmpeg 静态构建（子进程）

**为什么现在做（设计 §13 第 5 步、§7.1）：** 阶段 3 导入的是 **HEIC**（iPhone 保留原件后的原始格式），WebView2 解不了 → 网格里全是破图。缩略图从「优化项」变成「必需项」。

---

## 关键事实（2026-09-17 查证，勿凭记忆改）

1. **ffmpeg 能解 HEIC**：`libavformat/mov.c` 的「demux still HEIC images」补丁（ffmpeg-devel 2023-10）已合入；只要构建带 HEVC 解码器即可读 `.HEIC`。现代 Windows 静态构建（gyan.dev 8.x、BtbN）都满足。
2. **Windows 静态构建都是 GPLv3**（gyan.dev 明确标注）。与项目已定的 GPL-3.0-or-later 一致；捆绑时须随附其许可证与源码获取途径（见根 `THIRD_PARTY_NOTICES.md`）。
3. **子进程调用**：用 `std::process::Command` 直接调 `ffmpeg.exe` 即可（设计 §3/§11「子进程调用」）。**不**引入 Tauri sidecar 插件（那要额外权限配置；正式打包时再决定，见计划 9）。
4. 缩略图规格（设计 §7.1）：**512px 宽 WebP**。
5. 状态机（设计 §6.1）：`pending(0) → copied(1) → transcoded(2) → done(3)`。本计划把「缩略图成功」对应到 `transcoded`(2)。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. 所选 ffmpeg 构建是否含 **libwebp 编码器**（`ffmpeg -encoders | findstr webp`）。若 `ffmpeg-release-essentials` 不含，则改用完整构建，或退化为 JPEG（并记录偏差）。
2. HEIC 解码的实际可用性（`ffmpeg -i 某.HEIC out.png` 能否成功）——Task 0 用真实文件实测。
3. `libwebp` 的质量参数是 `-q:v` 还是 `-quality`——以 ffmpeg 文档/实测为准。
4. `-ss 0` 对 MOV 取首帧是否稳定；若首帧黑屏，改用 `-ss 0.5`。

---

## 开发期 ffmpeg 的放置（不进仓库）

- 二进制**不提交进 git**（体积大 + GPL 来源需可追溯）。
- 约定查找顺序（`ffmpeg.rs` 实现）：
  1. 环境变量 `LIVEPORTER_FFMPEG`（开发/测试用，指向 exe 绝对路径）；
  2. 与主程序同目录的 `ffmpeg.exe`；
  3. 主程序同目录下 `resources/ffmpeg.exe`（正式打包布局，计划 9 落实）。
- 在 `.gitignore` 增加 `ffmpeg.exe`、`resources/`（避免误提交）。

---

## 文件结构

```
src-tauri/src/
├── ffmpeg.rs            新增：定位 ffmpeg、构造参数、执行
├── thumb.rs             新增：缩略图命名（内容哈希）与生成
├── importer.rs          修改：传输成功后生成缩略图，推进到 transcoded
├── db.rs                修改：写 thumb_path / status=2
├── commands.rs          修改：generate_thumbs 命令（回填已有库）
└── lib.rs               修改：注册命令

src/
└── stores/library.ts    修改：网格优先用 thumb_path
```

**边界说明：** `ffmpeg.rs` 的「参数构造」与 `thumb.rs` 的「哈希/命名」是纯函数，单测不需要真 ffmpeg；真执行部分用 `#[ignore]` 冒烟测试（需本机有 ffmpeg）。

---

## Task 0: 取得 ffmpeg 并实测 HEIC 解码与 WebP 编码

**Files:** 无（环境 + 一次性验证）；Modify `.gitignore`

- [ ] **Step 1: 下载静态构建（开发用）**

用 gyan.dev 的 release essentials（约 32 MB 的 7z；或 zip）。解压后把 `ffmpeg.exe` 放到一个开发目录，例如 `C:\Users\Raven\livephoto-manager\src-tauri\ffmpeg.exe`。

> 若 `ffmpeg-release-essentials` 不含 `libwebp`，改用 release-full。

- [ ] **Step 2: 实测 HEIC 解码**

拿一个真实 HEIC（可从手机导一个，或用阶段 3 导入库里的文件）：
```powershell
& .\src-tauri\ffmpeg.exe -hide_banner -i "D:\path\IMG_0001.HEIC" -frames:v 1 -y out.png
```
Expected: 生成 `out.png` 且能打开。

- [ ] **Step 3: 实测 WebP 编码器**

```powershell
& .\src-tauri\ffmpeg.exe -hide_banner -encoders | Select-String -Pattern "webp"
```
Expected: 有 `libwebp`（或 `webp`）。

- [ ] **Step 4: 记录结论到下方**

```
（实测，2026-09-17）ffmpeg 9.0.1-essentials_build（gyan.dev）。
含 libwebp 与 libwebp_anim 编码器；含 hevc 解码器。
HEIC 解码实测成功：nokiatech 的 autumn_1440x960.heic（293,608 B）→ WebP 成功。
本机路径：C:\Users\Raven\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg.Essentials_..._8wekyb3d8bbwe\ffmpeg-9.0.1-essentials_build\bin\ffmpeg.exe
```

- [ ] **Step 5: `.gitignore` 增加排除**

```
# 开发期 ffmpeg（不提交，见 THIRD_PARTY_NOTICES.md）
src-tauri/ffmpeg.exe
src-tauri/resources/
```

- [ ] **Step 6: 提交**

```bash
git add .gitignore
git commit -m "chore: ignore local ffmpeg binary"
```

---

## Task 1: `ffmpeg.rs`——定位、参数、执行

**Files:** Create `src-tauri/src/ffmpeg.rs`；Modify `src-tauri/src/lib.rs`（加 `mod ffmpeg;`）

- [ ] **Step 1: 写失败的测试（参数构造是纯函数）**

```rust
use std::path::{Path, PathBuf};

/// 定位 ffmpeg.exe。顺序见计划文档「开发期 ffmpeg 的放置」。
pub fn find_ffmpeg() -> anyhow::Result<PathBuf> {
    if let Some(p) = std::env::var_os("LIVEPORTER_FFMPEG") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        anyhow::bail!("LIVEPORTER_FFMPEG 指向的文件不存在: {p:?}");
    }
    let exe_dir = std::env::current_exe()?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    for cand in [exe_dir.join("ffmpeg.exe"), exe_dir.join("resources").join("ffmpeg.exe")] {
        if cand.is_file() {
            return Ok(cand);
        }
    }
    anyhow::bail!("未找到 ffmpeg.exe（可设 LIVEPORTER_FFMPEG 指定路径）")
}

/// 从静态图生成 512px WebP 缩略图的参数（不含程序名）。
pub fn still_thumb_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(), "error".into(),
        "-y".into(),
        "-i".into(), input.to_string_lossy().into_owned(),
        "-vf".into(), "scale=512:-2".into(),
        "-frames:v".into(), "1".into(),
        "-c:v".into(), "libwebp".into(),
        "-q:v".into(), "80".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 从视频取首帧生成缩略图的参数。
pub fn movie_thumb_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(), "error".into(),
        "-y".into(),
        "-ss".into(), "0".into(),
        "-i".into(), input.to_string_lossy().into_owned(),
        "-vf".into(), "scale=512:-2".into(),
        "-frames:v".into(), "1".into(),
        "-c:v".into(), "libwebp".into(),
        "-q:v".into(), "80".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 执行 ffmpeg，失败时带上 stderr。
pub fn run(ffmpeg: &Path, args: &[String]) -> anyhow::Result<()> {
    let output = std::process::Command::new(ffmpeg).args(args).output()?;
    if !output.status.success() {
        anyhow::bail!(
            "ffmpeg 失败 ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_args_scale_to_512_and_webp() {
        let a = still_thumb_args(Path::new("in.heic"), Path::new("out.webp"));
        assert!(a.contains(&"scale=512:-2".to_string()));
        assert!(a.contains(&"libwebp".to_string()));
        assert_eq!(a.last().unwrap(), "out.webp");
    }

    #[test]
    fn movie_args_seek_before_input() {
        let a = movie_thumb_args(Path::new("in.mov"), Path::new("out.webp"));
        let ss = a.iter().position(|x| x == "-ss").unwrap();
        let i = a.iter().position(|x| x == "-i").unwrap();
        assert!(ss < i, "-ss 应在 -i 之前以快速定位");
    }
}
```

在 `lib.rs` 加 `mod ffmpeg;`。

- [ ] **Step 2: 运行测试，确认通过**

Run: `cargo test -p liveporter ffmpeg`
Expected: 2 个通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/ffmpeg.rs src-tauri/src/lib.rs
git commit -m "feat(app): add ffmpeg locator and argument builders"
```

---

## Task 2: `thumb.rs`——内容哈希命名 + 生成

**Files:** Create `src-tauri/src/thumb.rs`；Modify `src-tauri/src/lib.rs`、`src-tauri/Cargo.toml`（加 `sha2`）

- [ ] **Step 1: 加依赖**

Run:
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo add sha2 --package liveporter
```
记录实际版本：

```
（实测）sha2 0.11.0（其依赖 digest 0.11.3）
```

- [ ] **Step 2: 写失败的测试**

```rust
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::ffmpeg;

/// 内容派生的廉价哈希：`文件大小 + 前 64KiB` 的 SHA-256，取前 24 个十六进制字符。
/// 不读全文件（视频可能很大），但内容一变哈希必变。
pub fn content_hash(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)
        .with_context(|| format!("打开 {path:?} 失败"))?;
    let size = f.metadata()?.len();
    let mut head = vec![0u8; 64 * 1024];
    let n = f.read(&mut head)?;

    let mut h = Sha256::new();
    h.update(size.to_le_bytes());
    h.update(&head[..n]);
    let hex = format!("{:x}", h.finalize());
    Ok(hex[..24].to_string())
}

/// 缩略图文件名：`<hash>.webp`。
pub fn thumb_file_name(hash: &str) -> String {
    format!("{hash}.webp")
}

/// 为一张静态图生成缩略图，返回缩略图路径。
pub fn make_thumb_for_still(
    ffmpeg_bin: &Path,
    input: &Path,
    thumbs_dir: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(thumbs_dir)?;
    let out = thumbs_dir.join(thumb_file_name(&content_hash(input)?));
    // 已存在（同一内容）则直接复用，天然去重。
    if out.is_file() {
        return Ok(out);
    }
    let tmp = out.with_extension("webp.part");
    ffmpeg::run(ffmpeg_bin, &ffmpeg::still_thumb_args(input, &tmp))?;
    std::fs::rename(&tmp, &out)?;
    Ok(out)
}

/// 为视频取首帧生成缩略图。
pub fn make_thumb_for_movie(
    ffmpeg_bin: &Path,
    input: &Path,
    thumbs_dir: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(thumbs_dir)?;
    let out = thumbs_dir.join(thumb_file_name(&content_hash(input)?));
    if out.is_file() {
        return Ok(out);
    }
    let tmp = out.with_extension("webp.part");
    ffmpeg::run(ffmpeg_bin, &ffmpeg::movie_thumb_args(input, &tmp))?;
    std::fs::rename(&tmp, &out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_content_sensitive() {
        let dir = std::env::temp_dir().join("lpm_thumb_hash");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        std::fs::write(&a, b"hello").unwrap();
        std::fs::write(&b, b"hello").unwrap();
        assert_eq!(content_hash(&a).unwrap(), content_hash(&b).unwrap());

        std::fs::write(&b, b"hellp").unwrap();
        assert_ne!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn thumb_name_has_webp_extension() {
        assert_eq!(thumb_file_name("abc123"), "abc123.webp");
    }
}
```

在 `lib.rs` 加 `mod thumb;`。

- [ ] **Step 3: 运行测试，确认通过**

Run: `cargo test -p liveporter thumb`
Expected: 2 个通过。

- [ ] **Step 4: 加真 ffmpeg 的冒烟测试（ignored）**

```rust
    /// 需要本机有 ffmpeg：设 LIVEPORTER_FFMPEG 后跑
    ///   cargo test -p liveporter real_thumb_smoke -- --ignored --nocapture
    #[test]
    #[ignore = "requires ffmpeg"]
    fn real_thumb_smoke() {
        let bin = ffmpeg::find_ffmpeg().unwrap();
        let input = std::env::var("LPM_THUMB_INPUT")
            .expect("设 LPM_THUMB_INPUT 指向一个 HEIC/JPG/MOV");
        let out_dir = std::env::temp_dir().join("lpm_thumb_smoke");
        let _ = std::fs::remove_dir_all(&out_dir);
        let p = if input.to_ascii_lowercase().ends_with(".mov") {
            make_thumb_for_movie(&bin, Path::new(&input), &out_dir).unwrap()
        } else {
            make_thumb_for_still(&bin, Path::new(&input), &out_dir).unwrap()
        };
        println!("thumb = {p:?} ({} bytes)", std::fs::metadata(&p).unwrap().len());
        assert!(std::fs::metadata(&p).unwrap().len() > 0);
    }
```

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/thumb.rs src-tauri/src/lib.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "feat(app): generate 512px webp thumbnails with content hash naming"
```

---

## Task 3: 接入导入管道（copied → transcoded）

**Files:** Modify `src-tauri/src/importer.rs`、`src-tauri/src/db.rs`

**设计取舍：** 缩略图失败**不**让条目变 `failed`（文件本身已完好），只保留 `copied` 并写 `error` 说明缩略图失败原因。

- [ ] **Step 1: 加缩略图抽象（便于单测）**

在 `importer.rs`：
```rust
/// 缩略图生成的抽象：真实实现调 ffmpeg，测试用假的。
pub trait Thumbs {
    fn make(&mut self, source: &Path, thumbs_dir: &Path) -> anyhow::Result<PathBuf>;
}
```

在 `run_tasks` 增加参数 `thumbs: &mut dyn Thumbs`，并在条目成功传输、`set_asset_status(STATUS_COPIED)` 之后：
```rust
// 生成缩略图；失败只记录，不把条目判为失败（文件已完好）。
let source = asset
    .still
    .as_ref()
    .map(|f| PathBuf::from(&f.path))
    .or_else(|| asset.movie.as_ref().map(|f| PathBuf::from(&f.path)));
if let Some(src) = source {
    match thumbs.make(&src, thumbs_dir) {
        Ok(tp) => {
            crate::db::set_thumb_path(conn, device_id, &task.base_name, task.taken_at, &tp.to_string_lossy())?;
            crate::db::set_asset_status(conn, device_id, &task.base_name, task.taken_at, STATUS_TRANSCODED, None)?;
        }
        Err(e) => {
            crate::db::set_asset_status(
                conn, device_id, &task.base_name, task.taken_at, STATUS_COPIED,
                Some(&format!("缩略图失败: {e:#}")),
            )?;
        }
    }
}
```
新增常量 `pub const STATUS_TRANSCODED: i64 = 2;`。

- [ ] **Step 2: `db.rs` 加 `set_thumb_path`**

```rust
pub fn set_thumb_path(
    conn: &Connection,
    device_id: i64,
    base_name: &str,
    taken_at: i64,
    thumb_path: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE asset SET thumb_path=?1, updated_at=?2
         WHERE device_id=?3 AND base_name=?4 AND taken_at=?5",
        params![thumb_path, now_epoch(), device_id, base_name, taken_at],
    )?;
    Ok(())
}
```

- [ ] **Step 3: 更新现有测试（加假 Thumbs）**

给 `run_tasks_marks_status_and_survives_failure` 传一个假 `Thumbs`（在临时目录写个空文件即可），并把断言从 `STATUS_COPIED` 改为 `STATUS_TRANSCODED`（成功条目）。新增一个测试：**缩略图失败时条目仍为 `copied` 且 `error` 含 "缩略图失败"**。

- [ ] **Step 4: 运行测试**

Run: `cargo test -p liveporter importer`
Expected: 通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/importer.rs src-tauri/src/db.rs
git commit -m "feat(app): generate thumbnails during import and advance status"
```

---

## Task 4: 回填命令 `generate_thumbs`（对已有库补缩略图）

**Files:** Modify `src-tauri/src/db.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`

- [ ] **Step 1: `db.rs` 加「列出缺缩略图的条目」**

```rust
#[derive(Debug, Clone)]
pub struct ThumbJob {
    pub base_name: String,
    pub taken_at: i64,
    pub source_path: String,
}

pub fn assets_missing_thumbs(conn: &Connection) -> rusqlite::Result<Vec<ThumbJob>> {
    let mut stmt = conn.prepare(
        "SELECT base_name, taken_at, coalesce(still_path, movie_path)
         FROM asset
         WHERE missing = 0
           AND (thumb_path IS NULL OR thumb_path = '')
           AND (still_path IS NOT NULL OR movie_path IS NOT NULL)",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ThumbJob {
            base_name: r.get(0)?,
            taken_at: r.get(1)?,
            source_path: r.get(2)?,
        })
    })?;
    rows.collect()
}
```

- [ ] **Step 2: 命令**

```rust
#[derive(serde::Serialize)]
pub struct ThumbSummary {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
}

#[tauri::command]
pub async fn generate_thumbs(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ThumbSummary, String> {
    // 与导入一样放到阻塞线程；thumbs 目录从当前库取。
    // 实现要点：ffmpeg::find_ffmpeg() 一次；逐条 make；成功 set_thumb_path + status=transcoded。
    // 进度事件："thumbs://progress"，payload { total, done, failed }。
}
```
在 `lib.rs` `generate_handler!` 加 `commands::generate_thumbs`。

- [ ] **Step 3: 编译**

Run: `cargo check -p liveporter`
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/db.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(app): add backfill command for thumbnails"
```

---

## Task 5: 前端网格改用缩略图

**Files:** Modify `src/stores/library.ts`、`src/App.vue`

- [ ] **Step 1: store 暴露 thumb_path**

`AssetRow` 增加 `thumb_path: string | null`；`imageItems` 映射时：
```ts
.filter((a) => !a.missing && (a.thumb_path || a.still_path))
.map((a) => ({ path: (a.thumb_path || a.still_path) as string, name: a.base_name, size: 0 }));
```
`list_assets` 的 SQL 需 select `thumb_path`（在 `page_assets` 里补上）。

- [ ] **Step 2: 加「生成缩略图」按钮**

与「重建索引」并列，调用 `generate_thumbs`，显示进度。**不做样式打磨。**

- [ ] **Step 3: 类型检查与构建**

Run: `npm run build`
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src/stores/library.ts src/App.vue
git commit -m "feat(ui): display thumbnails in the grid"
```

---

## Task 6: 实测

**Files:** Create `docs/superpowers/notes/2026-09-17-thumbs-smoke.md`

- [ ] **Step 1: 本地实测（不需要手机）**

1. 把阶段 3 导入的库（或手工放的 HEIC）作为库打开；
2. 点「生成缩略图」，观察 `thumbs/` 下生成 `.webp`，且 `asset.thumb_path` 被写入、`status` 变 2；
3. 网格里 HEIC 不再破图（显示的是 WebP 缩略图）。

- [ ] **Step 2: 记录**

记录：缩略图耗时/张（512px HEIC→WebP）、缩略图体积、是否命中 `libwebp`、遇到的问题。

- [ ] **Step 3: 提交**

```bash
git add docs/superpowers/notes/2026-09-17-thumbs-smoke.md
git commit -m "docs: record thumbnail generation smoke test"
```

---

## Task 7: 收尾验证

- [ ] **Step 1: Rust 侧**

Run:
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test -p probe -p liveporter
cargo clippy -p probe -p liveporter --all-targets -- -D warnings
cargo fmt --check
```
Expected: 全绿。

- [ ] **Step 2: 前端侧**

Run:
```powershell
npx vue-tsc --noEmit
npm run build
```
Expected: 通过。

- [ ] **Step 3: 更新 AGENTS.md 进度与文档索引**

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "chore: stage 4 verification and progress update"
```

---

## 完成判据

- [ ] `ffmpeg.rs` 能定位二进制、构造正确参数（有单测）
- [ ] `thumb.rs` 内容哈希稳定且对内容敏感、缩略图命名正确（有单测）
- [ ] 缩略图生成对 HEIC 与 MOV 都可用（`#[ignore]` 冒烟 + 实测）
- [ ] 导入后状态推进到 `transcoded(2)` 且 `thumb_path` 写入；缩略图失败不误判为导入失败
- [ ] 回填命令能为已有库补缩略图
- [ ] 网格显示缩略图（HEIC 不再破图），且走 `lpm://` 不走 IPC
- [ ] `thumbs/` 用内容哈希命名，重复内容天然去重
- [ ] `cargo test / clippy / fmt`、`vue-tsc`、`npm run build` 全绿
- [ ] **未做**预览片、悬停播放、UUID 校验、诊断报告（计划 6/7）

## 明确不在本计划内

- 预览片（480p H.264，按需 + 350ms 防抖）与悬停播放（全局单 `<video>`）→ 计划 6
- ContentIdentifier 校验、integrity 1/2/5、诊断报告、iCloud 检测 → 计划 7
- 时间线分组、搜索筛选、多选导出、删除 → 计划 8
- ffmpeg 裁剪构建与随包分发 → 计划 9
- 大图点开看（设计 §7.2 第三行）→ 计划 6

---

## 已知风险与取舍

1. **ffmpeg 构建是否含 `libwebp`** —— Task 0 实测；若不含则改完整构建或退化为 JPEG，并记录偏差。
2. **HEIC 解码依赖构建带 HEVC 解码器** —— 实测确认；若个别 HEIC 解不了，记录为个例。
3. **缩略图串行 vs 并发**：导入池计划（设计 §7.1「CPU 核数-1」）本计划先做**串行**，实测耗时后再决定是否并发。
4. **哈希只用「大小 + 前 64KiB」**：极端情况下内容不同但大小与前 64KiB 相同会撞哈希。对本用途（同一文件重导）足够；若要更强，后续改为全文件哈希。

## 自查记录

**规格覆盖：** 设计 §13 第 5 步（仅缩略图部分）、§7.1 第一行（缩略图规格）、§4（thumbs 命名）、§6（管道状态推进）。

**类型一致性：** `Thumbs` trait 的 `make` 返回 `PathBuf`；`db::ThumbJob` 与 `commands::ThumbSummary` 字段将序列化给前端，按 snake_case 对齐。

**已知不确定点（实现时必须查证）：**

1. ffmpeg 构建的 `libwebp`/HEVC 情况（Task 0）
2. `libwebp` 质量参数名（`-q:v` vs `-quality`）
3. MOV 首帧 `-ss 0` 是否会黑屏
4. `tauri::Emitter` 发进度事件的 API（沿用阶段 3 `import://progress` 的写法）
