mod commands;
mod db;
mod indexer;
mod library;
mod pairing;
mod protocol;
mod state;

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
            commands::scan_library,
            commands::library_stats,
            commands::list_assets,
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
