use std::path::Path;

use tauri::Emitter;

use crate::device::transfer::download_object;
use crate::device::WpdDevice;
use crate::importer::{self, DeviceFile, ImportProgress, Thumbs, Transfer};
use crate::library::Library;
use crate::state::AppState;

#[tauri::command]
pub fn open_library(path: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .open(std::path::PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_library(
    state: tauri::State<'_, AppState>,
) -> Result<crate::indexer::ScanSummary, String> {
    state
        .with(crate::indexer::scan_library)
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
