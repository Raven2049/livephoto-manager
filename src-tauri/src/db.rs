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

/// 按库内目录名找设备行。重扫（`indexer`）拿不到真实序列号，必须复用导入时
/// 建立的设备，否则同一台手机会有两个 `device`，业务键含 `device_id` 会导致重复条目。
/// 同一目录若有多个设备行，优先返回非 `unknown` 型号的那个（即真实导入建立的）。
pub fn device_id_for_folder(conn: &Connection, folder_name: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM device WHERE folder_name = ?1
         ORDER BY (model = 'unknown') ASC, id ASC LIMIT 1",
        params![folder_name],
        |r| r.get(0),
    )
    .optional()
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

/// 重扫专用：同一 (device_id, base_name) 若已有条目，沿用其 taken_at；否则用 0。
///
/// `indexer`（扫描磁盘）拿不到拍摄时间，若直接用 0 会与导入时写入的真 `taken_at`
/// 组成不同的业务键，导致重扫新增重复行。它只能防止继续产生重复，
/// 不能清理历史遗留的重复行（那些行 `taken_at` 已然不同）。
pub fn upsert_asset_preserving_taken_at(
    conn: &Connection,
    device_id: i64,
    asset: &PairedAsset,
) -> rusqlite::Result<()> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT taken_at FROM asset WHERE device_id = ?1 AND base_name = ?2
             ORDER BY taken_at DESC LIMIT 1",
            params![device_id, asset.base_name],
            |r| r.get(0),
        )
        .optional()?;
    upsert_asset(conn, device_id, asset, existing.unwrap_or(0))
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

/// 写入 ContentIdentifier（静态图优先，其次视频）。
pub fn set_content_id(
    conn: &Connection,
    device_id: i64,
    base_name: &str,
    taken_at: i64,
    content_id: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE asset SET content_id=?1, updated_at=?2
         WHERE device_id=?3 AND base_name=?4 AND taken_at=?5",
        params![content_id, now_epoch(), device_id, base_name, taken_at],
    )?;
    Ok(())
}

/// 写入完整性分类。
pub fn set_integrity(
    conn: &Connection,
    device_id: i64,
    base_name: &str,
    taken_at: i64,
    integrity: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE asset SET integrity=?1, updated_at=?2
         WHERE device_id=?3 AND base_name=?4 AND taken_at=?5",
        params![integrity, now_epoch(), device_id, base_name, taken_at],
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

/// 列出所有需要重算 integrity 的条目（文件未标记缺失）。
#[derive(Debug, Clone)]
pub struct ClassifyJob {
    pub device_id: i64,
    pub base_name: String,
    pub taken_at: i64,
    pub still_path: Option<String>,
    pub movie_path: Option<String>,
    pub still_size: Option<i64>,
}

pub fn assets_for_classify(conn: &Connection) -> rusqlite::Result<Vec<ClassifyJob>> {
    let mut stmt = conn.prepare(
        "SELECT device_id, base_name, taken_at, still_path, movie_path, still_size
         FROM asset WHERE missing = 0",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ClassifyJob {
            device_id: r.get(0)?,
            base_name: r.get(1)?,
            taken_at: r.get(2)?,
            still_path: r.get(3)?,
            movie_path: r.get(4)?,
            still_size: r.get(5)?,
        })
    })?;
    rows.collect()
}

/// 删除指定 id 的条目，返回实际删除行数。
pub fn delete_assets(conn: &Connection, ids: &[i64]) -> rusqlite::Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let ph = vec!["?"; ids.len()].join(",");
    let sql = format!("DELETE FROM asset WHERE id IN ({ph})");
    conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
}

/// 还有多少条目引用同一个缩略图路径（用于判断缓存是否为孤儿）。
pub fn thumb_ref_count(conn: &Connection, path: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM asset WHERE thumb_path = ?1",
        params![path],
        |r| r.get(0),
    )
}

/// 还有多少条目引用同一个预览片路径。
pub fn preview_ref_count(conn: &Connection, path: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM asset WHERE preview_path = ?1",
        params![path],
        |r| r.get(0),
    )
}

/// 取第一条设备记录（型号, 序列号），供诊断报告。
pub fn first_device(conn: &Connection) -> rusqlite::Result<Option<(String, String)>> {
    conn.query_row(
        "SELECT model, serial FROM device ORDER BY id LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

/// 取失败条目的错误原文（设计 §10.2）。
pub fn failed_errors(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT error FROM asset WHERE error IS NOT NULL AND error <> '' ORDER BY updated_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| r.get::<_, String>(0))?;
    rows.collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetRow {
    pub id: i64,
    pub kind: i64,
    pub integrity: i64,
    pub taken_at: i64,
    pub base_name: String,
    pub still_path: Option<String>,
    pub movie_path: Option<String>,
    pub thumb_path: Option<String>,
    pub missing: bool,
}

/// 转义 SQLite LIKE 的通配符（`%` `_` `\`），配合 `ESCAPE '\'` 使用。
pub fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AssetFilter {
    /// 按 base_name 子串匹配（LIKE，大小写不敏感由 SQLite 的 ASCII 规则处理）。
    pub text: Option<String>,
    /// 1=photo 2=video 3=live。
    pub kind: Option<i64>,
    /// 完整性白名单；空或 None 表示不过滤。
    pub integrity: Option<Vec<i64>>,
    /// 拍摄时间下界（epoch 秒，含）。
    pub from: Option<i64>,
    /// 拍摄时间上界（epoch 秒，含）。
    pub to: Option<i64>,
}

fn where_clause(f: &AssetFilter, params: &mut Vec<rusqlite::types::Value>) -> String {
    use rusqlite::types::Value;
    let mut clauses: Vec<String> = Vec::new();

    if let Some(t) = f.text.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        clauses.push("base_name LIKE ? ESCAPE '\\'".to_string());
        params.push(Value::Text(format!("%{}%", escape_like(t))));
    }
    if let Some(k) = f.kind {
        clauses.push("kind = ?".to_string());
        params.push(Value::Integer(k));
    }
    if let Some(list) = f.integrity.as_ref().filter(|l| !l.is_empty()) {
        let ph = vec!["?"; list.len()].join(",");
        clauses.push(format!("integrity IN ({ph})"));
        for v in list {
            params.push(Value::Integer(*v));
        }
    }
    if let Some(from) = f.from {
        clauses.push("taken_at >= ?".to_string());
        params.push(Value::Integer(from));
    }
    if let Some(to) = f.to {
        clauses.push("taken_at <= ?".to_string());
        params.push(Value::Integer(to));
    }
    if clauses.is_empty() {
        "1=1".to_string()
    } else {
        clauses.join(" AND ")
    }
}

fn map_asset_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AssetRow> {
    Ok(AssetRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        integrity: r.get(2)?,
        taken_at: r.get(3)?,
        base_name: r.get(4)?,
        still_path: r.get(5)?,
        movie_path: r.get(6)?,
        thumb_path: r.get(7)?,
        missing: r.get::<_, i64>(8)? != 0,
    })
}

pub fn page_assets_filtered(
    conn: &Connection,
    filter: &AssetFilter,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<AssetRow>> {
    let mut params = Vec::new();
    let w = where_clause(filter, &mut params);
    let sql = format!(
        "SELECT id, kind, integrity, taken_at, base_name, still_path, movie_path, thumb_path, missing
         FROM asset WHERE {w} ORDER BY taken_at DESC, id DESC LIMIT ? OFFSET ?"
    );
    params.push(rusqlite::types::Value::Integer(limit));
    params.push(rusqlite::types::Value::Integer(offset));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), map_asset_row)?;
    rows.collect()
}

pub fn count_assets(conn: &Connection, filter: &AssetFilter) -> rusqlite::Result<i64> {
    let mut params = Vec::new();
    let w = where_clause(filter, &mut params);
    let sql = format!("SELECT count(*) FROM asset WHERE {w}");
    conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
        r.get(0)
    })
}

pub fn asset_ids(conn: &Connection, filter: &AssetFilter) -> rusqlite::Result<Vec<i64>> {
    let mut params = Vec::new();
    let w = where_clause(filter, &mut params);
    let sql = format!("SELECT id FROM asset WHERE {w} ORDER BY taken_at DESC, id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(0))?;
    rows.collect()
}

/// 导出/删除需要的条目文件信息（含原文件与派生缓存路径）。
#[derive(Debug, Clone)]
pub struct AssetFiles {
    pub id: i64,
    pub base_name: String,
    pub still_path: Option<String>,
    pub still_ext: Option<String>,
    pub movie_path: Option<String>,
    pub movie_ext: Option<String>,
    pub thumb_path: Option<String>,
    pub preview_path: Option<String>,
}

/// 按 id 批量取条目文件信息。顺序不保证，空输入返回空。
pub fn assets_by_ids(conn: &Connection, ids: &[i64]) -> rusqlite::Result<Vec<AssetFiles>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ph = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT id, base_name, still_path, still_ext, movie_path, movie_ext, thumb_path, preview_path
         FROM asset WHERE id IN ({ph})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok(AssetFiles {
            id: r.get(0)?,
            base_name: r.get(1)?,
            still_path: r.get(2)?,
            still_ext: r.get(3)?,
            movie_path: r.get(4)?,
            movie_ext: r.get(5)?,
            thumb_path: r.get(6)?,
            preview_path: r.get(7)?,
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
    /// (integrity 值, 数量)，按值升序。用于展示异常项分布。
    pub by_integrity: Vec<(i64, i64)>,
}

pub fn stats(conn: &Connection) -> rusqlite::Result<LibraryStats> {
    let mut by_integrity: Vec<(i64, i64)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT integrity, count(*) FROM asset GROUP BY integrity ORDER BY integrity",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        for row in rows {
            by_integrity.push(row?);
        }
    }

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
                by_integrity,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::{INTEGRITY_STILL_ONLY, KIND_LIVE, KIND_PHOTO, KIND_VIDEO};

    fn seed(conn: &Connection, dev: i64, name: &str, taken_at: i64, kind: i64, integrity: i64) {
        let a = PairedAsset {
            base_name: name.into(),
            kind,
            integrity,
            still: Some(FileRef {
                path: format!("{name}.JPG"),
                ext: "jpg".into(),
                size: 10,
            }),
            movie: if kind == KIND_LIVE {
                Some(FileRef {
                    path: format!("{name}.MOV"),
                    ext: "mov".into(),
                    size: 20,
                })
            } else {
                None
            },
        };
        upsert_asset(conn, dev, &a, taken_at).unwrap();
    }

    #[test]
    fn escape_like_escapes_wildcards() {
        assert_eq!(escape_like("a%b_c\\d"), "a\\%b\\_c\\\\d");
    }

    #[test]
    fn filter_by_kind_integrity_and_date() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        seed(&conn, dev, "IMG_1", 100, KIND_LIVE, 0);
        seed(&conn, dev, "IMG_2", 200, KIND_PHOTO, 3);
        seed(&conn, dev, "IMG_3", 300, KIND_VIDEO, 4);

        let f = AssetFilter {
            kind: Some(KIND_PHOTO),
            ..Default::default()
        };
        let rows = page_assets_filtered(&conn, &f, 0, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].base_name, "IMG_2");

        let f = AssetFilter {
            integrity: Some(vec![3, 4]),
            ..Default::default()
        };
        assert_eq!(page_assets_filtered(&conn, &f, 0, 10).unwrap().len(), 2);

        let f = AssetFilter {
            from: Some(150),
            to: Some(299),
            ..Default::default()
        };
        let rows = page_assets_filtered(&conn, &f, 0, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].base_name, "IMG_2");
    }

    #[test]
    fn filter_text_matches_substring_and_escapes() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        seed(&conn, dev, "IMG_0001", 1, KIND_PHOTO, 3);
        seed(&conn, dev, "IMG_1002", 2, KIND_PHOTO, 3);

        let f = AssetFilter {
            text: Some("IMG_0".into()),
            ..Default::default()
        };
        let rows = page_assets_filtered(&conn, &f, 0, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].base_name, "IMG_0001");
    }

    #[test]
    fn page_orders_by_taken_at_desc_then_id_desc() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        seed(&conn, dev, "OLD", 100, KIND_PHOTO, 3);
        seed(&conn, dev, "NEW", 300, KIND_PHOTO, 3);
        seed(&conn, dev, "MID", 200, KIND_PHOTO, 3);

        let rows = page_assets_filtered(&conn, &AssetFilter::default(), 0, 10).unwrap();
        let names: Vec<_> = rows.iter().map(|r| r.base_name.as_str()).collect();
        assert_eq!(names, vec!["NEW", "MID", "OLD"]);
        assert_eq!(rows[0].taken_at, 300);
    }

    #[test]
    fn count_and_ids_match_the_filtered_page() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        seed(&conn, dev, "A", 1, KIND_PHOTO, 3);
        seed(&conn, dev, "B", 2, KIND_LIVE, 0);

        let f = AssetFilter {
            integrity: Some(vec![0]),
            ..Default::default()
        };
        assert_eq!(count_assets(&conn, &f).unwrap(), 1);
        assert_eq!(asset_ids(&conn, &f).unwrap().len(), 1);
        assert_eq!(page_assets_filtered(&conn, &f, 0, 10).unwrap().len(), 1);
    }

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

        let page = page_assets_filtered(&conn, &AssetFilter::default(), 1, 1).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].base_name, "B");

        let s = stats(&conn).unwrap();
        assert_eq!(s.total, 3);
        assert_eq!(s.live, 1);
        assert_eq!(s.photo, 2);
    }

    #[test]
    fn rescan_preserves_imported_taken_at() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        seed(&conn, dev, "IMG_1", 12345, KIND_PHOTO, 3);

        let a = PairedAsset {
            base_name: "IMG_1".into(),
            kind: KIND_PHOTO,
            integrity: INTEGRITY_STILL_ONLY,
            still: Some(FileRef {
                path: "IMG_1.JPG".into(),
                ext: "jpg".into(),
                size: 10,
            }),
            movie: None,
        };
        upsert_asset_preserving_taken_at(&conn, dev, &a).unwrap();

        let n: i64 = conn
            .query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "重扫不得因 taken_at=0 新增重复行");
        let t: i64 = conn
            .query_row("SELECT taken_at FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(t, 12345, "已有条目的 taken_at 必须保留");
    }

    #[test]
    fn assets_by_ids_returns_matching_rows_and_empty_for_empty_input() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        seed(&conn, dev, "A", 1, KIND_LIVE, 0);
        seed(&conn, dev, "B", 2, KIND_PHOTO, 3);

        assert!(assets_by_ids(&conn, &[]).unwrap().is_empty());

        let ids = asset_ids(&conn, &AssetFilter::default()).unwrap();
        let rows = assets_by_ids(&conn, &ids).unwrap();
        assert_eq!(rows.len(), 2);
        let a = rows.iter().find(|r| r.base_name == "A").unwrap();
        assert_eq!(a.still_ext.as_deref(), Some("jpg"));
        assert_eq!(a.movie_ext.as_deref(), Some("mov"));
    }

    #[test]
    fn delete_assets_removes_rows_and_counts_cache_refs() {
        let conn = open_in_memory().unwrap();
        let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
        seed(&conn, dev, "A", 1, KIND_PHOTO, 3);
        seed(&conn, dev, "B", 2, KIND_PHOTO, 3);
        conn.execute(
            "UPDATE asset SET thumb_path='thumbs/x.webp', preview_path='previews/y.mp4'",
            [],
        )
        .unwrap();

        let ids = asset_ids(&conn, &AssetFilter::default()).unwrap();
        assert_eq!(thumb_ref_count(&conn, "thumbs/x.webp").unwrap(), 2);

        delete_assets(&conn, &[ids[0]]).unwrap();
        assert_eq!(thumb_ref_count(&conn, "thumbs/x.webp").unwrap(), 1);

        delete_assets(&conn, &[ids[1]]).unwrap();
        assert_eq!(thumb_ref_count(&conn, "thumbs/x.webp").unwrap(), 0);
        assert_eq!(preview_ref_count(&conn, "previews/y.mp4").unwrap(), 0);
        delete_assets(&conn, &[]).unwrap();
    }
}
