use rusqlite::{params, Connection, OptionalExtension};

use crate::pairing::{FileRef, PairedAsset};

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
#[cfg(test)]
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

pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

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
        params![serial, model, display_name, folder_name, now_epoch()],
    )?;
    conn.query_row(
        "SELECT id FROM device WHERE serial = ?1",
        params![serial],
        |r| r.get(0),
    )
}

fn split(f: &Option<FileRef>) -> (Option<String>, Option<String>, Option<i64>) {
    match f {
        Some(fr) => (
            Some(fr.path.clone()),
            Some(fr.ext.clone()),
            Some(fr.size as i64),
        ),
        None => (None, None, None),
    }
}

/// 写入/更新一个逻辑条目。业务键是 (device_id, base_name, taken_at)。
/// 本计划 `taken_at` 暂用 0（读不到拍摄时间，属计划 5），因此业务键实际退化为
/// (device_id, base_name)。见计划文档「已知偏差 2」。
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

/// 扫描前把某设备下所有条目置 missing=1；命中的条目会在 upsert 时清回 0。
/// 必须在 upsert 之前调用，否则会误标。
pub fn mark_device_missing(conn: &Connection, device_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE asset SET missing = 1 WHERE device_id = ?1",
        params![device_id],
    )?;
    Ok(())
}

/// 更新条目的导入状态与错误信息。
pub fn set_asset_status(
    conn: &Connection,
    device_id: i64,
    base_name: &str,
    taken_at: i64,
    status: i64,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE asset SET status=?1, error=?2, updated_at=?3
         WHERE device_id=?4 AND base_name=?5 AND taken_at=?6",
        params![status, error, now_epoch(), device_id, base_name, taken_at],
    )?;
    Ok(())
}

/// 写入缩略图路径。
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

/// 取一个条目的视频源路径（用于生成预览片）。不存在返回 None。
pub fn asset_movie_path(conn: &Connection, asset_id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT movie_path FROM asset WHERE id = ?1",
        params![asset_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|o| o.flatten())
}

/// 写入预览片路径。
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

/// 已「完成」条目的大小映射：`(base_name, taken_at) -> (still_size, movie_size)`。
pub type ExistingSizes = std::collections::HashMap<(String, i64), (Option<u64>, Option<u64>)>;

/// 已「完成」条目的大小，供增量比对。
///
/// **只包含 status >= 1（copied 及以上）的条目**：`pending`(0)/`failed`(4) 不在此列，
/// 因而会被 `diff_tasks` 视为"仍需传输"，这正是断点续传需要的语义。
pub fn existing_sizes(conn: &Connection) -> rusqlite::Result<ExistingSizes> {
    let mut stmt = conn.prepare(
        "SELECT base_name, taken_at, still_size, movie_size FROM asset WHERE status >= 1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            (r.get::<_, String>(0)?, r.get::<_, i64>(1)?),
            (
                r.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                r.get::<_, Option<i64>>(3)?.map(|v| v as u64),
            ),
        ))
    })?;
    rows.collect()
}

#[derive(Debug, Clone)]
pub struct ThumbJob {
    pub device_id: i64,
    pub base_name: String,
    pub taken_at: i64,
    pub source_path: String,
}

/// 列出「缺少缩略图」的条目（文件存在、有源路径）。
pub fn assets_missing_thumbs(conn: &Connection) -> rusqlite::Result<Vec<ThumbJob>> {
    let mut stmt = conn.prepare(
        "SELECT device_id, base_name, taken_at, coalesce(still_path, movie_path)
         FROM asset
         WHERE missing = 0
           AND (thumb_path IS NULL OR thumb_path = '')
           AND (still_path IS NOT NULL OR movie_path IS NOT NULL)",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ThumbJob {
            device_id: r.get(0)?,
            base_name: r.get(1)?,
            taken_at: r.get(2)?,
            source_path: r.get(3)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetRow {
    pub id: i64,
    pub kind: i64,
    pub integrity: i64,
    pub base_name: String,
    pub still_path: Option<String>,
    pub movie_path: Option<String>,
    pub thumb_path: Option<String>,
    pub missing: bool,
}

pub fn page_assets(conn: &Connection, offset: i64, limit: i64) -> rusqlite::Result<Vec<AssetRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, integrity, base_name, still_path, movie_path, thumb_path, missing
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
            thumb_path: r.get(6)?,
            missing: r.get::<_, i64>(7)? != 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::{INTEGRITY_STILL_ONLY, KIND_LIVE, KIND_PHOTO};

    #[test]
    fn migrate_creates_tables_and_sets_version() {
        let conn = open_in_memory().unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
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
    fn open_file_db_creates_file_and_sets_version() {
        let dir = std::env::temp_dir().join("lpm_db_file_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.db");

        let conn = open(&path).unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        assert!(path.exists());
    }

    #[test]
    fn asset_movie_path_and_preview_path() {
        use crate::pairing::{FileRef, PairedAsset, KIND_LIVE};
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();

        let a = PairedAsset {
            base_name: "IMG_1".into(),
            kind: KIND_LIVE,
            integrity: 0,
            still: Some(FileRef {
                path: "s.heic".into(),
                ext: "heic".into(),
                size: 1,
            }),
            movie: Some(FileRef {
                path: "m.mov".into(),
                ext: "mov".into(),
                size: 2,
            }),
        };
        upsert_asset(&conn, dev, &a, 100).unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM asset", [], |r| r.get(0))
            .unwrap();

        assert_eq!(
            asset_movie_path(&conn, id).unwrap().as_deref(),
            Some("m.mov")
        );
        // 不存在的 id → None
        assert!(asset_movie_path(&conn, 9999).unwrap().is_none());

        set_preview_path(&conn, id, "p.mp4").unwrap();
        let p: Option<String> = conn
            .query_row("SELECT preview_path FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p.as_deref(), Some("p.mp4"));
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
        assert!(
            insert(&conn).is_err(),
            "同一 (device,base,taken_at) 应被唯一约束挡下"
        );
    }

    #[test]
    fn upsert_asset_is_idempotent_and_marks_missing_zero() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "iPhone15Pro", None, "iPhone15Pro-3F9A2C").unwrap();

        let asset = PairedAsset {
            base_name: "IMG_0001".into(),
            kind: KIND_PHOTO,
            integrity: INTEGRITY_STILL_ONLY,
            still: Some(FileRef {
                path: "originals/dev/2024/IMG_0001.JPG".into(),
                ext: "jpg".into(),
                size: 42,
            }),
            movie: None,
        };

        conn.execute("UPDATE asset SET missing=1", []).unwrap();
        upsert_asset(&conn, dev, &asset, 0).unwrap();
        upsert_asset(&conn, dev, &asset, 0).unwrap();

        let (n, missing): (i64, i64) = conn
            .query_row("SELECT count(*), max(missing) FROM asset", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(n, 1, "同一业务键应只有一行");
        assert_eq!(missing, 0, "重扫应把 missing 清回 0");
    }

    #[test]
    fn mark_device_missing_flags_all_until_upserted() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        let a = PairedAsset {
            base_name: "IMG_1".into(),
            kind: KIND_PHOTO,
            integrity: INTEGRITY_STILL_ONLY,
            still: Some(FileRef {
                path: "x".into(),
                ext: "jpg".into(),
                size: 1,
            }),
            movie: None,
        };
        upsert_asset(&conn, dev, &a, 0).unwrap();
        mark_device_missing(&conn, dev).unwrap();
        let missing: i64 = conn
            .query_row("SELECT missing FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(missing, 1);
    }

    #[test]
    fn page_and_stats_work() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        for (i, name) in ["A", "B", "C"].iter().enumerate() {
            let a = PairedAsset {
                base_name: (*name).into(),
                kind: if i == 0 { KIND_LIVE } else { KIND_PHOTO },
                integrity: INTEGRITY_STILL_ONLY,
                still: Some(FileRef {
                    path: format!("{name}.JPG"),
                    ext: "jpg".into(),
                    size: 1,
                }),
                movie: if i == 0 {
                    Some(FileRef {
                        path: format!("{name}.MOV"),
                        ext: "mov".into(),
                        size: 1,
                    })
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
}
