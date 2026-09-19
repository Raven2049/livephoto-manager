//! 导入管道：目标路径规划、增量比对、状态机与断点续传。
//!
//! 本模块的**纯逻辑**（路径命名、比对、管道）不依赖设备：设备访问通过 `Transfer`
//! trait 注入，测试用假实现，因此可脱离 iPhone 完整单测。
//!
//! 新库模型：导入的文件直接平铺写入**库根**（数据库键 `dir = ""`），与已有照片同级；
//! 重名成对加 `_N` 后缀。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::pairing::{
    is_movie_name, is_still_name, FileRef, PairedAsset, INTEGRITY_OK, INTEGRITY_STILL_ONLY,
    INTEGRITY_VIDEO_ONLY, KIND_LIVE, KIND_PHOTO, KIND_VIDEO,
};

pub const STATUS_PENDING: i64 = 0;
pub const STATUS_COPIED: i64 = 1;
pub const STATUS_TRANSCODED: i64 = 2;
pub const STATUS_FAILED: i64 = 4;

/// 导入文件的库内目录：平铺在库根。
pub const IMPORT_DIR: &str = "";

// ---------------------------------------------------------------------------
// 目标命名
// ---------------------------------------------------------------------------

/// 生成不冲突的文件名。`names` 是本次要写的所有文件名（配对的两个）。
/// 若任一同名文件已存在，则整体加后缀 `_N`，保证配对关系同步改名。
pub fn unique_names(dir: &Path, names: &[String]) -> Vec<String> {
    let collide = |suffix: &str| {
        names
            .iter()
            .any(|n| dir.join(add_suffix(n, suffix)).exists())
    };
    if !collide("") {
        return names.to_vec();
    }
    let mut i = 1;
    loop {
        let suffix = format!("_{i}");
        if !collide(&suffix) {
            return names.iter().map(|n| add_suffix(n, &suffix)).collect();
        }
        i += 1;
    }
}

fn add_suffix(name: &str, suffix: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{}{}{}", &name[..i], suffix, &name[i..]),
        _ => format!("{name}{suffix}"),
    }
}

// ---------------------------------------------------------------------------
// 增量比对
// ---------------------------------------------------------------------------

/// 设备上的一个媒体文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFile {
    pub object_id: String,
    pub name: String,
    pub size: u64,
    pub taken_at: Option<i64>,
}

/// 一个逻辑条目的传输任务（静态图与视频可缺一）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferTask {
    pub base_name: String,
    pub taken_at: i64,
    pub still: Option<DeviceFile>,
    pub movie: Option<DeviceFile>,
}

/// 已导入条目的大小：`(src_serial, src_name) -> (still_size, movie_size)`。
pub use crate::db::ImportedSizes;

/// 与索引比对，产出需要传输的条目。仅当大小完全一致时才算"已存在"。
/// 去重键用导入来源 `(serial, 设备主名)`：平铺后落盘主名可能被加后缀，不能用它去重。
pub fn diff_tasks(
    device_files: &[DeviceFile],
    existing: &ImportedSizes,
    serial: &str,
) -> Vec<TransferTask> {
    let mut groups: BTreeMap<(String, i64), TransferTask> = BTreeMap::new();

    for f in device_files {
        let taken = f.taken_at.unwrap_or(0);
        let still = is_still_name(&f.name);
        let movie = is_movie_name(&f.name);
        if !still && !movie {
            continue;
        }
        let base = crate::pairing::base_name(&f.name).to_string();
        let entry = groups
            .entry((base.clone(), taken))
            .or_insert_with(|| TransferTask {
                base_name: base,
                taken_at: taken,
                still: None,
                movie: None,
            });
        if still {
            entry.still = Some(f.clone());
        } else {
            entry.movie = Some(f.clone());
        }
    }

    groups
        .into_values()
        .filter(|t| {
            let key = (serial.to_string(), t.base_name.clone());
            match existing.get(&key) {
                None => true,
                Some((still, movie)) => {
                    let same_still = match (&t.still, still) {
                        (Some(f), Some(s)) => f.size == *s,
                        (None, None) => true,
                        _ => false,
                    };
                    let same_movie = match (&t.movie, movie) {
                        (Some(f), Some(s)) => f.size == *s,
                        (None, None) => true,
                        _ => false,
                    };
                    !(same_still && same_movie)
                }
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 传输管道 + 状态机
// ---------------------------------------------------------------------------

/// 传输动作的抽象：真实实现走 WPD，测试用假的。
pub trait Transfer {
    /// 下载一个设备对象到 `dest`，返回写入字节数。
    fn fetch(&mut self, object_id: &str, dest: &Path) -> anyhow::Result<u64>;
}

/// 缩略图生成的抽象：真实实现调 ffmpeg，测试用假的。
/// 由实现按扩展名自行决定取静态图还是视频首帧。
pub trait Thumbs {
    fn make(&mut self, source: &Path, thumbs_dir: &Path) -> anyhow::Result<PathBuf>;
}

/// 缩略图回填进度/结果。
#[derive(Debug, Clone, Default, serde::Serialize, PartialEq, Eq)]
pub struct ThumbSummary {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
}

/// 为库里「缺少缩略图」的条目补生成缩略图。
/// 命令 `generate_thumbs` 与测试共用此函数，避免逻辑重复。
pub fn backfill_thumbs(
    lib: &crate::library::Library,
    conn: &Connection,
    thumbs: &mut dyn Thumbs,
    should_cancel: &dyn Fn() -> bool,
    mut on_progress: impl FnMut(&ThumbSummary),
) -> anyhow::Result<ThumbSummary> {
    let jobs = crate::db::assets_missing_thumbs(conn)?;
    let total = jobs.len();
    let mut summary = ThumbSummary {
        total,
        ..Default::default()
    };

    for job in &jobs {
        if should_cancel() {
            break;
        }
        let src = PathBuf::from(&job.source_path);
        match thumbs.make(&src, &lib.thumbs_dir()) {
            Ok(tp) => {
                crate::db::set_thumb_path(conn, &job.dir, &job.base_name, &tp.to_string_lossy())?;
                crate::db::set_asset_status(
                    conn,
                    &job.dir,
                    &job.base_name,
                    STATUS_TRANSCODED,
                    None,
                )?;
                summary.done += 1;
            }
            Err(e) => {
                summary.failed += 1;
                eprintln!("缩略图失败 {}: {e:#}", job.base_name);
            }
        }
        on_progress(&summary);
    }

    Ok(summary)
}

/// 导入进度（同时用于 Tauri 事件 payload）。
#[derive(Debug, Clone, Default, serde::Serialize, PartialEq, Eq)]
pub struct ImportProgress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub current: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub cancelled: bool,
}

/// 执行一批任务。每个条目独立：失败的标 `failed` 并保留 `error`，不阻断其余。
/// 这是断点续传的执行端——任务清单已由 `diff_tasks` 剔除已完成项。
///
/// `dest_dir` 是**库根**：文件平铺写入（数据库键 `dir = ""`）。
#[allow(clippy::too_many_arguments)]
pub fn run_tasks(
    tasks: &[TransferTask],
    dest_dir: &Path,
    thumbs_dir: &Path,
    src_serial: &str,
    conn: &Connection,
    transfer: &mut dyn Transfer,
    thumbs: &mut dyn Thumbs,
    ffprobe_bin: Option<&Path>,
    cloud_hint: bool,
    should_cancel: &dyn Fn() -> bool,
    mut on_progress: impl FnMut(&ImportProgress),
) -> anyhow::Result<ImportProgress> {
    let total = tasks.len();
    let bytes_total: u64 = tasks
        .iter()
        .flat_map(|t| [&t.still, &t.movie])
        .flatten()
        .map(|f| f.size)
        .sum();

    let mut progress = ImportProgress {
        total,
        bytes_total,
        ..Default::default()
    };

    for task in tasks {
        if should_cancel() {
            progress.cancelled = true;
            break;
        }
        progress.current = task.base_name.clone();

        let dir = dest_dir;

        let mut names = Vec::new();
        if let Some(s) = &task.still {
            names.push(s.name.clone());
        }
        if let Some(m) = &task.movie {
            names.push(m.name.clone());
        }
        let uniq = unique_names(dir, &names);
        // 落盘主名（可能带 `_N` 后缀）才是数据库键；设备原名另存 `src_name` 用于去重。
        let db_base = uniq
            .first()
            .map(|n| crate::pairing::base_name(n).to_string())
            .unwrap_or_else(|| task.base_name.clone());

        let mut next = 0usize;
        let still_dest = if task.still.is_some() {
            let d = dir.join(&uniq[next]);
            next += 1;
            Some(d)
        } else {
            None
        };
        let movie_dest = if task.movie.is_some() {
            Some(dir.join(&uniq[next]))
        } else {
            None
        };

        let asset = PairedAsset {
            base_name: db_base.clone(),
            kind: match (&task.still, &task.movie) {
                (Some(_), Some(_)) => KIND_LIVE,
                (Some(_), None) => KIND_PHOTO,
                (None, Some(_)) => KIND_VIDEO,
                (None, None) => KIND_PHOTO,
            },
            integrity: match (&task.still, &task.movie) {
                (Some(_), Some(_)) => INTEGRITY_OK,
                (Some(_), None) => INTEGRITY_STILL_ONLY,
                (None, Some(_)) => INTEGRITY_VIDEO_ONLY,
                (None, None) => INTEGRITY_STILL_ONLY,
            },
            still: task.still.as_ref().map(|f| FileRef {
                path: still_dest
                    .as_ref()
                    .map(|d| d.to_string_lossy().to_string())
                    .unwrap_or_default(),
                ext: crate::pairing::extension(&f.name),
                size: f.size,
            }),
            movie: task.movie.as_ref().map(|f| FileRef {
                path: movie_dest
                    .as_ref()
                    .map(|d| d.to_string_lossy().to_string())
                    .unwrap_or_default(),
                ext: crate::pairing::extension(&f.name),
                size: f.size,
            }),
        };

        // 先落一行并显式标 pending（也顺带清掉上次失败留下的 error），失败也不会丢条目。
        crate::db::upsert_imported_asset(
            conn,
            IMPORT_DIR,
            &asset,
            task.taken_at,
            src_serial,
            &task.base_name,
        )?;
        crate::db::set_asset_status(conn, IMPORT_DIR, &db_base, STATUS_PENDING, None)?;

        let mut error: Option<String> = None;
        for (f, dest) in [
            (task.still.as_ref(), still_dest.as_ref()),
            (task.movie.as_ref(), movie_dest.as_ref()),
        ] {
            let (Some(f), Some(dest)) = (f, dest) else {
                continue;
            };
            // 先写 .part，成功后改名，避免中断留下"看似完整"的损坏文件。
            let mut part = dest.clone().into_os_string();
            part.push(".part");
            let part = PathBuf::from(part);

            match transfer.fetch(&f.object_id, &part) {
                Ok(written) => {
                    // 防御：写入字节数必须与设备报称大小一致。实测遇到过"GetStream 成功但读到 0 字节"
                    // 的情况；不校验就会把空/损坏文件当成已导入。
                    if written != f.size {
                        let _ = std::fs::remove_file(&part);
                        error = Some(format!(
                            "{}: 大小不符（预期 {} 实得 {}）",
                            f.name, f.size, written
                        ));
                        break;
                    }
                    if let Err(e) = std::fs::rename(&part, dest) {
                        let _ = std::fs::remove_file(&part);
                        error = Some(format!("{}: 改名失败: {e}", f.name));
                        break;
                    }
                    progress.bytes_done += written;
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&part);
                    error = Some(format!("{}: {e:#}", f.name));
                    break;
                }
            }
        }

        match error {
            None => {
                crate::db::set_asset_status(conn, IMPORT_DIR, &db_base, STATUS_COPIED, None)?;

                // 读两侧 ContentIdentifier 并判定 integrity（设计 §10.1）。
                let still_id = asset.still.as_ref().and_then(|f| {
                    std::fs::read(&f.path)
                        .ok()
                        .and_then(|b| crate::livephoto::content_id_from_still(&b))
                });
                let movie_id = match (&asset.movie, ffprobe_bin) {
                    (Some(f), Some(probe)) => crate::ffmpeg::run_capture(
                        probe,
                        &crate::ffmpeg::movie_content_id_args(Path::new(&f.path)),
                    )
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
                    cloud_hint,
                );
                crate::db::set_content_id(
                    conn,
                    IMPORT_DIR,
                    &db_base,
                    still_id.as_deref().or(movie_id.as_deref()),
                )?;
                crate::db::set_integrity(conn, IMPORT_DIR, &db_base, integrity)?;

                // 缩略图：失败不把条目判为失败（文件已完好），只保留 copied 并记录原因。
                let source = still_dest.as_ref().or(movie_dest.as_ref());
                if let Some(src) = source {
                    match thumbs.make(src, thumbs_dir) {
                        Ok(tp) => {
                            crate::db::set_thumb_path(
                                conn,
                                IMPORT_DIR,
                                &db_base,
                                &tp.to_string_lossy(),
                            )?;
                            crate::db::set_asset_status(
                                conn,
                                IMPORT_DIR,
                                &db_base,
                                STATUS_TRANSCODED,
                                None,
                            )?;
                        }
                        Err(e) => {
                            crate::db::set_asset_status(
                                conn,
                                IMPORT_DIR,
                                &db_base,
                                STATUS_COPIED,
                                Some(&format!("缩略图失败: {e:#}")),
                            )?;
                        }
                    }
                }
                progress.done += 1;
            }
            Some(err) => {
                crate::db::set_asset_status(conn, IMPORT_DIR, &db_base, STATUS_FAILED, Some(&err))?;
                progress.failed += 1;
            }
        }

        on_progress(&progress);
    }

    Ok(progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::cell::RefCell;

    #[test]
    fn unique_names_adds_suffix_to_both_files() {
        let dir = std::env::temp_dir().join("lpm_names_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("IMG_0001.HEIC"), b"x").unwrap();

        let names = vec!["IMG_0001.HEIC".to_string(), "IMG_0001.MOV".to_string()];
        let out = unique_names(&dir, &names);
        assert_eq!(out, vec!["IMG_0001_1.HEIC", "IMG_0001_1.MOV"]);
    }

    fn df(name: &str, size: u64, taken: Option<i64>) -> DeviceFile {
        DeviceFile {
            object_id: name.to_string(),
            name: name.to_string(),
            size,
            taken_at: taken,
        }
    }

    #[test]
    fn diff_includes_new_and_excludes_same_size() {
        let files = vec![
            df("IMG_0001.JPG", 100, Some(1000)),
            df("IMG_0001.MOV", 200, Some(1000)),
            df("IMG_0002.JPG", 50, Some(2000)),
        ];
        // 空的索引 → 全部要传
        assert_eq!(diff_tasks(&files, &ImportedSizes::new(), "SN1").len(), 2);

        // 0001 已从该设备导入且大小一致 → 排除；0002 大小不一致 → 保留
        let mut existing = ImportedSizes::new();
        existing.insert(("SN1".into(), "IMG_0001".into()), (Some(100), Some(200)));
        existing.insert(("SN1".into(), "IMG_0002".into()), (Some(999), None));
        let tasks = diff_tasks(&files, &existing, "SN1");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].base_name, "IMG_0002");
    }

    struct FakeTransfer {
        calls: RefCell<Vec<String>>,
        fail_on: Option<String>,
    }

    impl Transfer for FakeTransfer {
        fn fetch(&mut self, object_id: &str, dest: &Path) -> anyhow::Result<u64> {
            self.calls.borrow_mut().push(object_id.to_string());
            if self.fail_on.as_deref() == Some(object_id) {
                anyhow::bail!("模拟失败");
            }
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(dest, b"data").unwrap();
            Ok(4)
        }
    }

    struct FakeThumbs {
        fail: bool,
    }

    impl Thumbs for FakeThumbs {
        fn make(&mut self, _source: &Path, thumbs_dir: &Path) -> anyhow::Result<PathBuf> {
            if self.fail {
                anyhow::bail!("模拟缩略图失败");
            }
            std::fs::create_dir_all(thumbs_dir).unwrap();
            let p = thumbs_dir.join("fake.webp");
            std::fs::write(&p, b"x").unwrap();
            Ok(p)
        }
    }

    #[test]
    fn run_tasks_marks_status_and_survives_failure() {
        let conn = db::open_in_memory().unwrap();

        let root = std::env::temp_dir().join("lpm_run_tasks_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let tasks = vec![
            TransferTask {
                base_name: "IMG_0001".into(),
                taken_at: 1000,
                still: Some(df("IMG_0001.JPG", 4, Some(1000))),
                movie: Some(df("IMG_0001.MOV", 4, Some(1000))),
            },
            TransferTask {
                base_name: "IMG_0002".into(),
                taken_at: 2000,
                still: Some(df("IMG_0002.JPG", 4, Some(2000))),
                movie: None,
            },
        ];

        let mut fake = FakeTransfer {
            calls: RefCell::new(Vec::new()),
            fail_on: Some("IMG_0002.JPG".into()),
        };
        let mut thumbs = FakeThumbs { fail: false };

        let p = run_tasks(
            &tasks,
            &root,
            &root.join("thumbs"),
            "SN1",
            &conn,
            &mut fake,
            &mut thumbs,
            None,
            false,
            &|| false,
            |_| {},
        )
        .unwrap();
        assert_eq!(p.total, 2);
        assert_eq!(p.done, 1);
        assert_eq!(p.failed, 1);

        assert_eq!(fake.calls.borrow().len(), 3, "每个文件都应被 fetch 一次");

        let statuses: Vec<(String, i64)> = conn
            .prepare("SELECT base_name, status FROM asset ORDER BY base_name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            statuses,
            vec![
                // 缩略图成功后推进到 transcoded
                ("IMG_0001".to_string(), STATUS_TRANSCODED),
                ("IMG_0002".to_string(), STATUS_FAILED)
            ]
        );

        // 失败条目必须带 error
        let err: Option<String> = conn
            .query_row(
                "SELECT error FROM asset WHERE base_name='IMG_0002'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(err.unwrap().contains("模拟失败"));

        // 文件应平铺写在库根（dir=""）。
        let dir: String = conn
            .query_row(
                "SELECT dir FROM asset WHERE base_name='IMG_0001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dir, "");
    }

    #[test]
    fn thumbnail_failure_keeps_copied_status() {
        let conn = db::open_in_memory().unwrap();

        let root = std::env::temp_dir().join("lpm_thumb_fail_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let tasks = vec![TransferTask {
            base_name: "IMG_0009".into(),
            taken_at: 1000,
            still: Some(df("IMG_0009.JPG", 4, Some(1000))),
            movie: None,
        }];

        let mut fake = FakeTransfer {
            calls: RefCell::new(Vec::new()),
            fail_on: None,
        };
        let mut thumbs = FakeThumbs { fail: true };

        run_tasks(
            &tasks,
            &root,
            &root.join("thumbs"),
            "SN1",
            &conn,
            &mut fake,
            &mut thumbs,
            None,
            false,
            &|| false,
            |_| {},
        )
        .unwrap();

        let (status, err): (i64, Option<String>) = conn
            .query_row("SELECT status, error FROM asset", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(status, STATUS_COPIED, "缩略图失败不应把条目判为导入失败");
        assert!(err.unwrap().contains("缩略图失败"));
    }

    #[test]
    fn backfill_generates_missing_thumbnails() {
        let conn = db::open_in_memory().unwrap();

        let root = std::env::temp_dir().join("lpm_backfill_test");
        let _ = std::fs::remove_dir_all(&root);
        let lib = crate::library::Library::open(&root).unwrap();

        let src = lib.root().join("IMG_1.HEIC");
        std::fs::write(&src, b"fake").unwrap();

        let a = PairedAsset {
            base_name: "IMG_1".into(),
            kind: KIND_PHOTO,
            integrity: INTEGRITY_STILL_ONLY,
            still: Some(FileRef {
                path: src.to_string_lossy().to_string(),
                ext: "heic".into(),
                size: 4,
            }),
            movie: None,
        };
        db::upsert_asset(&conn, "", &a, 0).unwrap();
        db::set_asset_status(&conn, "", "IMG_1", STATUS_COPIED, None).unwrap();

        let mut thumbs = FakeThumbs { fail: false };
        let s = backfill_thumbs(&lib, &conn, &mut thumbs, &|| false, |_| {}).unwrap();
        assert_eq!(s.total, 1);
        assert_eq!(s.done, 1);

        let (tp, status): (Option<String>, i64) = conn
            .query_row("SELECT thumb_path, status FROM asset", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert!(tp.unwrap().contains("fake.webp"));
        assert_eq!(status, STATUS_TRANSCODED);

        // 第二次没有可回填的条目
        let s2 = backfill_thumbs(&lib, &conn, &mut thumbs, &|| false, |_| {}).unwrap();
        assert_eq!(s2.total, 0);
    }
}
