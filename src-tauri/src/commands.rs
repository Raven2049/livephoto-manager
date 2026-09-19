use std::path::Path;

use tauri::Emitter;

use crate::device::transfer::download_object;
use crate::device::WpdDevice;
use crate::importer::{self, DeviceFile, ImportProgress, Thumbs, Transfer};
use crate::library::Library;
use crate::state::AppState;

/// 最近打开过的资料库（持久化在 exe 同目录的 `recent.json`，不写系统目录）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RecentEntry {
    path: String,
    name: String,
    last_opened: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecentLibrary {
    pub path: String,
    pub name: String,
    pub last_opened: i64,
    /// 目录当前是否还存在（可能被移动/删除）。
    pub exists: bool,
}

fn recent_file() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("recent.json"))
}

fn read_recent() -> Vec<RecentEntry> {
    let Some(f) = recent_file() else {
        return Vec::new();
    };
    std::fs::read_to_string(f)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_recent(list: &[RecentEntry]) {
    let Some(f) = recent_file() else {
        return;
    };
    if let Ok(text) = serde_json::to_string_pretty(list) {
        let _ = std::fs::write(f, text);
    }
}

fn with_exists(list: Vec<RecentEntry>) -> Vec<RecentLibrary> {
    list.into_iter()
        .map(|r| RecentLibrary {
            exists: Path::new(&r.path).is_dir(),
            path: r.path,
            name: r.name,
            last_opened: r.last_opened,
        })
        .collect()
}

fn record_recent(path: &Path, name: &str) {
    let mut list = read_recent();
    list.retain(|r| r.path != path.to_string_lossy());
    list.insert(
        0,
        RecentEntry {
            path: path.to_string_lossy().to_string(),
            name: name.to_string(),
            last_opened: crate::db::now_epoch(),
        },
    );
    list.truncate(8);
    write_recent(&list);
}

#[tauri::command]
pub fn recent_libraries() -> Vec<RecentLibrary> {
    with_exists(read_recent())
}

#[tauri::command]
pub fn forget_library(path: String) -> Vec<RecentLibrary> {
    let mut list = read_recent();
    list.retain(|r| r.path != path);
    write_recent(&list);
    with_exists(list)
}

#[tauri::command]
pub fn open_library(path: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    // 必须是已存在的目录：避免「最近资料库」被移动后误建一个空库。
    if !p.is_dir() {
        return Err("目录不存在或已被移动".to_string());
    }
    state.open(p.clone()).map_err(|e| e.to_string())?;
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    record_recent(&p, &name);
    Ok(())
}

/// 扫描 `originals/` 重建索引。
///
/// **必须异步 + `spawn_blocking`**：大库枚举磁盘可能耗时很久，同步命令会跑在
/// 主线程上，直接冻结窗口（曾实测「未响应」）。这里另开一个 DB 连接，不占用
/// `AppState` 里那个带锁的连接。
#[tauri::command]
pub async fn scan_library(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<crate::indexer::ScanSummary, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;
    state.reset_cancel();
    let cancel = state.cancel_flag();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<crate::indexer::ScanSummary> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let should_cancel = || cancel.load(std::sync::atomic::Ordering::SeqCst);
        let emit_app = app.clone();
        crate::indexer::scan_library_with(&lib, &conn, &should_cancel, |p| {
            let _ = emit_app.emit("scan://progress", p);
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

/// 请求停止当前的重建索引。
#[tauri::command]
pub fn cancel_scan(state: tauri::State<'_, AppState>) {
    state.request_cancel();
}

#[tauri::command]
pub fn library_stats(state: tauri::State<'_, AppState>) -> Result<crate::db::LibraryStats, String> {
    state
        .with(|_, conn| crate::db::stats(conn))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}

/// 分页取条目。异步 + `spawn_blocking`：会读取缩略图文件头拿尺寸（瀑布流用），
/// 不能跑在主线程上。
#[tauri::command]
pub async fn list_assets(
    offset: i64,
    limit: i64,
    filter: Option<crate::db::AssetFilter>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::db::AssetRow>, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;
    let f = filter.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<crate::db::AssetRow>> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let mut rows = crate::db::page_assets_filtered(&conn, &f, offset, limit)?;
        for r in &mut rows {
            if let Some(p) = r.thumb_path.as_deref() {
                if let Some((w, h)) = crate::thumb::read_image_size(Path::new(p)) {
                    r.thumb_w = Some(w as i64);
                    r.thumb_h = Some(h as i64);
                }
            }
        }
        Ok(rows)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn count_assets(
    filter: Option<crate::db::AssetFilter>,
    state: tauri::State<'_, AppState>,
) -> Result<i64, String> {
    let f = filter.unwrap_or_default();
    state
        .with(|_, conn| crate::db::count_assets(conn, &f))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_asset_ids(
    filter: Option<crate::db::AssetFilter>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<i64>, String> {
    let f = filter.unwrap_or_default();
    state
        .with(|_, conn| crate::db::asset_ids(conn, &f))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}

/// 走 WPD 的传输实现：`content` 必须与其 `WpdDevice` 在同一线程（STA）上使用。
struct WpdTransfer<'a> {
    content: &'a windows::Win32::Devices::PortableDevices::IPortableDeviceContent,
}

impl Transfer for WpdTransfer<'_> {
    fn fetch(&mut self, object_ref: &str, dest: &Path) -> anyhow::Result<u64> {
        // object_ref 是持久 ID；iOS 句柄会失效，必须在传输前重解析成新鲜对象 ID。
        let object_id = crate::device::resolve_object_id(self.content, object_ref)
            .map_err(|e| anyhow::anyhow!("重解析对象 ID 失败: {e:#}"))?;
        download_object(self.content, &object_id, dest, |_| {})
    }
}

/// 走 ffmpeg 的缩略图实现。按扩展名决定取静态图还是视频首帧。
struct FfmpegThumbs {
    bin: std::path::PathBuf,
}

impl Thumbs for FfmpegThumbs {
    fn make(&mut self, source: &Path, thumbs_dir: &Path) -> anyhow::Result<std::path::PathBuf> {
        let lower = source.to_string_lossy().to_ascii_lowercase();
        if lower.ends_with(".mov") || lower.ends_with(".mp4") || lower.ends_with(".m4v") {
            crate::thumb::make_thumb_for_movie(&self.bin, source, thumbs_dir)
        } else {
            crate::thumb::make_thumb_for_still(&self.bin, source, thumbs_dir)
        }
    }
}

/// 从 iPhone 增量导入到当前库。
///
/// WPD 调用全部放在一个 `spawn_blocking` 的专用线程上（该线程内初始化 STA），
/// 进度通过 `import://progress` 事件上报；正文图像/视频不走 IPC。
#[tauri::command]
pub async fn import_from_device(
    app: tauri::AppHandle,
    assume_cloud: bool,
    state: tauri::State<'_, AppState>,
) -> Result<ImportProgress, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;
    state.reset_cancel();
    let cancel = state.cancel_flag();

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<ImportProgress> {
        let _com = crate::device::ComGuard::new()?;
        let device = WpdDevice::open_first()?;
        let info = device.info().clone();

        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;

        let serial = info
            .serial
            .clone()
            .unwrap_or_else(|| info.folder_name.clone());
        let model = info.model.clone().unwrap_or_else(|| "unknown".into());
        let device_id = crate::db::upsert_device(
            &conn,
            &serial,
            &model,
            info.friendly_name.as_deref(),
            &info.folder_name,
        )?;

        let files = device.list_media()?;
        let device_files: Vec<DeviceFile> = files
            .iter()
            .map(|f| DeviceFile {
                // 传给管道的是**持久 ID**，由 WPD 传输实现负责重解析成新鲜句柄。
                object_id: f.persistent_id.clone(),
                name: f.name.clone(),
                size: f.size,
                taken_at: f.taken_at,
            })
            .collect();

        let existing = crate::db::existing_sizes(&conn)?;
        let tasks = importer::diff_tasks(&device_files, &existing);

        let mut transfer = WpdTransfer {
            content: device.content(),
        };
        let mut thumbs = FfmpegThumbs {
            bin: crate::ffmpeg::find_ffmpeg()?,
        };
        let should_cancel = || cancel.load(std::sync::atomic::Ordering::SeqCst);
        let ffprobe_bin = crate::ffmpeg::find_ffprobe().ok();
        let emit_target = app.clone();
        let started = std::time::Instant::now();
        let progress = importer::run_tasks(
            &tasks,
            &lib.originals_dir(),
            &lib.thumbs_dir(),
            &info.folder_name,
            device_id,
            &conn,
            &mut transfer,
            &mut thumbs,
            ffprobe_bin.as_deref(),
            assume_cloud,
            &should_cancel,
            |p| {
                if std::env::var_os("LIVEPORTER_IMPORT_LOG").is_some() {
                    let mb = p.bytes_done as f64 / (1024.0 * 1024.0);
                    let secs = started.elapsed().as_secs_f64().max(0.001);
                    eprintln!(
                        "[import] t={:.1}s {}/{} bytes={}/{} ({:.2} MB/s) current={} failed={}",
                        secs,
                        p.done,
                        p.total,
                        p.bytes_done,
                        p.bytes_total,
                        mb / secs,
                        p.current,
                        p.failed
                    );
                }
                let _ = emit_target.emit("import://progress", p);
            },
        )?;

        Ok(progress)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

/// 为当前库里「缺少缩略图」的条目补生成缩略图（回填已有库）。
#[tauri::command]
pub async fn generate_thumbs(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<importer::ThumbSummary, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;
    state.reset_cancel();
    let cancel = state.cancel_flag();

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<importer::ThumbSummary> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let mut thumbs = FfmpegThumbs {
            bin: crate::ffmpeg::find_ffmpeg()?,
        };
        let should_cancel = || cancel.load(std::sync::atomic::Ordering::SeqCst);
        importer::backfill_thumbs(&lib, &conn, &mut thumbs, &should_cancel, |s| {
            let _ = app.emit("thumbs://progress", s);
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

/// 请求停止当前的导入 / 缩略图回填。
#[tauri::command]
pub fn cancel_import(state: tauri::State<'_, AppState>) {
    state.request_cancel();
}

/// 生成诊断报告（设计 §10.2），写到库根 `.lpm/diagnostics-<时间>.txt`，返回路径。
/// 不含照片内容与文件路径；设备序列号打码。
#[tauri::command]
pub async fn export_diagnostics(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<String> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;

        let (model, serial) = match crate::db::first_device(&conn)? {
            Some((m, s)) => (Some(m), Some(s)),
            None => (None, None),
        };
        let stats = crate::db::stats(&conn)?;
        let integrity_counts = stats
            .by_integrity
            .iter()
            .map(|(k, n)| (*k, *n as usize))
            .collect();
        let failed_errors = crate::db::failed_errors(&conn, 50)?;

        let windows_version = crate::ffmpeg::command("cmd")
            .args(["/c", "ver"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let ffmpeg_version = crate::ffmpeg::find_ffmpeg()
            .ok()
            .and_then(|p| crate::ffmpeg::command(p).arg("-version").output().ok())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_default();

        let input = crate::diagnostics::DiagnosticsInput {
            device_model: model,
            serial_masked: serial.as_deref().map(crate::diagnostics::mask),
            files_total: stats.total as usize,
            integrity_counts,
            failed_errors,
            windows_version,
            ffmpeg_version,
        };

        let text = crate::diagnostics::render(&input);
        let path = lib
            .meta_dir()
            .join(format!("diagnostics-{}.txt", crate::db::now_epoch()));
        std::fs::write(&path, text)?;
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportProgress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportSummary {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// 把选中的条目按原样拷贝（配对文件一起）到 `dest` 目录。
/// 只拷贝 `originals/` 里的源文件，不转码、不改索引；重名成对加后缀。
#[tauri::command]
pub async fn export_assets(
    app: tauri::AppHandle,
    ids: Vec<i64>,
    dest: String,
    state: tauri::State<'_, AppState>,
) -> Result<ExportSummary, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;
    state.reset_cancel();
    let cancel = state.cancel_flag();

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<ExportSummary> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let dest_dir = std::path::PathBuf::from(&dest);
        if !dest_dir.is_dir() {
            anyhow::bail!("导出目标不是目录: {dest}");
        }

        let files = crate::db::assets_by_ids(&conn, &ids)?;
        let total = files.len();
        let mut summary = ExportSummary {
            total,
            done: 0,
            failed: 0,
            errors: Vec::new(),
        };

        let mut bytes_total = 0u64;
        for f in &files {
            for p in [&f.still_path, &f.movie_path].into_iter().flatten() {
                if let Ok(m) = std::fs::metadata(p) {
                    bytes_total += m.len();
                }
            }
        }
        let mut bytes_done = 0u64;

        for f in &files {
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let (still_dst, movie_dst) = crate::export::unique_export_paths(
                &dest_dir,
                &f.base_name,
                f.still_ext.as_deref(),
                f.movie_ext.as_deref(),
            );
            let mut ok = true;
            if let (Some(s), Some(d)) = (&f.still_path, &still_dst) {
                match std::fs::copy(s, d) {
                    Ok(n) => bytes_done += n,
                    Err(e) => {
                        ok = false;
                        summary.errors.push(format!("{s}: {e}"));
                    }
                }
            }
            if let (Some(s), Some(d)) = (&f.movie_path, &movie_dst) {
                match std::fs::copy(s, d) {
                    Ok(n) => bytes_done += n,
                    Err(e) => {
                        ok = false;
                        summary.errors.push(format!("{s}: {e}"));
                    }
                }
            }
            if ok {
                summary.done += 1;
            } else {
                summary.failed += 1;
            }
            let p = ExportProgress {
                total,
                done: summary.done + summary.failed,
                failed: summary.failed,
                bytes_done,
                bytes_total,
            };
            let _ = app.emit("export://progress", p);
        }

        summary.errors.truncate(50);
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeleteProgress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeleteSummary {
    pub total: usize,
    pub deleted: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// 把选中条目移入 Windows 回收站，并清理索引与孤儿缓存。
/// 只删本地库文件，不接触设备（设计非目标：不做双向删除）。
#[tauri::command]
pub async fn delete_assets(
    app: tauri::AppHandle,
    ids: Vec<i64>,
    state: tauri::State<'_, AppState>,
) -> Result<DeleteSummary, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;
    state.reset_cancel();
    let cancel = state.cancel_flag();

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<DeleteSummary> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let files = crate::db::assets_by_ids(&conn, &ids)?;
        let total = files.len();
        let mut summary = DeleteSummary {
            total,
            deleted: 0,
            failed: 0,
            errors: Vec::new(),
        };

        let root_path = lib.root().to_path_buf();
        let mut deletable: std::collections::HashSet<i64> = std::collections::HashSet::new();

        for (i, f) in files.iter().enumerate() {
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let mut ok = true;
            for src in crate::delete::source_paths(f) {
                let p = std::path::Path::new(&src);
                if !p.exists() {
                    continue; // 文件已不在，条目仍可删
                }
                if !crate::delete::inside_root(p, &root_path) {
                    ok = false;
                    summary.errors.push(format!("{src}: 不在库目录内，已跳过"));
                    continue;
                }
                if let Err(e) = trash::delete(p) {
                    ok = false;
                    summary.errors.push(format!("{src}: {e}"));
                }
            }
            if ok {
                deletable.insert(f.id);
            } else {
                summary.failed += 1;
            }
            let _ = app.emit(
                "delete://progress",
                DeleteProgress {
                    total,
                    done: i + 1,
                    failed: summary.failed,
                },
            );
        }

        crate::db::delete_assets(&conn, &deletable.iter().copied().collect::<Vec<_>>())?;
        summary.deleted = deletable.len();

        // 清理孤儿缓存：必须在删行之后按剩余引用判断（内容哈希可被多条复用）。
        for f in &files {
            if !deletable.contains(&f.id) {
                continue;
            }
            if let Some(path) = f.thumb_path.as_deref() {
                if crate::delete::cache_is_orphan(crate::db::thumb_ref_count(&conn, path)?) {
                    let _ = std::fs::remove_file(path);
                }
            }
            if let Some(path) = f.preview_path.as_deref() {
                if crate::delete::cache_is_orphan(crate::db::preview_ref_count(&conn, path)?) {
                    let _ = std::fs::remove_file(path);
                }
            }
        }

        summary.errors.truncate(50);
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClassifySummary {
    pub total: usize,
    pub changed: usize,
    pub cancelled: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClassifyProgress {
    pub total: usize,
    pub done: usize,
}

/// 重新读取所有条目的 ContentIdentifier 并重算 integrity（回填已有库）。
#[tauri::command]
pub async fn classify_library(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ClassifySummary, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;
    state.reset_cancel();
    let cancel = state.cancel_flag();

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<ClassifySummary> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let probe = crate::ffmpeg::find_ffprobe().ok();

        let jobs = crate::db::assets_for_classify(&conn)?;
        let total = jobs.len();
        let mut changed = 0usize;
        let mut cancelled = false;
        let mut last_emit = 0usize;

        for job in &jobs {
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            let still_id = job.still_path.as_ref().and_then(|p| {
                std::fs::read(p)
                    .ok()
                    .and_then(|b| crate::livephoto::content_id_from_still(&b))
            });
            let movie_id = match (&job.movie_path, &probe) {
                (Some(p), Some(pr)) => crate::ffmpeg::run_capture(
                    pr,
                    &crate::ffmpeg::movie_content_id_args(std::path::Path::new(p)),
                )
                .ok()
                .and_then(|s| crate::livephoto::clean_id(s.trim().as_bytes())),
                _ => None,
            };
            let integrity = crate::livephoto::classify(
                job.still_path.is_some(),
                job.movie_path.is_some(),
                still_id.as_deref(),
                movie_id.as_deref(),
                job.still_size.unwrap_or(0).max(0) as u64,
                false,
            );
            crate::db::set_content_id(
                &conn,
                job.device_id,
                &job.base_name,
                job.taken_at,
                still_id.as_deref().or(movie_id.as_deref()),
            )?;
            crate::db::set_integrity(
                &conn,
                job.device_id,
                &job.base_name,
                job.taken_at,
                integrity,
            )?;
            changed += 1;
            if changed - last_emit >= 20 {
                last_emit = changed;
                let _ = app.emit(
                    "classify://progress",
                    ClassifyProgress {
                        total,
                        done: changed,
                    },
                );
            }
        }

        Ok(ClassifySummary {
            total,
            changed,
            cancelled,
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

/// 确保某条目的原分辨率查看图存在（单张查看用）；返回其绝对路径。
/// 缓存到 `.lpm/view`（临时、带 LRU），源优先静态图，其次视频首帧。
#[tauri::command]
pub async fn ensure_view(
    asset_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<String> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let files = crate::db::assets_by_ids(&conn, &[asset_id])?;
        let f = files
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("条目不存在"))?;
        let source = f
            .still_path
            .clone()
            .or_else(|| f.movie_path.clone())
            .ok_or_else(|| anyhow::anyhow!("该条目没有可用的源文件"))?;
        let bin = crate::ffmpeg::find_ffmpeg()?;
        let p = crate::thumb::make_view(&bin, Path::new(&source), &lib.view_tmp_dir())?;
        Ok(p.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

/// 确保某条目的高分大图存在（单列浏览用）；返回其绝对路径。
/// 源优先静态图，其次视频首帧；由 ffmpeg 生成 WebP 并缓存。
#[tauri::command]
pub async fn ensure_large(
    asset_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<String> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;
        let files = crate::db::assets_by_ids(&conn, &[asset_id])?;
        let f = files
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("条目不存在"))?;
        let source = f
            .still_path
            .clone()
            .or_else(|| f.movie_path.clone())
            .ok_or_else(|| anyhow::anyhow!("该条目没有可用的源文件"))?;
        let bin = crate::ffmpeg::find_ffmpeg()?;
        let p = crate::thumb::make_large(&bin, Path::new(&source), &lib.larges_dir())?;
        Ok(p.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

/// 确保某条目的预览片存在；返回其绝对路径（前端用 `lpm://` 加载）。
/// 无视频源的条目返回明确错误。
#[tauri::command]
pub async fn ensure_preview(
    asset_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let root = state.library_root().ok_or_else(|| "库未打开".to_string())?;

    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<String> {
        let lib = Library::open(&root)?;
        let conn = crate::db::open(&lib.db_path())?;

        let movie = crate::db::asset_movie_path(&conn, asset_id)?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("该条目没有视频源，无法生成预览"))?;

        let bin = crate::ffmpeg::find_ffmpeg()?;
        let p = crate::thumb::make_preview_for_movie(
            &bin,
            std::path::Path::new(&movie),
            &lib.previews_dir(),
        )?;
        crate::db::set_preview_path(&conn, asset_id, &p.to_string_lossy())?;
        Ok(p.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}
