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
