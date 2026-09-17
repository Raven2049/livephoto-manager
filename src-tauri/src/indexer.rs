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

/// 扫描 `originals/<device_folder>/<year>/` 下的文件，配对后写进索引。
/// 幂等：重复扫描不产生重复行；已消失的文件会被置 `missing=1`。
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

        // 本计划没有设备元数据，序列号先用目录名占位，计划 4 会用真实序列号替换。
        let device_id =
            crate::db::upsert_device(conn, &folder_name, "unknown", None, &folder_name)?;

        // 先把该设备下所有条目置 missing=1；下面遍历命中时会 upsert 清回 0。
        // 顺序不能反，否则会误标（见计划 Task 4 的自查发现）。
        crate::db::mark_device_missing(conn, device_id)?;

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
        scan_library(&lib, &conn).unwrap();
        std::fs::remove_file(year.join("IMG_1.JPG")).unwrap();
        scan_library(&lib, &conn).unwrap();

        let missing: i64 = conn
            .query_row("SELECT missing FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(missing, 1);
    }
}
