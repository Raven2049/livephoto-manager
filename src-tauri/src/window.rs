//! 窗口几何持久化（位置 / 大小 / 最大化）。
//!
//! 依据 `launching.md:26`（重启后恢复窗口位置与大小）。存到 **exe 同目录的 `window.json`**，
//! 与本项目「绿色版、不写注册表」一致（同 `recent.json`）。恢复前会把窗口夹回可见显示器，
//! 避免上次所在显示器已断开时窗口跑到屏幕外。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{Monitor, PhysicalPosition, PhysicalSize, Window, WindowEvent};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

fn state_file() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("window.json"))
}

fn load() -> Option<WindowState> {
    let f = state_file()?;
    let text = std::fs::read_to_string(f).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_state(st: &WindowState) {
    let Some(f) = state_file() else { return };
    if let Ok(text) = serde_json::to_string_pretty(st) {
        let _ = std::fs::write(f, text);
    }
}

/// 启动时恢复窗口几何（在窗口已按配置创建后调用）。
pub fn restore(win: &tauri::WebviewWindow) {
    let Some(st) = load() else { return };
    let monitors = win.available_monitors().unwrap_or_default();
    let primary = win.primary_monitor().ok().flatten();
    let fixed = clamp(st, &monitors, primary.as_ref());

    let _ = win.set_size(PhysicalSize::new(fixed.width, fixed.height));
    let _ = win.set_position(PhysicalPosition::new(fixed.x, fixed.y));
    if st.maximized {
        let _ = win.maximize();
    }
}

/// 窗口事件处理：移动/缩放节流保存，关闭时保存。
pub fn on_event(win: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => throttled_save(win),
        WindowEvent::CloseRequested { .. } => save(win),
        _ => {}
    }
}

/// 移动/缩放会高频触发；最多每 ~500ms 落盘一次，关闭时再补一次最终值。
fn throttled_save(win: &Window) {
    static LAST_MS: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last = LAST_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 500 {
        return;
    }
    LAST_MS.store(now, Ordering::Relaxed);
    save(win);
}

/// 保存当前几何。最大化时保留之前记录的「正常」尺寸/位置，只更新最大化标志，
/// 这样取消最大化能回到原位。
pub fn save(win: &Window) {
    let maximized = win.is_maximized().unwrap_or(false);
    if maximized {
        let mut st = load().unwrap_or_default();
        st.maximized = true;
        write_state(&st);
        return;
    }
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else {
        return;
    };
    write_state(&WindowState {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        maximized: false,
    });
}

/// 夹取到可见显示器内：与任一显示器相交面积足够则保留（尺寸不超过该显示器）；
/// 否则把保存的尺寸居中放到主显示器（或第一个显示器）。
fn clamp(st: WindowState, monitors: &[Monitor], primary: Option<&Monitor>) -> WindowState {
    if monitors.is_empty() {
        return st;
    }
    const MIN_VISIBLE: i64 = 100 * 100;

    let mut best: Option<(i64, &Monitor)> = None;
    for m in monitors {
        let area = intersect_area(&st, m);
        if best.is_none_or(|(a, _)| area > a) {
            best = Some((area, m));
        }
    }
    if let Some((area, m)) = best {
        if area >= MIN_VISIBLE {
            let ms = m.size();
            return WindowState {
                x: st.x,
                y: st.y,
                width: st.width.min(ms.width),
                height: st.height.min(ms.height),
                maximized: st.maximized,
            };
        }
    }

    // 完全在屏幕外（或显示器变了）：居中到主显示器。
    let m = primary.or_else(|| monitors.first()).expect("monitors 非空");
    let mp = m.position();
    let ms = m.size();
    let width = st.width.min(ms.width);
    let height = st.height.min(ms.height);
    WindowState {
        x: mp.x + ((ms.width as i64 - width as i64) / 2) as i32,
        y: mp.y + ((ms.height as i64 - height as i64) / 2) as i32,
        width,
        height,
        maximized: st.maximized,
    }
}

fn intersect_area(st: &WindowState, m: &Monitor) -> i64 {
    let mp = m.position();
    let ms = m.size();
    let mx2 = mp.x as i64 + ms.width as i64;
    let my2 = mp.y as i64 + ms.height as i64;
    let wx2 = st.x as i64 + st.width as i64;
    let wy2 = st.y as i64 + st.height as i64;
    let ix = (mx2.min(wx2) - (mp.x as i64).max(st.x as i64)).max(0);
    let iy = (my2.min(wy2) - (mp.y as i64).max(st.y as i64)).max(0);
    ix * iy
}
