# 阶段 5：预览片 + 悬停播放 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 鼠标悬停在实况照片/视频的格子上 **≥350ms** 时，就地播放一段 480p H.264 预览片（按需生成、落盘缓存）。整个应用**只有一个 `<video>` 元素**，悬停时移过去换 `src` 播放，离开即暂停。这是设计 §7.1/§7.2/§7.3 与 §8.1 决定 1 的落地。

**Architecture:** 后端按需生成：`ensure_preview(asset_id)` 若 `previews/<hash>.mp4` 不存在则调 ffmpeg 生成，返回路径；已存在直接返回。前端只负责「何时请求」——防抖、滚动/缩放期间与结束后 300ms 内不请求、同时最多 2 个在飞。**图片/视频一律走 `lpm://`**，不走 IPC。

**Tech Stack:** Rust 2021 / ffmpeg（已有）/ Tauri 2（已有）/ Vue 3 单例 `<video>`

**为什么这样切（设计 §7.2）：** 5 万张全量预生成约 2~3 GB，不可接受。悬停触发 + 落盘缓存把成本压到「用户真正看过的那些」。

---

## 关键事实与设计约束

1. **全局只有一个 `<video>`**（设计 §8.1 决定 1）。一万个格子放一万个 `<video>` 会让 WebView2 崩。悬停时把唯一那个 `<video>` 移到目标格子上、换 `src`、播放；离开暂停。**不要**在 `PhotoGrid` 里给每个瓦片放 `<video>`。
2. **防抖与抑制（设计 §7.3）**：
   - 悬停停留 ≥ **350ms** 才请求预览；
   - **滚动中不请求**，滚动停止后 **300ms 内也不请求**；
   - **缩放中与缩放结束后 300ms 内不请求**；
   - 同时最多 **2 个**预览生成任务。
3. **预览片规格（设计 §7.1）**：480p、H.264/MP4、约 100~400 KB、**无音轨**。
4. **仅对含视频的条目生成**：`kind=3`（实况）或 `kind=2`（纯视频）。纯静态照片悬停不播放（保持缩略图）。
5. **`-filter_complex` 优先**：iPhone 的 HEIC 是多流，`-vf` 会冲突（阶段 4 教训）；视频虽多为单流，仍统一用 `-filter_complex` 保持一致。
6. **ffmpeg 定位**沿用 `ffmpeg::find_ffmpeg()`（阶段 4）。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. `libx264` 在本机 ffmpeg（gyan essentials 9.0.1）可用——阶段 4 生成过 H.264 `.MOV`，应可用；仍以 `-encoders` 确认。
2. 长视频的**时长上限**：设计只说「约 100~400 KB」。本计划**默认截取前 6 秒**（`-t 6`）以免长视频生成巨大预览。**这是本计划新增的取舍，需用户确认**；若确认，写入设计 §7.1。
3. `-movflags +faststart` 对 mp4 输出配合 `.part` 临时名的行为——以能快速起播为准。
4. WebView2 播放 H.264 MP4 的可用性（本项目它一定支持；H.264 是唯一有把握的编码）。

---

## 文件结构

```
src-tauri/src/
├── ffmpeg.rs            修改：加 preview_args()
├── thumb.rs → media.rs  重命名或新增 preview 生成（见 Task 1）
├── db.rs                修改：set_preview_path / asset_source(id) 查询
├── commands.rs          修改：ensure_preview(asset_id)
└── lib.rs               修改：注册命令

src/
├── components/PhotoGrid.vue   修改：瓦片 hover 回调、暴露瓦片矩形
├── composables/usePreview.ts  新增：单例 <video>、防抖与抑制、最多 2 在飞
└── App.vue                    修改：挂载单例 <video>
```

**边界说明：** 防抖/抑制/并发限制都在**前端**（前端才知道用户是否在滚动/缩放）。后端只做「有则返回、无则生成」。这是对设计 §7.3「悬停池固定 2」的一种实现方式——**由前端限流到 2**，后端不再维护工作池。**此取舍需在计划过审时确认。**

---

## Task 0: 预览片 ffmpeg 参数 + 冒烟验证

**Files:** Modify `src-tauri/src/ffmpeg.rs`

- [ ] **Step 1: 写失败的测试**

```rust
/// 从视频生成 480p H.264 预览片的参数（不含程序名）。
/// 无音轨、限制时长、faststart 便于快速起播。
pub fn preview_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(), "error".into(),
        "-y".into(),
        "-i".into(), input.to_string_lossy().into_owned(),
        "-t".into(), "6".into(),                 // 见「待实测 2」：默认截前 6 秒
        "-filter_complex".into(), "scale=480:-2".into(),
        "-an".into(),
        "-c:v".into(), "libx264".into(),
        "-crf".into(), "28".into(),
        "-preset".into(), "veryfast".into(),
        "-movflags".into(), "+faststart".into(),
        "-f".into(), "mp4".into(),
        output.to_string_lossy().into_owned(),
    ]
}
```

测试：
```rust
#[test]
fn preview_args_are_480p_h264_muted() {
    let a = preview_args(Path::new("in.mov"), Path::new("out.mp4.part"));
    assert!(a.contains(&"scale=480:-2".to_string()));
    assert!(a.contains(&"libx264".to_string()));
    assert!(a.contains(&"-an".to_string()));
    assert_eq!(a.last().unwrap(), "out.mp4.part");
}
```

- [ ] **Step 2: 跑测试**

Run: `cargo test -p liveporter ffmpeg`
Expected: 通过。

- [ ] **Step 3: 真 ffmpeg 冒烟（ignored）**

临时用 `LPM_THUMB_INPUT` 指向一个 MOV：
```powershell
$env:LIVEPORTER_FFMPEG = "<你机器上 ffmpeg.exe>"
$env:LPM_PREVIEW_INPUT = "<某个 .MOV 或 .MP4>"
cargo test -p liveporter real_preview_smoke -- --ignored --nocapture
```
其中 `real_preview_smoke`（Task 1 里加）会打印生成的 mp4 大小与是否 > 0。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/ffmpeg.rs
git commit -m "feat(app): add 480p h264 preview ffmpeg args"
```

---

## Task 1: 预览片生成（与缩略图同模块）

**Files:** Modify `src-tauri/src/thumb.rs`（保留文件名，职责扩展为「派生媒体」；或按团队偏好改名 `media.rs`，二选一并说明）

- [ ] **Step 1: 写失败的测试（哈希复用 + 命名）**

```rust
/// 预览片文件名：`<hash>.mp4`。
pub fn preview_file_name(hash: &str) -> String {
    format!("{hash}.mp4")
}

/// 为视频生成 480p 预览片，返回路径。
pub fn make_preview_for_movie(
    ffmpeg_bin: &Path,
    input: &Path,
    previews_dir: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(previews_dir)?;
    let out = previews_dir.join(preview_file_name(&content_hash(input)?));
    if out.is_file() {
        return Ok(out);
    }
    let mut tmp = out.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);

    if let Err(e) = ffmpeg::run(ffmpeg_bin, &ffmpeg::preview_args(input, &tmp)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &out)?;
    Ok(out)
}

#[test]
fn preview_name_has_mp4_extension() {
    assert_eq!(preview_file_name("abc"), "abc.mp4");
}
```

- [ ] **Step 2: 跑测试**

Run: `cargo test -p liveporter preview`
Expected: 通过。

- [ ] **Step 3: 加真 ffmpeg 冒烟（ignored）**

```rust
#[test]
#[ignore = "requires ffmpeg"]
fn real_preview_smoke() {
    let bin = ffmpeg::find_ffmpeg().unwrap();
    let input = std::env::var("LPM_PREVIEW_INPUT").expect("设 LPM_PREVIEW_INPUT");
    let out_dir = std::env::temp_dir().join("lpm_preview_smoke");
    let _ = std::fs::remove_dir_all(&out_dir);
    let p = make_preview_for_movie(&bin, Path::new(&input), &out_dir).unwrap();
    println!("preview = {p:?} ({} bytes)", std::fs::metadata(&p).unwrap().len());
    assert!(std::fs::metadata(&p).unwrap().len() > 0);
}
```

用真实 MOV 跑一次，记录大小（应在几十~几百 KB 量级）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/thumb.rs
git commit -m "feat(app): generate 480p h264 preview clips"
```

---

## Task 2: 数据访问

**Files:** Modify `src-tauri/src/db.rs`

- [ ] **Step 1: 写失败的测试**

```rust
/// 取一个条目的视频源路径（用于生成预览片）。
pub fn asset_movie_path(conn: &Connection, asset_id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT movie_path FROM asset WHERE id = ?1",
        params![asset_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|o| o.flatten())
}

pub fn set_preview_path(
    conn: &Connection,
    asset_id: i64,
    preview_path: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE asset SET preview_path=?1, updated_at=?2 WHERE id=?3",
        params![preview_path, now_epoch(), asset_id],
    )?;
    Ok(())
}
```
> `optional()` 需要 `use rusqlite::OptionalExtension;`。

测试加入既有 `db` 测试模块：插入一条含 `movie_path` 的 asset，断言 `asset_movie_path(id)` 返回该路径、不存在的 id 返回 `None`、`set_preview_path` 后能查到。

- [ ] **Step 2: 跑测试**

Run: `cargo test -p liveporter db`
Expected: 通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/db.rs
git commit -m "feat(app): db accessors for preview generation"
```

---

## Task 3: 命令 `ensure_preview`

**Files:** Modify `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`

- [ ] **Step 1: 实现**

```rust
/// 确保某条目的预览片存在；返回其绝对路径（供前端拼 lpm://）。
/// 无视频源的条目返回明确错误。
#[tauri::command]
pub async fn ensure_preview(
    asset_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;

    // 查询在阻塞线程里做（rusqlite 是阻塞 API）。
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<String> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;

        let movie = crate::db::asset_movie_path(&conn, asset_id)?
            .ok_or_else(|| anyhow::anyhow!("该条目没有视频源，无法生成预览"))?;

        // 已有 preview_path 且文件仍在 → 直接用。
        // （简单起见：先查 DB，再校验文件存在。）

        let bin = crate::ffmpeg::find_ffmpeg()?;
        let p = crate::thumb::make_preview_for_movie(&bin, std::path::Path::new(&movie), &lib.previews_dir())?;
        crate::db::set_preview_path(&conn, asset_id, &p.to_string_lossy())?;
        Ok(p.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}
```
`generate_handler!` 加 `commands::ensure_preview`。

> 并发/去重：同一 hash 的文件已存在时 `make_preview_for_movie` 直接复用，天然去重；前端限流到 2 即可。

- [ ] **Step 2: 编译**

Run: `cargo check -p liveporter`
Expected: 通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(app): ensure_preview command"
```

---

## Task 4: 前端单例 `<video>` + 悬停防抖

**Files:** Create `src/composables/usePreview.ts`；Modify `src/components/PhotoGrid.vue`、`src/App.vue`

- [ ] **Step 1: `usePreview.ts`（单例 video + 防抖 + 抑制 + 限流）**

```ts
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { lpmUrl } from "../lib/lpm";
import type { AssetRow } from "../stores/library";

const HOVER_DELAY = 350;
const SUPPRESS_AFTER = 300; // 滚动/缩放结束后多久内不触发
const MAX_IN_FLIGHT = 2;

export function usePreview() {
  const video = ref<HTMLVideoElement | null>(null);
  const loading = ref(false);
  const visible = ref(false);

  let hoverTimer: number | undefined;
  let suppressUntil = 0;
  let inFlight = 0;
  let currentAssetId: number | null = null;

  /** 滚动/缩放时调用：抑制触发，并取消已排队的悬停。 */
  function suppress() {
    suppressUntil = Date.now() + SUPPRESS_AFTER;
    if (hoverTimer !== undefined) {
      clearTimeout(hoverTimer);
      hoverTimer = undefined;
    }
    hide();
  }

  function hide() {
    visible.value = false;
    const v = video.value;
    if (v) {
      v.pause();
      v.removeAttribute("src");
      v.load();
    }
    currentAssetId = null;
    loading.value = false;
  }

  /** 悬停进入某个格子。asset 为 null 表示纯静态/无视频，不播放。 */
  function enter(asset: AssetRow | null) {
    if (hoverTimer !== undefined) clearTimeout(hoverTimer);
    if (!asset || !asset.movie_path) return;
    if (Date.now() < suppressUntil) return;
    hoverTimer = window.setTimeout(() => void play(asset), HOVER_DELAY);
  }

  async function play(asset: AssetRow) {
    if (currentAssetId === asset.id && visible.value) return;
    if (inFlight >= MAX_IN_FLIGHT) return; // 超限就放弃这次（悬停预览可丢）
    inFlight++;
    loading.value = true;
    try {
      const path = await invoke<string>("ensure_preview", { assetId: asset.id });
      currentAssetId = asset.id;
      loading.value = false;
      visible.value = true;
      const v = video.value;
      if (v) {
        v.src = lpmUrl(path);
        await v.play().catch(() => {});
      }
    } catch {
      loading.value = false;
    } finally {
      inFlight--;
    }
  }

  return { video, loading, visible, enter, suppress, hide };
}
```

- [ ] **Step 2: `PhotoGrid.vue` 暴露瓦片矩形并回调 hover**

给组件加 props `assets: AssetRow[]`（保留 `items` 或改造），并在 `.tile` 上：
```html
<div class="tile" @mouseenter="onEnter(t, $event)" @mouseleave="$emit('hover-out')">
```
```ts
function onEnter(t: Tile, e: MouseEvent) {
  const el = e.currentTarget as HTMLElement | null;
  if (!el) return;
  const r = el.getBoundingClientRect();
  emit("hover", { asset: props.assets[t.index], rect: { x: r.left, y: r.top, w: r.width, h: r.height } });
}
```
（`assets[t.index]` 用于拿到 `id`/`movie_path`；`rect` 用来给单例 `<video>` 定位。）

- [ ] **Step 3: `App.vue` 挂单例 `<video>` 并接线**

模板里在 `main` 内放**唯一一个**：
```html
<video ref="pv.video" class="preview" :class="{ show: pv.visible }" muted playsinline loop />
```
样式：绝对定位、`pointer-events:none`、`object-fit:cover`，位置由 hover 事件里的 `rect` 换算到 `main` 坐标系（减去 `main` 的 `getBoundingClientRect()`）。加载中在瓦片上显示一个淡进度点（简单起见先用 `<video>` 的 `waiting`/`canplay` 事件切换 `pv.loading`）。

`enter`/`suppress` 接线：
- `PhotoGrid` 的 `@hover` → `pv.enter(asset)` 并定位 `<video>`；
- `@hover-out` → `pv.hide()`；
- `onWheel` 与滚动回调里调用 `pv.suppress()`。

- [ ] **Step 4: 类型检查与构建**

Run: `npm run build`
Expected: 通过。

- [ ] **Step 5: 提交**

```bash
git add src/composables/usePreview.ts src/components/PhotoGrid.vue src/App.vue
git commit -m "feat(ui): single video element with hover-triggered preview playback"
```

---

## Task 5: 实测

**Files:** Create `docs/superpowers/notes/2026-09-18-preview-smoke.md`

- [ ] **Step 1: 实测（需先有一个含实况/视频的库）**

1. 打开库；把鼠标停在某个实况照片格子上 ≥350ms；
2. 预期：短暂延迟后该格子内播放动态（或至少 `.MOV` 对应的画面）；
3. 快速划过多个格子：**不应**触发（防抖）；
4. 滚动/缩放过程中与之后 300ms 内：**不应**触发；
5. DevTools 里确认**只有一个** `<video>` 元素；
6. `previews/` 下出现 `<hash>.mp4`，大小在几十~几百 KB。

- [ ] **Step 2: 记录**

记录：单个预览生成耗时、文件大小、悬停手感、是否有卡顿。

- [ ] **Step 3: 提交**

```bash
git add docs/superpowers/notes/2026-09-18-preview-smoke.md
git commit -m "docs: record preview hover smoke test"
```

---

## Task 6: 收尾验证

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

- [ ] **Step 3: 更新 AGENTS.md 进度与文档索引；若「前 6 秒」取舍确认，写入设计 §7.1**

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "chore: stage 5 verification and progress update"
```

---

## 完成判据

- [ ] `preview_args` 生成 480p H.264、无音轨、`faststart`（有单测）
- [ ] 预览片按内容哈希命名、已存在则复用（有单测）
- [ ] `ensure_preview` 对无视频源的条目返回明确错误；对含视频的条目生成并写 `preview_path`
- [ ] 全局**只有一个** `<video>`（DevTools 确认）
- [ ] 悬停 ≥350ms 才触发；滚动/缩放中及之后 300ms 不触发；同时最多 2 个在飞
- [ ] 预览片走 `lpm://`，不走 IPC
- [ ] `cargo test / clippy / fmt`、`vue-tsc`、`npm run build` 全绿
- [ ] **未做** UUID 校验/异常分类/诊断/iCloud 检测（计划 7）

## 明确不在本计划内

- ContentIdentifier 校验、integrity 1/2/5、诊断报告、iCloud 检测 → 计划 7
- 时间线分组、搜索筛选、多选导出、删除 → 计划 8
- 大图点开查看（设计 §7.2 第三行）→ 后续
- 视口预取（设计 §7.2 提到的低优先级预取队列）→ 本计划**不做**，先看悬停手感再定
- ffmpeg 裁剪与随包分发 → 计划 9

---

## 已知风险与取舍（需过审确认）

1. **预览片默认截取前 6 秒**（`-t 6`）：设计只给「约 100~400 KB」，未定长视频策略。截前 6 秒是合理默认，但属新增决策——**确认后写入设计 §7.1**。
2. **限流放在前端**（最多 2 个在飞）：与设计 §7.3「悬停池固定 2」等价但实现不同；后端不维护工作池。若更希望后端强制，需要额外的信号量/去重机制。
3. **超限时丢弃悬停请求**：当已有 2 个生成中，新的悬停不再排队。对悬停预览体验可接受（用户很快就移开了）。
4. **不做视口预取**：首次悬停会有生成延迟（几百 ms）。先测量再决定是否加预取。

## 自查记录

**规格覆盖：** 设计 §7.1（预览片规格）、§7.2（按需 + 落盘缓存）、§7.3（防抖与抑制与并发上限）、§8.1 决定 1（全局单 `<video>`）、§8.1 决定 3（走 `lpm://`）。

**类型一致性：** `ensure_preview(assetId)` ↔ 前端 `invoke("ensure_preview", { assetId })`；`AssetRow.id` / `movie_path` 已存在。

**对阶段 4 的承接：** 复用 `ffmpeg::find_ffmpeg`、`thumb::content_hash`、`Library::previews_dir()`。阶段 4 的 `-filter_complex` 教训在此沿用。

**已知不确定点（实现时必须查证）：**

1. `libx264` 在本机 ffmpeg 的可用性（Task 0 Step 3 实测）
2. `-t 6` 与 `filter_complex` 的组合是否正常截断
3. `faststart` + `.part` 临时名的配合
4. WebView2 播放生成的 MP4（预期 OK）
