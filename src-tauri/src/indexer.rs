use std::path::Path;

use rusqlite::Connection;

use crate::library::Library;
use crate::pairing::pair_files;

#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScanSummary {
    pub devices: usize,
    pub files: usize,
    pub assets: usize,
}

/// 扫描进度（用于非阻塞进度条）。
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ScanProgress {
    pub devices: usize,
    pub files: usize,
    pub assets: usize,
}

/// 扫描 `originals/<device_folder>/<year>/` 下的文件，配对后写进索引。
/// 幂等：重复扫描不产生重复行；已消失的文件会被置 `missing=1`。
///
/// `cancel` 在每个目录前检查；`on_progress` 每个年份目录回调一次。
/// 测试与简单调用可传 `&|| false` 和 `|_| {}`。
pub fn scan_library_with(
    lib: &Library,
    conn: &Connection,
    cancel: &dyn Fn() -> bool,
    mut on_progress: impl FnMut(ScanProgress),
) -> anyhow::Result<ScanSummary> {
    let mut summary = ScanSummary::default();
    let originals = lib.originals_dir();
    if !originals.is_dir() {
        return Ok(summary);
    }

    for device_entry in std::fs::read_dir(&originals)?.flatten() {
        if cancel() {
            break;
        }
        let device_dir = device_entry.path();
        if !device_dir.is_dir() {
            continue;
        }
        let folder_name = device_dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        summary.devices += 1;

        // 优先复用该目录已存在的设备（通常是 WPD 导入建立的、带真实序列号与型号）。
        // 只有从未导入过、纯凭磁盘文件建库时，才用目录名占位。
        let device_id = match crate::db::device_id_for_folder(conn, &folder_name)? {
            Some(id) => id,
            None => crate::db::upsert_device(conn, &folder_name, "unknown", None, &folder_name)?,
        };

        // 先把该设备下所有条目置 missing=1；下面遍历命中时会 upsert 清回 0。
        // 顺序不能反，否则会误标。
        crate::db::mark_device_missing(conn, device_id)?;

        for year_entry in std::fs::read_dir(&device_dir)?.flatten() {
            if cancel() {
                break;
            }
            let year_dir = year_entry.path();
            if !year_dir.is_dir() {
                continue;
            }
            let files = collect_files(&year_dir);
            summary.files += files.len();
            for asset in pair_files(&files) {
                crate::db::upsert_asset_preserving_taken_at(conn, device_id, &asset)?;
                summary.assets += 1;
            }
            on_progress(ScanProgress {
                devices: summary.devices,
                files: summary.files,
                assets: summary.assets,
            });
        }
    }

    Ok(summary)
}

/// 列出目录下所有普通文件（不做扩展名过滤，配对函数内部会过滤）。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::library::Library;

    fn scan(lib: &Library, conn: &Connection) -> anyhow::Result<ScanSummary> {
        scan_library_with(lib, conn, &|| false, |_| {})
    }

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

        let s1 = scan(&lib, &conn).unwrap();
        assert_eq!(s1.devices, 1);
        assert_eq!(s1.assets, 2);
        assert_eq!(s1.files, 4);

        let s2 = scan(&lib, &conn).unwrap();
        assert_eq!(s2.assets, 2, "重扫不应新增条目");

        let n: i64 = conn
            .query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);

        let live: i64 = conn
            .query_row(
                "SELECT count(*) FROM asset WHERE kind=3 AND integrity=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live, 1);
    }

    #[test]
    fn missing_is_set_when_file_disappears() {
        let root = std::env::temp_dir().join("lpm_indexer_missing");
        let _ = std::fs::remove_dir_all(&root);
        let lib = Library::open(&root).unwrap();
        let year = lib.originals_dir().join("dev").join("2024");
        std::fs::create_dir_all(&year).unwrap();
        std::fs::write(year.join("IMG_1.JPG"), b"a").unwrap();

        let conn = db::open_in_memory().unwrap();
        scan(&lib, &conn).unwrap();
        std::fs::remove_file(year.join("IMG_1.JPG")).unwrap();
        scan(&lib, &conn).unwrap();

        let missing: i64 = conn
            .query_row("SELECT missing FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(missing, 1);
    }

    #[test]
    fn rescan_reuses_imported_device_and_preserves_taken_at() {
        let root = std::env::temp_dir().join("lpm_indexer_device_reuse");
        let _ = std::fs::remove_dir_all(&root);
        let lib = Library::open(&root).unwrap();
        let year = lib.originals_dir().join("AppleiPhone-CY4RV0").join("2025");
        std::fs::create_dir_all(&year).unwrap();
        std::fs::write(year.join("IMG_1.HEIC"), b"a").unwrap();

        let conn = db::open_in_memory().unwrap();
        // WPD 导入建立的设备行：真实序列号 + 真实型号。
        let dev = db::upsert_device(
            &conn,
            "GV95CY4RV0",
            "Apple iPhone",
            None,
            "AppleiPhone-CY4RV0",
        )
        .unwrap();
        // 已有一条带真实拍摄时间的条目。
        let existing = crate::pairing::PairedAsset {
            base_name: "IMG_1".into(),
            kind: crate::pairing::KIND_PHOTO,
            integrity: crate::pairing::INTEGRITY_STILL_ONLY,
            still: Some(crate::pairing::FileRef {
                path: year.join("IMG_1.HEIC").to_string_lossy().into_owned(),
                ext: "heic".into(),
                size: 1,
            }),
            movie: None,
        };
        db::upsert_asset(&conn, dev, &existing, 12345).unwrap();

        scan(&lib, &conn).unwrap();

        let devices: i64 = conn
            .query_row("SELECT count(*) FROM device", [], |r| r.get(0))
            .unwrap();
        assert_eq!(devices, 1, "重扫不应为同一目录新建第二个设备");
        let (n, taken): (i64, i64) = conn
            .query_row("SELECT count(*), max(taken_at) FROM asset", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(n, 1, "重扫不应新增重复条目");
        assert_eq!(taken, 12345, "已有条目的 taken_at 必须保留");
    }
}
