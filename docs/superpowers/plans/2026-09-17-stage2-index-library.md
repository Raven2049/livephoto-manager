# 阶段 2：SQLite 索引 + 库目录管理 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 LivePorter 拥有一个真正的「库」：用户指定一个硬盘目录，应用在其中建立 `originals/ thumbs/ previews/ .lpm/index.db` 结构；把 `originals/` 下已有的文件扫描、配对、写进 SQLite 索引；UI 从索引读数据而不是每次遍历磁盘。

**Architecture:** 索引层与 Tauri 解耦——纯 Rust 模块（`library` / `db` / `indexer`）只依赖 `rusqlite`，全部可单测；Tauri 只做薄薄一层命令转发。**磁盘上的文件是唯一真相，SQLite 只是加速层**（设计 §5.4）：索引可随时删除重建。

**Tech Stack:** Rust 2021 / `rusqlite`（`bundled`）/ `serde` / Tauri 2（已有）

**为什么先做这个（设计 §13 第 3 步）：** 计划 4 的设备导入、计划 5 的缩略图与校验，产物都要落进这套库目录与索引。先把「库目录是什么」「索引长什么样」「重扫如何幂等」定死，后面才有地方放数据。

**本计划明确不碰：** WPD/设备、ffmpeg、ContentIdentifier 校验、前端浏览体验优化。见文末「明确不在本计划内」。

---

## 环境现状（2026-09-17 在本开发机实测）

| 项 | 状态 |
|---|---|
| Rust | 1.98.1，`stable-x86_64-pc-windows-msvc` |
| MSVC | VS BuildTools 2022，`cl.exe` 14.44.35207（`rusqlite` 的 `bundled` 需要它编译 SQLite 的 C 源码） |
| Node | v24.19.0 |
| 已有 | 计划 1 的 `crates/probe`、计划 2 的 `src-tauri` + `src`，均验证通过 |

**核对命令：**

```powershell
rustc --version
Get-ChildItem "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe"
```

---

## 关键事实与约定

1. **设计 §5.2 的表结构是「设计级，非最终 migration」**（原文如此）。本计划在其基础上补两列，理由在 Task 2 说明。
2. **库目录即数据**（设计 §4、附录 A 决策 2）：`originals/` 下按 `<型号>-<序列号后6位>/<年份>/` 组织；`.lpm/`、`thumbs/`、`previews/` 都可随时删除。
3. **`rusqlite` 的版本以 `cargo add` 实际写入为准**，不要手填版本号。`bundled` feature 会把 SQLite 源码一起编译，不依赖系统 sqlite3。
4. **`rusqlite::Connection` 是 `!Sync`**，放进 Tauri 的 `State` 必须包 `Mutex`。
5. **时间统一用 epoch 秒（`i64`）**，不引 `chrono`/`time`。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. `rusqlite` 的 `bundled` 在当前 MSVC 下能否直接编过（预期可以，但首次编译会有几分钟）。
2. `rusqlite` 的 upsert 语法：`INSERT ... ON CONFLICT(...) DO UPDATE SET ...` 由 SQLite 原生支持，`rusqlite::Connection::execute` 直接透传；确认参数绑定 `params![]` 写法以编译为准。
3. `PRAGMA journal_mode=WAL` 返回一个结果行，用 `query_row` 还是 `pragma_update` —— 以编译与运行结果为准。

---

## 文件结构

```
livephoto-manager/
├── src-tauri/src/
│   ├── lib.rs                修改：注册新命令、把 Db 塞进 State
│   ├── state.rs              修改：AppState 持有 library + db
│   ├── library.rs            新增：库目录结构（创建/校验/路径）
│   ├── db.rs                 新增：连接、migration、asset/device 的读写
│   ├── indexer.rs            新增：扫描 originals/ → 配对 → 写索引
│   ├── pairing.rs            新增：从 probe 搬运的配对逻辑（纯函数）
│   └── commands.rs           新增：Tauri 命令薄壳
└── src/
    ├── stores/library.ts     修改：改为从索引命令取数据
    └── App.vue               修改：加「打开库 / 重建索引 / 统计」最小面板
```

**边界说明：**
- `library.rs` / `db.rs` / `indexer.rs` / `pairing.rs` **不含任何 Tauri 类型**，可 `cargo test -p liveporter` 完整单测。
- `commands.rs` 只做参数校验 + 调上面几个模块，不含业务逻辑。
- 复用计划 2 的 `scanner.rs` 里的文件遍历？**不复用**：`indexer` 需要的是「库目录结构感知」的遍历（区分 device/year），与 `scanner` 的「任意目录列图片」不同。`scanner` 保留给「选择本地目录预览」用。

---

## Task 0: 加依赖并确认能编过

**Files:** Modify `src-tauri/Cargo.toml`

- [ ] **Step 1: 加 rusqlite**

Run:
```powershell
cd C:\Users\Raven\livephoto-manager
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo add rusqlite --package liveporter --features bundled
```
把实际写入的版本记在下方：

```
（待填）
```

- [ ] **Step 2: 确认能编译（首次会编译 SQLite C 源码，较慢）**

Run:
```powershell
cargo check -p liveporter
```
Expected: 编译通过。若 MSVC 报错，先回到计划 1 的环境核对。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/Cargo.toml Cargo.lock
git commit -m "chore(app): add rusqlite with bundled sqlite"
```

---

## Task 1: 库目录结构 `library.rs`

**Files:** Create `src-tauri/src/library.rs`；Modify `src-tauri/src/lib.rs`（加 `mod library;`）

- [ ] **Step 1: 写失败的测试**

Create `src-tauri/src/library.rs`：
```rust
use std::path::{Path, PathBuf};

/// 一个库 = 用户指定的一个目录。应用不维护第二份副本。
pub struct Library {
    root: PathBuf,
}

pub const ORIGINALS: &str = "originals";
pub const THUMBS: &str = "thumbs";
pub const PREVIEWS: &str = "previews";
pub const META_DIR: &str = ".lpm";
pub const DB_FILE: &str = "index.db";

impl Library {
    /// 打开（不存在则创建）一个库，并确保目录结构齐全。
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let lib = Library { root: root.into() };
        lib.ensure_structure()?;
        Ok(lib)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn originals_dir(&self) -> PathBuf {
        self.root.join(ORIGINALS)
    }
    pub fn thumbs_dir(&self) -> PathBuf {
        self.root.join(THUMBS)
    }
    pub fn previews_dir(&self) -> PathBuf {
        self.root.join(PREVIEWS)
    }
    pub fn meta_dir(&self) -> PathBuf {
        self.root.join(META_DIR)
    }
    pub fn db_path(&self) -> PathBuf {
        self.meta_dir().join(DB_FILE)
    }

    fn ensure_structure(&self) -> std::io::Result<()> {
        for dir in [
            self.originals_dir(),
            self.thumbs_dir(),
            self.previews_dir(),
            self.meta_dir(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    /// 把库内相对路径还原成绝对路径，并确保结果仍在库内（防目录穿越）。
    pub fn resolve_inside(&self, relative: &str) -> Option<PathBuf> {
        let candidate = self.root.join(relative);
        let canonical_root = std::fs::canonicalize(&self.root).ok()?;
        let canonical = std::fs::canonicalize(&candidate).ok()?;
        canonical.starts_with(&canonical_root).then_some(canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn open_creates_structure() {
        let root = temp_root("lpm_lib_test");
        let lib = Library::open(&root).unwrap();
        assert!(lib.originals_dir().is_dir());
        assert!(lib.thumbs_dir().is_dir());
        assert!(lib.previews_dir().is_dir());
        assert!(lib.meta_dir().is_dir());
        assert_eq!(lib.db_path(), root.join(".lpm").join("index.db"));
    }

    #[test]
    fn resolve_rejects_escape() {
        let root = temp_root("lpm_lib_escape");
        let lib = Library::open(&root).unwrap();
        std::fs::write(root.join("inside.txt"), b"x").unwrap();
        assert!(lib.resolve_inside("inside.txt").is_some());
        assert!(lib.resolve_inside("../outside.txt").is_none());
    }
}
```

- [ ] **Step 2: 运行测试，确认通过**

Run:
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test -p liveporter library
```
Expected: 2 个测试通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/library.rs src-tauri/src/lib.rs
git commit -m "feat(app): add library directory structure"
```

---

## Task 2: SQLite schema 与迁移 `db.rs`

**Files:** Create `src-tauri/src/db.rs`；Modify `src-tauri/src/lib.rs`

**相对设计 §5.2 的两处增补（设计原文声明表结构非最终版）：**
- `asset.missing INTEGER NOT NULL DEFAULT 0` —— 设计 §5.4 要求「启动时抽查，文件不在则标记失效条目」，需要一个字段承载，原表没有。
- 用 `PRAGMA user_version` 记 schema 版本，避免引额外迁移框架。

- [ ] **Step 1: 写失败的测试**

Create `src-tauri/src/db.rs`：
```rust
use rusqlite::{params, Connection};

pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS device (
  id            INTEGER PRIMARY KEY,
  serial        TEXT NOT NULL UNIQUE,
  model         TEXT NOT NULL,
  display_name  TEXT,
  folder_name   TEXT NOT NULL,
  last_seen_at  INTEGER
);

CREATE TABLE IF NOT EXISTS asset (
  id            INTEGER PRIMARY KEY,
  device_id     INTEGER NOT NULL REFERENCES device(id),

  kind          INTEGER NOT NULL,
  base_name     TEXT NOT NULL,
  taken_at      INTEGER NOT NULL,

  still_path    TEXT,
  still_ext     TEXT,
  still_size    INTEGER,

  movie_path    TEXT,
  movie_ext     TEXT,
  movie_size    INTEGER,

  content_id    TEXT,
  integrity     INTEGER NOT NULL DEFAULT 0,

  status        INTEGER NOT NULL DEFAULT 0,
  error         TEXT,

  thumb_path    TEXT,
  preview_path  TEXT,

  missing       INTEGER NOT NULL DEFAULT 0,

  created_at    INTEGER,
  updated_at    INTEGER,

  UNIQUE(device_id, base_name, taken_at)
);

CREATE INDEX IF NOT EXISTS idx_asset_taken_at ON asset(taken_at DESC);
CREATE INDEX IF NOT EXISTS idx_asset_kind ON asset(kind);
CREATE INDEX IF NOT EXISTS idx_asset_status ON asset(status);
CREATE INDEX IF NOT EXISTS idx_asset_integrity ON asset(integrity);
"#;

/// 打开（必要时创建）索引库。WAL 模式让读写并发更友好。
pub fn open(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrate(&conn)?;
    Ok(conn)
}

/// 仅测试用：内存库。
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current < SCHEMA_VERSION {
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_tables_and_sets_version() {
        let conn = open_in_memory().unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);

        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('device','asset')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
    }

    #[test]
    fn asset_unique_key_rejects_true_duplicate() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO device (serial, model, folder_name) VALUES (?1, ?2, ?3)",
            params!["SN1", "iPhone15Pro", "iPhone15Pro-3F9A2C"],
        )
        .unwrap();

        let insert = |conn: &Connection| {
            conn.execute(
                "INSERT INTO asset (device_id, kind, base_name, taken_at, still_path)
                 VALUES (1, 3, 'IMG_0001', 1700000000, 'originals/x/2024/IMG_0001.HEIC')",
                [],
            )
        };
        insert(&conn).unwrap();
        assert!(insert(&conn).is_err(), "同一 (device,base,taken_at) 应被唯一约束挡下");
    }
}
```

在 `lib.rs` 顶部加 `mod db;`。

- [ ] **Step 2: 运行测试，确认通过**

Run:
```powershell
cargo test -p liveporter db
```
Expected: 3 个测试通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/db.rs src-tauri/src/lib.rs
git commit -m "feat(app): add sqlite schema and migration"
```

---

## Task 3: 配对与完整性分类 `pairing.rs`

**Files:** Create `src-tauri/src/pairing.rs`；Modify `src-tauri/src/lib.rs`

**为什么要重复一份：** 计划 1 的配对逻辑在独立 crate `crates/probe` 里，那是**一次性探测程序**，不该被正式应用依赖。这里按应用的数据模型重写一份，并加上 integrity 分类。

**关于 integrity 的诚实说明：** 本计划**读不到** `ContentIdentifier`（需要解析 HEIC/QuickTime，属计划 5）。所以：
- 静态图 + 视频同名 → `integrity = 0`，但**是"未经 UUID 校验"的临时值**，`content_id` 留 NULL；计划 5 补齐后可能降级为 1/2。
- 只有静态图 → `3`（仅静态）
- 只有视频 → `4`（仅视频）
- 计划 5 才可能产生 `1`（UUID 不一致）、`2`（残缺实况）、`5`（疑似非原件）。

- [ ] **Step 1: 写失败的测试**

Create `src-tauri/src/pairing.rs`：
```rust
use std::collections::BTreeMap;

pub const KIND_PHOTO: i64 = 1;
pub const KIND_VIDEO: i64 = 2;
pub const KIND_LIVE: i64 = 3;

pub const INTEGRITY_OK: i64 = 0;
pub const INTEGRITY_STILL_ONLY: i64 = 3;
pub const INTEGRITY_VIDEO_ONLY: i64 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRef {
    pub path: String,
    pub ext: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedAsset {
    pub base_name: String,
    pub kind: i64,
    pub integrity: i64,
    pub still: Option<FileRef>,
    pub movie: Option<FileRef>,
}

fn is_still_ext(ext: &str) -> bool {
    matches!(
        ext,
        "heic" | "heif" | "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif"
    )
}

fn is_movie_ext(ext: &str) -> bool {
    matches!(ext, "mov" | "mp4" | "m4v" | "avi" | "3gp" | "mkv")
}

/// 主名：去掉最后一个扩展名（`.` 开头且无其他点视为无扩展名）。
pub fn base_name(file_name: &str) -> &str {
    match file_name.rfind('.') {
        Some(i) if i > 0 => &file_name[..i],
        _ => file_name,
    }
}

fn ext_of(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(i) if i > 0 => file_name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// 把「同一目录下的一批文件」按主名配对。输入只含已过滤的媒体文件。
pub fn pair_files(files: &[(String, u64)]) -> Vec<PairedAsset> {
    let mut map: BTreeMap<String, PairedAsset> = BTreeMap::new();

    for (path, size) in files {
        let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let base = base_name(file_name).to_string();
        let ext = ext_of(file_name);
        let entry = map.entry(base.clone()).or_insert_with(|| PairedAsset {
            base_name: base,
            kind: KIND_PHOTO,
            integrity: INTEGRITY_OK,
            still: None,
            movie: None,
        });
        let fr = FileRef {
            path: path.clone(),
            ext: ext.clone(),
            size: *size,
        };
        if is_still_ext(&ext) {
            entry.still = Some(fr);
        } else if is_movie_ext(&ext) {
            entry.movie = Some(fr);
        }
    }

    map.into_values()
        .map(|mut a| {
            a.kind = match (&a.still, &a.movie) {
                (Some(_), Some(_)) => KIND_LIVE,
                (Some(_), None) => KIND_PHOTO,
                (None, Some(_)) => KIND_VIDEO,
                (None, None) => KIND_PHOTO,
            };
            a.integrity = match (&a.still, &a.movie) {
                (Some(_), Some(_)) => INTEGRITY_OK,
                (Some(_), None) => INTEGRITY_STILL_ONLY,
                (None, Some(_)) => INTEGRITY_VIDEO_ONLY,
                (None, None) => INTEGRITY_STILL_ONLY,
            };
            a
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str) -> (String, u64) {
        (format!("originals/dev/2024/{name}"), 10)
    }

    #[test]
    fn pairs_live_photo() {
        let assets = pair_files(&[f("IMG_0001.HEIC"), f("IMG_0001.MOV")]);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].kind, KIND_LIVE);
        assert_eq!(assets[0].integrity, INTEGRITY_OK);
    }

    #[test]
    fn classifies_still_only_and_video_only() {
        let still = pair_files(&[f("IMG_0002.JPG")]);
        assert_eq!(still[0].kind, KIND_PHOTO);
        assert_eq!(still[0].integrity, INTEGRITY_STILL_ONLY);

        let video = pair_files(&[f("IMG_0003.MOV")]);
        assert_eq!(video[0].kind, KIND_VIDEO);
        assert_eq!(video[0].integrity, INTEGRITY_VIDEO_ONLY);
    }

    #[test]
    fn ignores_aae_and_sorts_by_base_name() {
        let assets = pair_files(&[f("IMG_0003.MOV"), f("IMG_0001.HEIC"), f("IMG_0001.AAE")]);
        let names: Vec<_> = assets.iter().map(|a| a.base_name.as_str()).collect();
        assert_eq!(names, vec!["IMG_0001", "IMG_0003"]);
    }

    #[test]
    fn edited_variants_are_separate_assets() {
        // IMG_E0102 与 IMG_0102 是不同主名，不能互相配对
        let assets = pair_files(&[f("IMG_0102.JPG"), f("IMG_E0102.JPG"), f("IMG_E0102.MOV")]);
        assert_eq!(assets.len(), 2);
        let e = assets.iter().find(|a| a.base_name == "IMG_E0102").unwrap();
        assert_eq!(e.kind, KIND_LIVE);
    }
}
```

在 `lib.rs` 顶部加 `mod pairing;`。

- [ ] **Step 2: 运行测试，确认通过**

Run:
```powershell
cargo test -p liveporter pairing
```
Expected: 4 个测试通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/pairing.rs src-tauri/src/lib.rs
git commit -m "feat(app): add pairing and integrity classification"
```

---

## Task 4: 写索引 `db.rs` 的写库函数 + `indexer.rs`

**Files:** Modify `src-tauri/src/db.rs`（加写函数）；Create `src-tauri/src/indexer.rs`；Modify `src-tauri/src/lib.rs`

- [ ] **Step 1: 在 `db.rs` 加 device / asset 的读写函数与测试**

追加到 `db.rs`（`migrate` 之后）：

```rust
use crate::pairing::PairedAsset;

pub fn upsert_device(
    conn: &Connection,
    serial: &str,
    model: &str,
    display_name: Option<&str>,
    folder_name: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO device (serial, model, display_name, folder_name, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(serial) DO UPDATE SET
           model=excluded.model,
           display_name=excluded.display_name,
           folder_name=excluded.folder_name,
           last_seen_at=excluded.last_seen_at",
        params![
            serial,
            model,
            display_name,
            folder_name,
            now_epoch()
        ],
    )?;
    conn.query_row("SELECT id FROM device WHERE serial = ?1", params![serial], |r| r.get(0))
}

/// 写入/更新一个逻辑条目。业务键是 (device_id, base_name, taken_at)。
/// `taken_at` 本计划暂时用 0（读不到拍摄时间，属计划 5），因此业务键退化为
/// (device_id, base_name)。这是本计划的已知近似，见文末「已知偏差」。
pub fn upsert_asset(
    conn: &Connection,
    device_id: i64,
    asset: &PairedAsset,
    taken_at: i64,
) -> rusqlite::Result<()> {
    let (still_path, still_ext, still_size) = split(&asset.still);
    let (movie_path, movie_ext, movie_size) = split(&asset.movie);
    conn.execute(
        "INSERT INTO asset
           (device_id, kind, base_name, taken_at, still_path, still_ext, still_size,
            movie_path, movie_ext, movie_size, integrity, missing, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0,?12,?12)
         ON CONFLICT(device_id, base_name, taken_at) DO UPDATE SET
           kind=excluded.kind,
           still_path=excluded.still_path,
           still_ext=excluded.still_ext,
           still_size=excluded.still_size,
           movie_path=excluded.movie_path,
           movie_ext=excluded.movie_ext,
           movie_size=excluded.movie_size,
           integrity=excluded.integrity,
           missing=0,
           updated_at=excluded.updated_at",
        params![
            device_id,
            asset.kind,
            asset.base_name,
            taken_at,
            still_path,
            still_ext,
            still_size,
            movie_path,
            movie_ext,
            movie_size,
            asset.integrity,
            now_epoch(),
        ],
    )?;
    Ok(())
}

fn split(f: &Option<crate::pairing::FileRef>) -> (Option<String>, Option<String>, Option<i64>) {
    match f {
        Some(fr) => (
            Some(fr.path.clone()),
            Some(fr.ext.clone()),
            Some(fr.size as i64),
        ),
        None => (None, None, None),
    }
}

pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
```

追加测试到 `db.rs` 的 `tests` 模块：

```rust
    #[test]
    fn upsert_asset_is_idempotent_and_marks_missing_zero() {
        use crate::pairing::PairedAsset;
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "iPhone15Pro", None, "iPhone15Pro-3F9A2C").unwrap();

        let asset = PairedAsset {
            base_name: "IMG_0001".into(),
            kind: crate::pairing::KIND_PHOTO,
            integrity: crate::pairing::INTEGRITY_STILL_ONLY,
            still: Some(crate::pairing::FileRef {
                path: "originals/dev/2024/IMG_0001.JPG".into(),
                ext: "jpg".into(),
                size: 42,
            }),
            movie: None,
        };

        conn.execute("UPDATE asset SET missing=1", []).unwrap();
        upsert_asset(&conn, dev, &asset, 0).unwrap();
        upsert_asset(&conn, dev, &asset, 0).unwrap(); // 再来一次不应报错

        let (n, missing): (i64, i64) = conn
            .query_row("SELECT count(*), max(missing) FROM asset", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(n, 1, "同一业务键应只有一行");
        assert_eq!(missing, 0, "重扫应把 missing 清回 0");
    }
```

- [ ] **Step 2: 运行测试，确认通过**

Run:
```powershell
cargo test -p liveporter db
```
Expected: 之前 3 个 + 新增 1 个 = 4 个通过。

- [ ] **Step 3: 写 `indexer.rs`（遍历 + 调用上面两个函数）**

Create `src-tauri/src/indexer.rs`：
```rust
use std::path::Path;

use rusqlite::Connection;

use crate::library::Library;
use crate::pairing::{pair_files, PairedAsset};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub devices: usize,
    pub files: usize,
    pub assets: usize,
}

/// 扫描 `originals/<device_folder>/<year>/` 下的文件，配对后写进索引。
/// 幂等：重复扫描不产生重复行。
pub fn scan_library(lib: &Library, conn: &Connection) -> anyhow::Result<ScanSummary> {
    let mut summary = ScanSummary::default();
    let originals = lib.originals_dir();
    if !originals.is_dir() {
        return Ok(summary);
    }

    for device_entry in std::fs::read_dir(&originals)?.flatten() {
        let device_dir = device_entry.path();
        if !device_dir.is_dir() {
            continue;
        }
        let folder_name = device_dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        summary.devices += 1;

        // 本计划没有设备元数据，序列号先用目录名占位，计划 4 写入真实序列号。
        let device_id = crate::db::upsert_device(
            conn,
            &folder_name,
            "unknown",
            None,
            &folder_name,
        )?;

        for year_entry in std::fs::read_dir(&device_dir)?.flatten() {
            let year_dir = year_entry.path();
            if !year_dir.is_dir() {
                continue;
            }
            let files = collect_files(&year_dir);
            summary.files += files.len();
            for asset in pair_files(&files) {
                crate::db::upsert_asset(conn, device_id, &asset, 0)?;
                summary.assets += 1;
            }
        }

        // 该设备下已消失的条目标记为 missing=1
        let _ = mark_missing(conn, device_id);
    }

    Ok(summary)
}

fn collect_files(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            continue;
        }
        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
        out.push((path.to_string_lossy().to_string(), size));
    }
    out.sort();
    out
}

fn mark_missing(conn: &Connection, device_id: i64) -> rusqlite::Result<()> {
    // 把该设备下所有条目先置 missing=1，再在 upsert 时清回 0。
    // 这里采用更简单可靠的做法：重新扫描前先全部置 1。
    conn.execute(
        "UPDATE asset SET missing = 1 WHERE device_id = ?1",
        rusqlite::params![device_id],
    )?;
    Ok(())
}

/// 把某个 PairedAsset 的配对结果用于断言测试（本函数仅为测试可见性）。
#[cfg(test)]
pub(crate) fn _assert_paired(a: &PairedAsset) {
    assert!(!a.base_name.is_empty());
}
```

> 注意 `mark_missing` 的调用顺序：**必须**在 `upsert_asset` 之前把该设备整体置 `missing=1`，否则会误标。请把上面 `for asset ... upsert` 与 `mark_missing` 的先后顺序调整为：**先 `mark_missing`，再遍历 upsert**。实现时按此修正（这是本计划的一处自查发现）。

- [ ] **Step 4: 加端到端单测**

追加到 `indexer.rs` 末尾：
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::library::Library;

    #[test]
    fn scans_and_indexes_idempotently() {
        let root = std::env::temp_dir().join("lpm_indexer_test");
        let _ = std::fs::remove_dir_all(&root);
        let lib = Library::open(&root).unwrap();

        let year = lib.originals_dir().join("iPhone15Pro-3F9A2C").join("2024");
        std::fs::create_dir_all(&year).unwrap();
        std::fs::write(year.join("IMG_0001.HEIC"), b"a").unwrap();
        std::fs::write(year.join("IMG_0001.MOV"), b"bb").unwrap();
        std::fs::write(year.join("IMG_0002.JPG"), b"ccc").unwrap();
        std::fs::write(year.join("notes.txt"), b"ignored").unwrap();

        let conn = db::open_in_memory().unwrap();

        let s1 = scan_library(&lib, &conn).unwrap();
        assert_eq!(s1.devices, 1);
        assert_eq!(s1.assets, 2);
        assert_eq!(s1.files, 4);

        let s2 = scan_library(&lib, &conn).unwrap();
        assert_eq!(s2.assets, 2, "重扫不应新增条目");

        let n: i64 = conn.query_row("SELECT count(*) FROM asset", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);

        let live: i64 = conn
            .query_row("SELECT count(*) FROM asset WHERE kind=3 AND integrity=0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(live, 1);
    }
}
```

在 `lib.rs` 顶部加 `mod indexer;`。

- [ ] **Step 5: 运行测试，确认通过**

Run:
```powershell
cargo test -p liveporter indexer
```
Expected: 1 个测试通过。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/db.rs src-tauri/src/indexer.rs src-tauri/src/lib.rs
git commit -m "feat(app): scan originals and write asset index"
```

---

## Task 5: 查询（分页 + 统计）

**Files:** Modify `src-tauri/src/db.rs`

- [ ] **Step 1: 写失败的测试**

追加到 `db.rs`：
```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetRow {
    pub id: i64,
    pub kind: i64,
    pub integrity: i64,
    pub base_name: String,
    pub still_path: Option<String>,
    pub movie_path: Option<String>,
    pub missing: bool,
}

pub fn page_assets(
    conn: &Connection,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<AssetRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, integrity, base_name, still_path, movie_path, missing
         FROM asset
         ORDER BY base_name
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |r| {
        Ok(AssetRow {
            id: r.get(0)?,
            kind: r.get(1)?,
            integrity: r.get(2)?,
            base_name: r.get(3)?,
            still_path: r.get(4)?,
            movie_path: r.get(5)?,
            missing: r.get::<_, i64>(6)? != 0,
        })
    })?;
    rows.collect()
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct LibraryStats {
    pub total: i64,
    pub live: i64,
    pub photo: i64,
    pub video: i64,
    pub missing: i64,
}

pub fn stats(conn: &Connection) -> rusqlite::Result<LibraryStats> {
    conn.query_row(
        "SELECT
           count(*),
           coalesce(sum(CASE WHEN kind=3 THEN 1 ELSE 0 END),0),
           coalesce(sum(CASE WHEN kind=1 THEN 1 ELSE 0 END),0),
           coalesce(sum(CASE WHEN kind=2 THEN 1 ELSE 0 END),0),
           coalesce(sum(missing),0)
         FROM asset",
        [],
        |r| {
            Ok(LibraryStats {
                total: r.get(0)?,
                live: r.get(1)?,
                photo: r.get(2)?,
                video: r.get(3)?,
                missing: r.get(4)?,
            })
        },
    )
}
```

追加测试：
```rust
    #[test]
    fn page_and_stats_work() {
        use crate::pairing::*;
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        for (i, name) in ["A", "B", "C"].iter().enumerate() {
            let a = PairedAsset {
                base_name: (*name).into(),
                kind: if i == 0 { KIND_LIVE } else { KIND_PHOTO },
                integrity: INTEGRITY_STILL_ONLY,
                still: Some(FileRef { path: format!("{name}.JPG"), ext: "jpg".into(), size: 1 }),
                movie: if i == 0 {
                    Some(FileRef { path: format!("{name}.MOV"), ext: "mov".into(), size: 1 })
                } else {
                    None
                },
            };
            upsert_asset(&conn, dev, &a, 0).unwrap();
        }

        let page = page_assets(&conn, 1, 1).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].base_name, "B");

        let s = stats(&conn).unwrap();
        assert_eq!(s.total, 3);
        assert_eq!(s.live, 1);
        assert_eq!(s.photo, 2);
    }
```

- [ ] **Step 2: 运行测试，确认通过**

Run:
```powershell
cargo test -p liveporter db
```
Expected: 全部通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/db.rs
git commit -m "feat(app): add asset pagination and library stats"
```

---

## Task 6: Tauri 命令与状态

**Files:** Create `src-tauri/src/commands.rs`；Modify `src-tauri/src/state.rs`、`src-tauri/src/lib.rs`

- [ ] **Step 1: 改造 `state.rs`**

```rust
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::library::Library;

/// 全局状态：当前打开的库 + 它的索引连接。
/// 未打开库时 `library` 为 None，所有库相关命令返回明确错误。
pub struct AppState {
    inner: Mutex<Option<Open>>,
}

struct Open {
    library: Library,
    conn: Connection,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            inner: Mutex::new(None),
        }
    }
}

impl AppState {
    pub fn open(&self, root: PathBuf) -> anyhow::Result<()> {
        let library = Library::open(root)?;
        let conn = crate::db::open(&library.db_path())?;
        *self.inner.lock().expect("poisoned") = Some(Open { library, conn });
        Ok(())
    }

    /// 在库打开时对其执行一段操作。
    pub fn with<T>(&self, f: impl FnOnce(&Library, &Connection) -> T) -> Option<T> {
        let guard = self.inner.lock().expect("poisoned");
        guard.as_ref().map(|o| f(&o.library, &o.conn))
    }

    pub fn db_path_if_open(&self) -> Option<PathBuf> {
        self.with(|lib, _| lib.db_path())
    }
}
```

> 计划 2 的 `AppState` 只有 `allowed_root`，`lpm://` 协议依赖它。**保留协议所需的「允许根」语义**：把 `allowed_root` 改成从当前打开的库取（库根即允许根）；未打开库时协议一律 403。相应地 `commands::scan_dir`（计划 2 的本地目录预览）可保留但不再是主路径，或直接删除——实现时二选一并说明。

- [ ] **Step 2: 写 `commands.rs`**

```rust
use crate::state::AppState;

#[tauri::command]
pub fn open_library(path: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.open(std::path::PathBuf::from(path)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_library(state: tauri::State<'_, AppState>) -> Result<crate::indexer::ScanSummary, String> {
    state
        .with(|lib, conn| crate::indexer::scan_library(lib, conn))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn library_stats(state: tauri::State<'_, AppState>) -> Result<crate::db::LibraryStats, String> {
    state
        .with(|_, conn| crate::db::stats(conn))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_assets(
    offset: i64,
    limit: i64,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::db::AssetRow>, String> {
    state
        .with(|_, conn| crate::db::page_assets(conn, offset, limit))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}
```

`ScanSummary` 与 `LibraryStats`、`AssetRow` 需 `serde::Serialize`（前两者补 `derive`）。在 `lib.rs` 加 `mod commands;` 并把命令注册进 `generate_handler!`。

- [ ] **Step 3: 加 `ScanSummary` 的 Serialize**

`indexer.rs` 的 `ScanSummary` 加 `#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]`。

- [ ] **Step 4: 编译**

Run:
```powershell
cargo check -p liveporter
```
Expected: 通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands.rs src-tauri/src/state.rs src-tauri/src/lib.rs src-tauri/src/indexer.rs
git commit -m "feat(app): expose library commands to the frontend"
```

---

## Task 7: 前端最小接线

**Files:** Modify `src/stores/library.ts`、`src/App.vue`

**目标：** 只证明「索引能被 UI 读出来」，不做浏览体验优化（计划 4）。

- [ ] **Step 1: 改 store**

`src/stores/library.ts` 改为：
```ts
import { defineStore } from "pinia";
import { shallowRef, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface AssetRow {
  id: number;
  kind: number;
  integrity: number;
  base_name: string;
  still_path: string | null;
  movie_path: string | null;
  missing: boolean;
}

export interface LibraryStats {
  total: number;
  live: number;
  photo: number;
  video: number;
  missing: number;
}

export const useLibrary = defineStore("library", () => {
  const root = ref<string | null>(null);
  const assets = shallowRef<AssetRow[]>([]);
  const stats = ref<LibraryStats | null>(null);
  const busy = ref(false);
  const error = ref<string | null>(null);

  async function openLibrary(path: string) {
    await invoke("open_library", { path });
    root.value = path;
  }

  async function rescan() {
    busy.value = true;
    error.value = null;
    try {
      await invoke("scan_library");
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      busy.value = false;
    }
  }

  async function refresh() {
    stats.value = await invoke<LibraryStats>("library_stats");
    // 本计划先一次性取前 5000 条；分页 + 虚拟滚动属计划 4。
    assets.value = await invoke<AssetRow[]>("list_assets", { offset: 0, limit: 5000 });
  }

  return { root, assets, stats, busy, error, openLibrary, rescan, refresh };
});
```

- [ ] **Step 2: 改 App.vue**

在计划 2 的 App.vue 基础上：把「选择文件夹」换成「打开库」（仍用 dialog 选目录），加「重建索引」按钮，显示统计；网格数据由 `assets` 映射而来：
```ts
const imageItems = computed(() =>
  lib.assets.value
    .filter((a) => a.still_path)
    .map((a) => ({ path: a.still_path as string, name: a.base_name, size: 0 })),
);
```
> `still_path` 目前是**绝对路径**（indexer 写入时用的是 `path.to_string_lossy()` 的绝对路径）。若后续改为相对路径，需要在这里拼库根。实现时统一：**索引里存绝对路径**，简化本计划。

- [ ] **Step 3: 类型检查与构建**

Run:
```powershell
npm run build
```
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src/stores/library.ts src/App.vue
git commit -m "feat(ui): read grid data from the sqlite index"
```

---

## Task 8: 端到端实测

**Files:** Create `docs/superpowers/notes/2026-09-17-index-smoke.md`

- [ ] **Step 1: 造一个测试库**

手工造出库目录（模拟将来设备导入后的样子）：

```powershell
$root = "C:\Users\Raven\AppData\Local\Temp\opencode\testlib"
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
$y = "$root\originals\iPhone15Pro-3F9A2C\2024"
New-Item -ItemType Directory -Force -Path $y | Out-Null
# 从本机图片目录拷 20 张 JPG，重命名成 IMG_0001..；再给前 10 个各配一个 .MOV（用任意小文件占位即可，本计划不读内容）
```

- [ ] **Step 2: 跑应用实测**

Run:
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
npm run tauri dev
```
Expected:
1. 点「打开库」选上面的 `testlib` → 目录结构被创建/识别。
2. 点「重建索引」→ 统计显示 条目总数 / 实况 / 照片 / 视频 与手工预期一致（20 个条目，其中 10 个实况）。
3. 网格显示索引中的图（能显示的是 JPG；若放入 HEIC 会破图，属正常）。
4. 再点一次「重建索引」→ 统计数字**不变**（幂等）。
5. 删掉库里一个文件，再重建 → `missing` 计数 +1。

- [ ] **Step 3: 用 sqlite 直接核对（可选但推荐）**

```powershell
# 若装有 sqlite3；否则用任意 SQLite 工具打开 .lpm\index.db
sqlite3 "$root\.lpm\index.db" "select kind, integrity, count(*) from asset group by 1,2;"
```

- [ ] **Step 4: 记录结果**

把实测输出写进 `docs/superpowers/notes/2026-09-17-index-smoke.md`。

- [ ] **Step 5: 提交**

```bash
git add docs/superpowers/notes/2026-09-17-index-smoke.md
git commit -m "docs: record stage-2 index smoke test"
```

---

## Task 9: 收尾验证

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
git commit -m "chore: stage 2 verification and progress update"
```

---

## 完成判据

- [ ] `Library::open` 能创建/校验库目录结构，越界路径被拒（有单测）
- [ ] schema 建立且迁移幂等（有单测）
- [ ] 配对与 integrity 分类正确，`IMG_E…` 变体不被误配（有单测）
- [ ] 扫描写索引幂等：重复扫描不新增行、`missing` 正确复位（有单测）
- [ ] 分页与统计正确（有单测）
- [ ] 端到端：打开库 → 重建索引 → 统计与手工预期一致 → 再重建数字不变
- [ ] 删文件后重建，`missing` 计数增加
- [ ] `cargo test / clippy / fmt`、`vue-tsc`、`npm run build` 全绿
- [ ] **未引入** WPD、ffmpeg、ContentIdentifier 解析

## 明确不在本计划内

- WPD/设备导入、状态机、断点续传（计划 4，对应设计 §13 第 4 步）
- ffmpeg 缩略图 / 预览片（计划 5，对应设计 §13 第 5 步）
- ContentIdentifier 校验、integrity 1/2/5、诊断报告（计划 5，对应设计 §13 第 6 步）
- 分页 + 虚拟滚动的联动、时间线分组、搜索筛选、多选导出、删除（计划 6，对应设计 §9）
- 样式与浏览体验（计划 6）

---

## 已知偏差（必须如实记录）

1. **integrity=0 是"未经 UUID 校验"的临时值。** 本计划读不到 `ContentIdentifier`，凡同名静态图+视频即判 0，`content_id` 留 NULL。计划 5 补齐校验后可能降级为 1/2。UI 上不应把 0 宣传为"已校验一致"。
2. **`taken_at` 暂用 0。** 拍摄时间要从文件元数据读（属计划 5），因此业务键 `(device_id, base_name, taken_at)` 实际退化为 `(device_id, base_name)`。若同目录存在同主名但拍摄时间不同的两张照片，会被误合并——这是计划 5 之前可接受的近似。
3. **`device` 的 serial 先用设备目录名占位**（`serial = folder_name`），计划 4 会用真实序列号替换并做映射。
4. **索引里存绝对路径。** 设计 §5.2 的 `still_path` 是「相对库根路径」。本计划为了简化前端拼接，先存绝对路径；计划 4/6 需要重定位库时再迁回相对路径（注意：库被移动后绝对路径会失效，这是本计划的已知限制）。

## 自查记录

**规格覆盖：** 对应设计 §13 第 3 步、§4（库目录结构）、§5（数据模型与去重）、§5.4（文件是唯一真相）。

**类型一致性：** `PairedAsset`（`pairing.rs`）→ `upsert_asset`（`db.rs`）→ `AssetRow`（`db.rs`）→ TS `AssetRow`（`stores/library.ts`）字段对齐。

**已知不确定点（实现时必须查证）：**

1. `rusqlite` 实际版本与 `params!` 用法（Task 0）
2. `PRAGMA journal_mode=WAL` 的正确设置方式（Task 2）
3. 计划 2 遗留：`state.rs` 里 `allowed_root` 与 `lpm://` 协议的耦合如何迁移（Task 6 Step 1 已指出，需二选一）
4. `serde::Serialize` 派生的字段名是否与 TS 端一致（默认 snake_case，TS 端也按 snake_case 写）

**对计划 2 的承接：** 会改动 `state.rs`（`allowed_root` → 库根）与 `stores/library.ts`，并可能删除计划 2 的 `scanner::scan_dir` 主路径。改完必须保证 `lpm://` 协议仍能取图（计划 2 的 8 个测试与交互验证不能回退）。
