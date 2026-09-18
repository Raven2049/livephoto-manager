# 阶段 6：UUID 校验 + 异常分类 + iCloud 手动开关 + 诊断报告 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把设计 §10 的「异常项分类」真正落地：读实况照片的 `ContentIdentifier`（UUID），校验静态图与视频是否一致，据此把 `integrity` 分成 0/1/2/3/4/5；提供 iCloud 场景的 v1「手动开关 + 体积合理性」；并具备 **导出诊断报告** 的能力（开源项目刚需）。

**Architecture:** 纯逻辑（标识解析、完整性判定）放在 `livephoto.rs`，用合成字节做单测，不依赖真机；从视频取标识走 `ffprobe`（已有 ffmpeg），从静态图取标识用**签名扫描 + 手写 TIFF/MakerNote 解析**。整体接入导入管道（传输后判定）并提供回填命令。

**Tech Stack:** Rust 2021 / `ffprobe`（随 ffmpeg）/ 手写 EXIF MakerNote 解析 / Tauri 2（已有）

**为什么现在做（设计 §13 第 6 步、§10、§6.4）：** 到目前 `integrity` 只区分了「配对/仅静态/仅视频」，并没有真正读 UUID 校验；诊断报告缺失会让开源后的 bug 报告无法定位。

---

## 关键事实（2026-09-17 查证，勿凭记忆改）

1. **实况照片的配对标识**（设计 §2.1、ExifTool Apple.pm）：
   - 静态图：**Apple MakerNote tag `0x0011`**（ExifTool 名 `ContentIdentifier`，也称 `MediaGroupUUID`）。
   - 视频：`moov/meta` 下的 **`com.apple.quicktime.content.identifier`**。
2. **视频侧可直接用 ffprobe**：`ffprobe` 会把该键解析成 **format tag**，因此
   ```powershell
   ffprobe -v error -show_entries format_tags=com.apple.quicktime.content.identifier -of default=nw=1:nk=1 <file>
   ```
   直接打印 UUID（或空行）。**Task 0 用真机素材实测确认。**
3. **静态图侧 ffprobe 读不到 MakerNote**（ffprobe 对 HEIC 只会给出 `major_brand`/tile 信息）。改用：
   - **在文件字节里扫描签名 `"Apple iOS\0"`** 定位 MakerNote；
   - MakerNote 布局（已由多个独立实现确认）：
     ```
     0    "Apple iOS\0"        (10 字节)
     10   version              (2 字节)
     12   byte order "MM"/"II" (2 字节)
     14   entry count (u16)，随后每条 12 字节
     每条: tag(u16) type(u16) count(u32) value/offset(u32)
     需要 tag == 0x0011、type == 2(ASCII)；count>4 时 offset 相对 **MakerNote 起点**。
     ```
     该布局是 **TIFF 风格 IFD**，字节序由 `MM`/`II` 决定。
   > 说明：扫描签名是**启发式**（不做完整 HEIF 容器解析）。签名 `"Apple iOS\0"` 足够独特，风险低；若误伤，会在 Task 0 实测暴露。
4. **ffprobe 与 ffmpeg 同目录**：都是 gyan 构建，取 `ffmpeg::find_ffmpeg()` 的父目录下的 `ffprobe.exe`。
5. **integrity 语义（设计 §10.1）**：0 正常 / 1 UUID 不一致 / 2 残缺实况（有 UUID 缺伴生）/ 3 仅静态（无 UUID）/ 4 仅视频 / 5 疑似非原件（体积偏小）。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. ffprobe 对本机 iPhone MOV 是否真的打印出该 tag（Task 0）。
2. HEIC 里 `"Apple iOS\0"` 签名扫描能否稳定命中（Task 0 用真机 HEIC 实测）。
3. **「体积偏小」的判定阈值**：没有绝对标准。本计划**默认 200 KB**（iPhone HEIC 通常 1~3 MB），作为常量并写注释；属新增取舍，**需过审确认**。
4. HEIC 里 EXIF/MakerNote 是否可能被压缩（压缩的 EXIF 需先解压）——实测若命中失败再处理，不要预防性写复杂逻辑。

---

## 文件结构

```
src-tauri/src/
├── ffmpeg.rs       修改：ffprobe 定位 + movie_content_id_args()
├── livephoto.rs    新增：纯逻辑——静态图 MakerNote 解析、视频 tag 解析结果清洗、integrity 判定
├── importer.rs     修改：传输后读标识、判定 integrity、写 content_id/integrity
├── db.rs           修改：回填查询（缺 content_id 的条目）、更新 content_id/integrity
├── commands.rs     修改：classify_library（回填校验）、export_diagnostics
└── lib.rs          修改：注册命令
```

---

## Task 0: 用真机素材确认两条读取路径

**Files:** 无（一次性验证）；把结论记到 `docs/superpowers/notes/2026-09-18-content-id-probe.md`

- [ ] **Step 1: 下载一对真实实况素材**

用既有忽略测试：
```powershell
$env:LIVEPORTER_FFMPEG = "<ffmpeg.exe>"
$env:LPM_SMOKE_N = "2"
cargo test -p liveporter real_device_transfer_smoke -- --ignored --nocapture
```
素材落到 `%TEMP%\lpm_device_smoke\`（形如 `0000_IMG_xxxx.MOV` / `0001_IMG_xxxx.HEIC`）。

- [ ] **Step 2: ffprobe 验证视频 tag**

```powershell
$ffprobe = Join-Path (Split-Path "<ffmpeg.exe>") "ffprobe.exe"
& $ffprobe -v error -show_entries format_tags=com.apple.quicktime.content.identifier -of default=nw=1:nk=1 "$env:TEMP\lpm_device_smoke\0000_IMG_xxxx.MOV"
```
Expected: 打印一个 UUID。**把结果记下来。**

- [ ] **Step 3: 手工验证 HEIC 签名与 MakerNote**

用一段 PowerShell 读字节，找 `"Apple iOS"` 出现的位置与之后的 IFD 条目：
```powershell
$bytes = [IO.File]::ReadAllBytes("$env:TEMP\lpm_device_smoke\0001_IMG_xxxx.HEIC")
$sig = [Text.Encoding]::ASCII.GetBytes("Apple iOS")
# 简单子串查找（列出所有命中位置）
...
```
Expected: 至少一处命中，且其后能解析出 `tag=0x0011` 的 ASCII UUID。

- [ ] **Step 4: 记录结论**

把「ffprobe 是否可用」「HEIC 签名是否命中」「两个 UUID 是否相同」写入
`docs/superpowers/notes/2026-09-18-content-id-probe.md`。若任一路径失败，**停下来报告**，不要硬写。

- [ ] **Step 5: 提交**

```bash
git add docs/superpowers/notes/2026-09-18-content-id-probe.md
git commit -m "docs: probe content identifier extraction on a real iphone"
```

---

## Task 1: `livephoto.rs`——标识解析与完整性判定（纯逻辑）

**Files:** Create `src-tauri/src/livephoto.rs`；Modify `src-tauri/src/lib.rs`（加 `mod livephoto;`）

- [ ] **Step 1: 写失败的测试**

```rust
//! 实况照片的配对标识（ContentIdentifier）读取与完整性判定。

pub const INTEGRITY_OK: i64 = 0;
pub const INTEGRITY_MISMATCH: i64 = 1;
pub const INTEGRITY_PARTIAL: i64 = 2;
pub const INTEGRITY_STILL_ONLY: i64 = 3;
pub const INTEGRITY_VIDEO_ONLY: i64 = 4;
pub const INTEGRITY_SUSPECT: i64 = 5;

/// 「体积明显偏小」的阈值（字节）。iPhone HEIC 通常 1~3 MB；200 KB 以下视为可疑。
pub const SUSPECT_STILL_MAX: u64 = 200 * 1024;

pub const APPLE_MAKERNOTE_SIGNATURE: &[u8] = b"Apple iOS\0";
const CONTENT_IDENTIFIER_TAG: u16 = 0x0011;

/// 从静态图字节里找 Apple MakerNote 并取出 ContentIdentifier。
/// 做法：扫描签名 `"Apple iOS\0"`，再按 TIFF 风格 IFD 解析 tag 0x0011。
pub fn content_id_from_still(bytes: &[u8]) -> Option<String> {
    let mut start = 0usize;
    while let Some(pos) = find_subslice(&bytes[start..], APPLE_MAKERNOTE_SIGNATURE) {
        let at = start + pos;
        if let Some(id) = parse_apple_makernote(&bytes[at..]) {
            return Some(id);
        }
        start = at + 1;
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 解析一个以 `"Apple iOS\0"` 开头的 MakerNote blob。
fn parse_apple_makernote(blob: &[u8]) -> Option<String> {
    if blob.len() < 16 || !blob.starts_with(APPLE_MAKERNOTE_SIGNATURE) {
        return None;
    }
    let big_endian = match &blob[12..14] {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |p: usize| -> Option<u16> {
        let b: [u8; 2] = blob.get(p..p + 2)?.try_into().ok()?;
        Some(if big_endian { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) })
    };
    let u32_at = |p: usize| -> Option<u32> {
        let b: [u8; 4] = blob.get(p..p + 4)?.try_into().ok()?;
        Some(if big_endian { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    };

    let entries = u16_at(14)? as usize;
    for i in 0..entries {
        let entry = 16 + i * 12;
        if u16_at(entry)? != CONTENT_IDENTIFIER_TAG {
            continue;
        }
        if u16_at(entry + 2)? != 2 {
            return None; // 非 ASCII
        }
        let len = u32_at(entry + 4)? as usize;
        let bytes = if len <= 4 {
            blob.get(entry + 8..entry + 8 + len)?
        } else {
            let off = u32_at(entry + 8)? as usize;
            blob.get(off..off.checked_add(len)?)?
        };
        return clean_id(bytes);
    }
    None
}

/// 清洗 ffprobe/字节里取到的标识：去 NUL 与空白，拒绝空与过长。
pub fn clean_id(raw: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(raw);
    let s = s.trim_end_matches('\0').trim();
    if s.is_empty() || s.len() > 128 {
        return None;
    }
    Some(s.to_string())
}

/// 判定一个条目的 integrity。
///
/// - `still_id` / `movie_id`：两侧读到的标识（读不到为 None）
/// - `has_still` / `has_movie`：是否两个文件都存在
/// - `still_size`：静态图字节数（用于体积合理性，仅在缺标识时兜底）
pub fn classify(
    has_still: bool,
    has_movie: bool,
    still_id: Option<&str>,
    movie_id: Option<&str>,
    still_size: u64,
) -> i64 {
    match (has_still, has_movie) {
        (true, true) => match (still_id, movie_id) {
            (Some(a), Some(b)) if a == b => INTEGRITY_OK,
            (Some(_), Some(_)) => INTEGRITY_MISMATCH,
            // 一方缺标识：无法确认配对，但两文件都在 → 按"残缺实况"处理更保守
            _ => INTEGRITY_PARTIAL,
        },
        (true, false) => {
            if still_id.is_some() {
                INTEGRITY_PARTIAL // 有标识但缺视频 → 残缺实况
            } else {
                INTEGRITY_STILL_ONLY // 普通照片
            }
        }
        (false, true) => INTEGRITY_VIDEO_ONLY,
        (false, false) => INTEGRITY_PARTIAL,
    }
    .max(if has_still && still_id.is_none() && still_size < SUSPECT_STILL_MAX {
        // 无标识且体积明显偏小：可能是 iCloud 占位副本
        INTEGRITY_SUSPECT
    } else {
        -1
    })
}
```

测试（自造 MakerNote blob）：
```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个 Apple MakerNote：一条 tag 0x0011 的 ASCII 值。
    fn makernote(id: &str, big_endian: bool) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(APPLE_MAKERNOTE_SIGNATURE);
        b.extend_from_slice(&[0, 1]);
        b.extend_from_slice(if big_endian { b"MM" } else { b"II" });
        let push16 = |v: &mut Vec<u8>, x: u16| {
            v.extend_from_slice(&if big_endian { x.to_be_bytes() } else { x.to_le_bytes() })
        };
        let push32 = |v: &mut Vec<u8>, x: u32| {
            v.extend_from_slice(&if big_endian { x.to_be_bytes() } else { x.to_le_bytes() })
        };
        push16(&mut b, 1); // 1 entry
        push16(&mut b, CONTENT_IDENTIFIER_TAG);
        push16(&mut b, 2); // ASCII
        push32(&mut b, (id.len() + 1) as u32);
        // 值放在目录之后：16 + 12 = 28 起
        push32(&mut b, 28);
        b.extend_from_slice(id.as_bytes());
        b.push(0);
        b
    }

    #[test]
    fn finds_identifier_after_signature() {
        let mut file = vec![0u8; 100];
        file.extend_from_slice(&makernote("F0652AEA-5229-4BF7-A366-B4C79E90CA1C", true));
        file.extend_from_slice(&[0u8; 100]);
        assert_eq!(
            content_id_from_still(&file).as_deref(),
            Some("F0652AEA-5229-4BF7-A366-B4C79E90CA1C")
        );
    }

    #[test]
    fn handles_little_endian() {
        let blob = makernote("ABC-123", false);
        assert_eq!(parse_apple_makernote(&blob).as_deref(), Some("ABC-123"));
    }

    #[test]
    fn missing_signature_returns_none() {
        assert!(content_id_from_still(b"not an apple file").is_none());
    }

    #[test]
    fn classify_rules() {
        assert_eq!(classify(true, true, Some("X"), Some("X"), 1_000_000), INTEGRITY_OK);
        assert_eq!(classify(true, true, Some("X"), Some("Y"), 1_000_000), INTEGRITY_MISMATCH);
        assert_eq!(classify(true, false, Some("X"), None, 1_000_000), INTEGRITY_PARTIAL);
        assert_eq!(classify(true, false, None, None, 1_000_000), INTEGRITY_STILL_ONLY);
        assert_eq!(classify(false, true, None, Some("Y"), 0), INTEGRITY_VIDEO_ONLY);
        // 无标识 + 体积偏小 → 疑似非原件（优先级最高）
        assert_eq!(classify(true, false, None, None, 1000), INTEGRITY_SUSPECT);
    }
}
```

- [ ] **Step 2: 跑测试**

Run: `cargo test -p liveporter livephoto`
Expected: 通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/livephoto.rs src-tauri/src/lib.rs
git commit -m "feat(app): content identifier parsing and integrity classification"
```

---

## Task 2: ffprobe 读取视频标识

**Files:** Modify `src-tauri/src/ffmpeg.rs`

- [ ] **Step 1: 写失败的测试**

```rust
/// 与 ffmpeg 同目录的 ffprobe。
pub fn find_ffprobe() -> anyhow::Result<PathBuf> {
    let ff = find_ffmpeg()?;
    let probe = ff.with_file_name("ffprobe.exe");
    if probe.is_file() {
        return Ok(probe);
    }
    anyhow::bail!("未找到 ffprobe.exe（应与 ffmpeg.exe 同目录）")
}

/// 读取视频 ContentIdentifier 的 ffprobe 参数。
pub fn movie_content_id_args(input: &Path) -> Vec<String> {
    vec![
        "-v".into(), "error".into(),
        "-show_entries".into(),
        "format_tags=com.apple.quicktime.content.identifier".into(),
        "-of".into(), "default=nw=1:nk=1".into(),
        input.to_string_lossy().into_owned(),
    ]
}

/// 运行 ffprobe 并返回 stdout 文本。
pub fn run_capture(probe: &Path, args: &[String]) -> anyhow::Result<String> {
    let out = std::process::Command::new(probe).args(args).output()?;
    if !out.status.success() {
        anyhow::bail!("ffprobe 失败 ({}): {}", out.status, String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[test]
fn movie_id_args_request_only_the_key() {
    let a = movie_content_id_args(Path::new("in.mov"));
    assert!(a.iter().any(|x| x.contains("com.apple.quicktime.content.identifier")));
    assert_eq!(a.last().unwrap(), "in.mov");
}
```

- [ ] **Step 2: 跑测试**

Run: `cargo test -p liveporter ffmpeg`
Expected: 通过。

- [ ] **Step 3: 真 ffprobe 冒烟（ignored）**

```rust
#[test]
#[ignore = "requires ffmpeg/ffprobe"]
fn real_movie_content_id_smoke() {
    let probe = find_ffprobe().unwrap();
    let input = std::env::var("LPM_MOVIE_ID_INPUT").expect("设 LPM_MOVIE_ID_INPUT");
    let text = run_capture(&probe, &movie_content_id_args(Path::new(&input))).unwrap();
    println!("content id = {:?}", text.trim());
    assert!(!text.trim().is_empty(), "ffprobe 未输出 content identifier");
}
```
用 Task 0 下载的真实 MOV 跑一次。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/ffmpeg.rs
git commit -m "feat(app): read movie content identifier via ffprobe"
```

---

## Task 3: 接入导入管道

**Files:** Modify `src-tauri/src/importer.rs`、`src-tauri/src/db.rs`

- [ ] **Step 1: 传输成功后读标识并判定 integrity**

在 `run_tasks` 里，传输成功、写 thumb 之前/之后，加：
```rust
// 读两侧 ContentIdentifier（静态图扫签名，视频问 ffprobe），判定 integrity。
let still_id = asset.still.as_ref().and_then(|f| {
    std::fs::read(&f.path).ok().and_then(|b| crate::livephoto::content_id_from_still(&b))
});
let movie_id = match (&asset.movie, &ffprobe_bin) {
    (Some(f), Some(probe)) => crate::ffmpeg::run_capture(probe, &crate::ffmpeg::movie_content_id_args(Path::new(&f.path)))
        .ok()
        .and_then(|s| crate::livephoto::clean_id(s.trim().as_bytes())),
    _ => None,
};
let integrity = crate::livephoto::classify(
    asset.still.is_some(),
    asset.movie.is_some(),
    still_id.as_deref(),
    movie_id.as_deref(),
    asset.still.as_ref().map(|f| f.size).unwrap_or(0),
);
crate::db::set_content_id(conn, device_id, &task.base_name, task.taken_at, still_id.as_deref().or(movie_id.as_deref()))?;
crate::db::set_integrity(conn, device_id, &task.base_name, task.taken_at, integrity)?;
```
> `run_tasks` 需要新增参数 `ffprobe_bin: Option<&Path>`（测试传 None）。签名会再多一个参数——`#[allow(clippy::too_many_arguments)]` 已在。

- [ ] **Step 2: `db.rs` 加两个 setter**

```rust
pub fn set_content_id(conn: &Connection, device_id: i64, base_name: &str, taken_at: i64, content_id: Option<&str>) -> rusqlite::Result<()> { ... }
pub fn set_integrity(conn: &Connection, device_id: i64, base_name: &str, taken_at: i64, integrity: i64) -> rusqlite::Result<()> { ... }
```

- [ ] **Step 3: 更新既有测试**

既有 `run_tasks` 测试传 `None` 作为 ffprobe；断言可不变（它们用假 Thumbs、无真实文件）。新增一个测试：
构造两个真实临时文件（内容为合成 MakerNote blob 的 HEIC、和一个假 MOV），用假 `Transfer` 写入，断言 integrity 写入 DB。**若嫌重，至少断言 `classify` 被调用路径不 panic**。

- [ ] **Step 4: 跑测试**

Run: `cargo test -p liveporter`
Expected: 通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/importer.rs src-tauri/src/db.rs
git commit -m "feat(app): classify integrity during import"
```

---

## Task 4: 回填命令 `classify_library` + iCloud 手动开关

**Files:** Modify `src-tauri/src/db.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`、`src/App.vue`

- [ ] **Step 1: 回填**

```rust
#[tauri::command]
pub async fn classify_library(state: tauri::State<'_, AppState>) -> Result<ClassifySummary, String> { ... }
```
逻辑：列出所有条目 → 对每个读两侧标识 → `classify` → 写 content_id/integrity。进度事件 `classify://progress`。可复用 §Task 3 的读标识代码（抽成 `livephoto`/`importer` 的公共函数，避免重复）。

- [ ] **Step 2: iCloud 手动开关（设计 §6.4 v1）**

`import_from_device` 增加参数 `assume_cloud: bool`：为真时，导入后把**无标识且体积 < `SUSPECT_STILL_MAX`** 的条目判为 `integrity=5`（这已由 `classify` 覆盖）。前端导入按钮旁加一个复选框「本次的原件可能不在手机上」。**不自动阻止导入**（v1 只标记）。

- [ ] **Step 3: 前端**：加「校验标识」按钮（调 `classify_library`）+ 统计里显示各类 `integrity` 计数。

- [ ] **Step 4: 类型检查与构建**

Run: `npm run build`

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/db.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/App.vue
git commit -m "feat(app): integrity backfill command and cloud hint toggle"
```

---

## Task 5: 诊断报告导出（设计 §10.2）

**Files:** Modify `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`；Create `src-tauri/src/diagnostics.rs`

**约束（设计 §10.2）：** **绝不包含照片内容或文件名以外的元数据。** 设备序列号打码（复用 `model::mask_serial`）。

- [ ] **Step 1: 组装报告文本（纯函数 + 单测）**

```rust
pub struct DiagnosticsInput {
    pub device_model: Option<String>,
    pub serial_masked: Option<String>,
    pub files_total: usize,
    pub integrity_counts: Vec<(i64, usize)>,
    pub failed_errors: Vec<String>,
    pub windows_version: String,
    pub ffmpeg_version: String,
}

pub fn render(input: &DiagnosticsInput) -> String { /* 纯文本 */ }
```
单测：断言输出含设备型号、打码序列号；**断言不含** `still_path`/文件名之类的字段（用一个明确的“不得出现”清单核对）。

- [ ] **Step 2: 命令**

```rust
#[tauri::command]
pub async fn export_diagnostics(state, ffmpeg_bin_hint: ...) -> Result<String, String>
```
收集：设备信息（从 `device` 表）、integrity 计数（`db` 聚合）、失败条目 `error`（截断）、
Windows 版本（`cmd /c ver` 或 `std::env::consts::OS`+注册表）、ffmpeg 版本（`ffmpeg -version` 首行）。
把文本写到用户选择的位置（前端用 dialog 的 save？**本计划先用固定文件**：写到库根 `.lpm/diagnostics-<时间>.txt`，返回路径）。

- [ ] **Step 3: 前端加「导出诊断报告」按钮**，弹提示显示生成的文件路径。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/diagnostics.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/App.vue
git commit -m "feat(app): export diagnostics report"
```

---

## Task 6: 实测

**Files:** Create `docs/superpowers/notes/2026-09-18-integrity-smoke.md`

- [ ] **Step 1: 真机实测**

导入一批真实实况照片 → 点「校验标识」：
- `integrity` 应以 **0（一致）** 为主；
- 抽查 `content_id` 非空且两侧一致；
- 纯照片为 3、纯视频为 4；
- 若人为改名/换伴生文件，应得到 1（不一致）或 2（残缺）。

- [ ] **Step 2: 诊断报告**：生成一次，确认内容合规（无文件名/路径以外的隐私）。

- [ ] **Step 3: 记录并提交**

```bash
git add docs/superpowers/notes/2026-09-18-integrity-smoke.md
git commit -m "docs: record integrity and diagnostics smoke test"
```

---

## Task 7: 收尾验证

- [ ] **Step 1:**

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test -p probe -p liveporter
cargo clippy -p probe -p liveporter --all-targets -- -D warnings
cargo fmt --check
```

- [ ] **Step 2:** `npx vue-tsc --noEmit; npm run build`

- [ ] **Step 3:** 更新 AGENTS.md；若「200 KB 阈值」确认，写入设计 §6.4/§10。

- [ ] **Step 4:** 提交。

---

## 完成判据

- [ ] 静态图标识解析（签名扫描 + MakerNote IFD）有单测，含大小端
- [ ] ffprobe 能读出视频标识（真机实测）
- [ ] `classify` 覆盖 0/1/2/3/4/5 六种情况（有单测）
- [ ] 导入后写入 `content_id` 与 `integrity`
- [ ] 「校验标识」回填命令可对已有库重算
- [ ] iCloud 手动开关（v1：只标记不阻止）
- [ ] 诊断报告可导出且**不含隐私**（有单测核对“不得出现”清单）
- [ ] `cargo test / clippy / fmt`、`vue-tsc`、`npm run build` 全绿
- [ ] **未做** 浏览体验与 UI 的筛选/时间线（计划 8）

## 明确不在本计划内

- 时间线分组、搜索筛选、多选导出、删除 → 计划 8
- iCloud **自动**检测（设计 §6.4 第二步）→ 待检测手段实测后再做
- ffmpeg 裁剪与打包 → 计划 9

---

## 已知风险与取舍（需过审确认）

1. **静态图用「签名扫描」而非完整 HEIF 解析**：实现简单、风险低（签名独特），但不是标准解析；若实测失败再考虑完整 `iinf/iloc` 解析。
2. **体积阈值 200 KB**：新增取舍；iPhone HEIC 通常 1~3 MB。**确认后写入设计 §6.4**。
3. **`classify` 对「两文件都在但缺标识」判为 `2 残缺实况`**：偏保守。若产品上想区分“缺标识但配对存在”，需要新的 integrity 值（设计只有 0~5）。**需过审确认是否接受。**
4. **诊断报告先写固定路径**（库根 `.lpm/`），不做保存对话框；够用即可。
5. **ffprobe 每次调用是子进程**：整库回填会很慢（每个视频一次）。先接受，实测后再考虑只对配对条目调用或缓存。

## 自查记录

**规格覆盖：** 设计 §13 第 6 步、§10（异常分类与诊断报告）、§6.4 v1、§2.1/§5.2（content_id 字段）。

**对既有实现的承接：** 会修改 `run_tasks` 签名（加 ffprobe 参数）与 `db`；不改动 `integrity` 既有取值语义，只从“只有 0/3/4”扩到全部六种。

**已知不确定点（实现时必须查证）：** 见文首「待实测 1~4」。
