use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;

use crate::library::{Library, META_DIR};
use crate::pairing::pair_files;

#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScanSummary {
    /// 已废弃（保留字段以兼容前端事件）；现在按目录扫描，不再统计设备。
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

/// 递归扫描库根下的全部媒体文件，配对后写进索引。
///
/// - 跳过隐藏的 `.lpm/`（索引与缓存所在），不跟随符号链接/junction（防环）。
/// - 配对只在**同一目录内**按主名进行（实况的 HEIC/MOV 本就同目录）。
/// - `dir` 为相对库根的目录（`''` = 根），作为条目的业务键。
///
/// 幂等：重复扫描不产生重复行；已消失的文件会被置 `missing=1`。
/// `cancel` 在每个目录前检查；`on_progress` 每个含文件的目录回调一次。
pub fn scan_library_with(
    lib: &Library,
    conn: &Connection,
    cancel: &dyn Fn() -> bool,
    mut on_progress: impl FnMut(ScanProgress),
) -> anyhow::Result<ScanSummary> {
    let mut summary = ScanSummary::default();
    let root = lib.root().to_path_buf();
    if !root.is_dir() {
        return Ok(summary);
    }

    // 已有元数据来源拍摄时间的条目：重扫时跳过读文件（传 (0,UNKNOWN) 即保留旧值）。
    let have_time = crate::db::assets_with_meta_time(conn)?;

    // 整次扫描包在一个事务里：否则每条 upsert 都是独立事务并逐条落盘，
    // 在机械盘/被杀软扫描的盘上会慢到不可用（实测 2000 条约 150s → 事务内 <1s）。
    let tx = conn.unchecked_transaction()?;
    // 先把全部条目置 missing=1；命中的目录 upsert 时会清回 0。
    crate::db::mark_all_missing(&tx)?;
    walk(
        &root,
        &root,
        &tx,
        &have_time,
        cancel,
        &mut summary,
        &mut on_progress,
    )?;
    tx.commit()?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    dir: &Path,
    root: &Path,
    conn: &Connection,
    have_time: &HashSet<(String, String)>,
    cancel: &dyn Fn() -> bool,
    summary: &mut ScanSummary,
    on_progress: &mut impl FnMut(ScanProgress),
) -> anyhow::Result<()> {
    if cancel() {
        return Ok(());
    }

    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
    let mut files: Vec<(String, u64)> = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        // 不跟随重解析点，避免符号链接/junction 造成的环。
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if entry.file_name() == std::ffi::OsStr::new(META_DIR) {
                continue;
            }
            subdirs.push(path);
        } else if ft.is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            files.push((path.to_string_lossy().to_string(), size));
        }
    }

    if !files.is_empty() {
        summary.files += files.len();
        let rel = relative_dir(dir, root);
        for asset in pair_files(&files) {
            // 拍摄时间：已有元数据来源则跳过读文件；否则静态图优先（EXIF）、
            // 其次视频（mvhd），都读不到回退文件修改时间。
            let (taken_at, taken_src) =
                if have_time.contains(&(rel.clone(), asset.base_name.clone())) {
                    (0, crate::time::SRC_UNKNOWN)
                } else {
                    let src_path = asset
                        .still
                        .as_ref()
                        .map(|f| f.path.as_str())
                        .or_else(|| asset.movie.as_ref().map(|f| f.path.as_str()));
                    match src_path {
                        Some(p) => crate::time::capture_time(Path::new(p)),
                        None => (0, crate::time::SRC_UNKNOWN),
                    }
                };
            crate::db::upsert_scanned_asset(conn, &rel, &asset, taken_at, taken_src)?;
            summary.assets += 1;
        }
        on_progress(ScanProgress {
            devices: 0,
            files: summary.files,
            assets: summary.assets,
        });
    }

    // 目录名排序，保证扫描顺序稳定。
    subdirs.sort();
    for sub in subdirs {
        walk(&sub, root, conn, have_time, cancel, summary, on_progress)?;
    }
    Ok(())
}

/// 目录相对库根的路径，统一用 `/` 分隔；库根本身为 `""`。
fn relative_dir(dir: &Path, root: &Path) -> String {
    match dir.strip_prefix(root) {
        Ok(p) => p
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn scan(lib: &Library, conn: &Connection) -> anyhow::Result<ScanSummary> {
        scan_library_with(lib, conn, &|| false, |_| {})
    }

    #[test]
    fn scans_recursively_and_skips_dot_lpm() {
        let root = std::env::temp_dir().join("lpm_indexer_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let lib = Library::open(&root).unwrap();

        // 库根直接放一对实况。
        std::fs::write(root.join("IMG_0001.HEIC"), b"a").unwrap();
        std::fs::write(root.join("IMG_0001.MOV"), b"bb").unwrap();
        // 子目录里放一张照片与一个非媒体文件。
        let sub = root.join("MI10PRO").join("2020-08");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("IMG_0002.JPG"), b"ccc").unwrap();
        std::fs::write(sub.join("notes.txt"), b"ignored").unwrap();
        // `.lpm` 里的派生文件不得被当照片。
        std::fs::write(lib.thumbs_dir().join("hash.webp"), b"thumb").unwrap();

        let conn = db::open_in_memory().unwrap();
        let s1 = scan(&lib, &conn).unwrap();
        assert_eq!(s1.assets, 2, "库根一对 + 子目录一张");
        assert_eq!(s1.files, 4, "含 notes.txt；不含 .lpm 里的缩略图");

        let s2 = scan(&lib, &conn).unwrap();
        assert_eq!(s2.assets, 2, "重扫不应新增条目");

        let n: i64 = conn
            .query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);

        // dir 归属正确。
        let root_dir: String = conn
            .query_row(
                "SELECT dir FROM asset WHERE base_name='IMG_0001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(root_dir, "");
        let sub_dir: String = conn
            .query_row(
                "SELECT dir FROM asset WHERE base_name='IMG_0002'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sub_dir, "MI10PRO/2020-08");
    }

    #[test]
    fn same_base_name_in_different_dirs_stays_distinct() {
        let root = std::env::temp_dir().join("lpm_indexer_dupnames");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let lib = Library::open(&root).unwrap();

        for sub in ["phoneA", "phoneB"] {
            let d = root.join(sub);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("IMG_0001.JPG"), b"x").unwrap();
        }

        let conn = db::open_in_memory().unwrap();
        scan(&lib, &conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM asset WHERE base_name='IMG_0001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2, "同名不同目录必须各自独立");
    }

    #[test]
    fn missing_is_set_when_file_disappears() {
        let root = std::env::temp_dir().join("lpm_indexer_missing");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let lib = Library::open(&root).unwrap();
        std::fs::write(root.join("IMG_1.JPG"), b"a").unwrap();

        let conn = db::open_in_memory().unwrap();
        scan(&lib, &conn).unwrap();
        std::fs::remove_file(root.join("IMG_1.JPG")).unwrap();
        scan(&lib, &conn).unwrap();

        let missing: i64 = conn
            .query_row("SELECT missing FROM asset", [], |r| r.get(0))
            .unwrap();
        assert_eq!(missing, 1);
    }

    #[test]
    fn rescan_preserves_imported_taken_at() {
        let root = std::env::temp_dir().join("lpm_indexer_taken");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let lib = Library::open(&root).unwrap();
        std::fs::write(root.join("IMG_1.HEIC"), b"a").unwrap();

        let conn = db::open_in_memory().unwrap();
        // 模拟导入：库根（dir=""）一条真实 taken_at 的记录。
        let imported = crate::pairing::PairedAsset {
            base_name: "IMG_1".into(),
            kind: crate::pairing::KIND_PHOTO,
            integrity: crate::pairing::INTEGRITY_STILL_ONLY,
            still: Some(crate::pairing::FileRef {
                path: root.join("IMG_1.HEIC").to_string_lossy().into_owned(),
                ext: "heic".into(),
                size: 1,
            }),
            movie: None,
        };
        // 模拟导入：库根（dir=""）一条真实 taken_at 的导入记录（taken_src=2）。
        db::upsert_imported_asset(&conn, "", &imported, 12345, "SN1", "IMG_1").unwrap();

        scan(&lib, &conn).unwrap();

        let (n, taken): (i64, i64) = conn
            .query_row("SELECT count(*), max(taken_at) FROM asset", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(n, 1, "重扫不应新增重复条目");
        assert_eq!(taken, 12345, "已有条目的 taken_at 必须保留");
    }

    /// 真实目录冒烟：对 `LIVEPORTER_SMOKE_DIR` 指向的相册目录做一次扫描并打印统计。
    /// 只读取源文件、只在库根创建隐藏 `.lpm/`，不移动/改名任何照片。
    ///
    /// 运行：
    /// `$env:LIVEPORTER_SMOKE_DIR="D:\图片\Pictures\MI10PRO"`;
    /// `cargo test -p liveporter --lib real_directory_scan_smoke -- --ignored --nocapture`
    #[test]
    #[ignore = "requires a real photo directory (set LIVEPORTER_SMOKE_DIR)"]
    fn real_directory_scan_smoke() {
        let dir = match std::env::var("LIVEPORTER_SMOKE_DIR") {
            Ok(d) if !d.is_empty() => d,
            _ => {
                eprintln!("未设置 LIVEPORTER_SMOKE_DIR，跳过");
                return;
            }
        };
        let lib = Library::open(&dir).unwrap();
        let conn = db::open(&lib.db_path()).unwrap();

        let started = std::time::Instant::now();
        let s = scan(&lib, &conn).unwrap();
        let stats = db::stats(&conn).unwrap();

        let noise: i64 = conn
            .query_row(
                "SELECT count(*) FROM asset WHERE dir LIKE '.lpm%' OR base_name LIKE '%hash%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(noise, 0, "不得把 .lpm 缓存当照片索引");

        let dirs: i64 = conn
            .query_row("SELECT count(DISTINCT dir) FROM asset", [], |r| r.get(0))
            .unwrap();

        eprintln!("== 扫描完成 ==");
        eprintln!("库根: {}", lib.root().display());
        eprintln!("耗时: {:?}", started.elapsed());
        eprintln!("文件: {}  条目: {}  目录数: {}", s.files, s.assets, dirs);
        eprintln!(
            "统计: total={} live={} photo={} video={} missing={} missing_thumbs={}",
            stats.total, stats.live, stats.photo, stats.video, stats.missing, stats.missing_thumbs
        );
        eprintln!("完整性分布: {:?}", stats.by_integrity);

        eprintln!("-- 抽样（base_name | dir | kind | taken_at）--");
        let mut stmt = conn
            .prepare(
                "SELECT base_name, dir, kind, taken_at FROM asset
                 WHERE dir NOT IN ('larges','thumbs','previews')
                 ORDER BY id LIMIT 12",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .unwrap();
        for row in rows {
            let (b, d, k, t) = row.unwrap();
            eprintln!(
                "  {b}  |  {}  |  kind={k}  taken_at={t}",
                if d.is_empty() { "<根>" } else { &d }
            );
        }
    }
}
