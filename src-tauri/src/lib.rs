mod commands;
mod db;
mod delete;
mod device;
mod diagnostics;
mod export;
mod ffmpeg;
mod importer;
mod indexer;
mod library;
mod livephoto;
mod pairing;
mod protocol;
mod state;
mod thumb;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .register_asynchronous_uri_scheme_protocol("lpm", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // 协议处理函数是同步的：把读盘放到受控的阻塞线程池，读完再 respond。
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<state::AppState>();
                let root = state.library_root();
                let response = match root {
                    Some(root) => {
                        let raw = request.uri().path().to_string();
                        match protocol::resolve_allowed(&raw, &root) {
                            Some(path) => {
                                let header = |name: tauri::http::HeaderName| {
                                    request
                                        .headers()
                                        .get(name)
                                        .and_then(|v| v.to_str().ok())
                                        .map(|s| s.to_string())
                                };
                                let range = header(tauri::http::header::RANGE);
                                let if_none_match = header(tauri::http::header::IF_NONE_MATCH);
                                protocol::build_response(
                                    &path,
                                    range.as_deref(),
                                    if_none_match.as_deref(),
                                )
                                .unwrap_or_else(|e| internal_error(&e.to_string()))
                            }
                            None => forbidden(),
                        }
                    }
                    None => forbidden(),
                };
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_library,
            commands::recent_libraries,
            commands::forget_library,
            commands::scan_library,
            commands::library_stats,
            commands::list_assets,
            commands::count_assets,
            commands::list_asset_ids,
            commands::import_from_device,
            commands::generate_thumbs,
            commands::cancel_import,
            commands::cancel_scan,
            commands::ensure_preview,
            commands::ensure_large,
            commands::ensure_view,
            commands::classify_library,
            commands::export_diagnostics,
            commands::export_assets,
            commands::delete_assets,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn forbidden() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(tauri::http::StatusCode::FORBIDDEN)
        .body(Vec::new())
        .expect("static headers are valid")
}

fn internal_error(msg: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(tauri::http::StatusCode::INTERNAL_SERVER_ERROR)
        .header(tauri::http::header::CONTENT_TYPE, "text/plain")
        .body(msg.as_bytes().to_vec())
        .expect("static headers are valid")
}

/// 守卫测试：设备访问目录下不得出现对设备的写操作。
#[cfg(test)]
mod readonly_guard {
    #[test]
    fn device_module_contains_no_write_operations() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/device");
        let mut offenders = Vec::new();
        for entry in walkdir(dir) {
            let text = std::fs::read_to_string(&entry).unwrap();
            for needle in [
                ".Delete(",
                ".Move(",
                ".Copy(",
                "CreateObject",
                "CreateResource",
                "CopyHere",
            ] {
                if text.contains(needle) {
                    offenders.push(format!("{entry}: {needle}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "设备访问必须是只读的，发现写操作: {offenders:?}"
        );
    }

    fn walkdir(dir: &str) -> Vec<String> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walkdir(p.to_str().unwrap()));
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p.to_str().unwrap().to_string());
            }
        }
        out
    }
}
