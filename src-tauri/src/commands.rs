use std::path::Path;

use tauri::Emitter;

use crate::device::transfer::download_object;
use crate::device::WpdDevice;
use crate::importer::{self, DeviceFile, ImportProgress, Transfer};
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
    fn fetch(&mut self, object_id: &str, dest: &Path) -> anyhow::Result<u64> {
        download_object(self.content, object_id, dest, |_| {})
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
                object_id: f.object_id.clone(),
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
        let emit_target = app.clone();
        let started = std::time::Instant::now();
        let progress = importer::run_tasks(
            &tasks,
            &lib.originals_dir(),
            &info.folder_name,
            device_id,
            &conn,
            &mut transfer,
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
