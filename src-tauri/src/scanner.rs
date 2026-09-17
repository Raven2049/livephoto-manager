use std::path::{Path, PathBuf};

use serde::Serialize;

const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "heic", "heif",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImageItem {
    pub path: String,
    pub name: String,
    pub size: u64,
}

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 递归收集图片，按路径排序（稳定顺序，虚拟滚动的锚点依赖它）。
pub fn collect_images(root: &Path) -> Vec<ImageItem> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(dir: &Path, out: &mut Vec<ImageItem>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if is_image(&path) {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(ImageItem {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                size,
            });
        }
    }
}

#[tauri::command]
pub async fn scan_dir(
    path: String,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<ImageItem>, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("不是目录: {path}"));
    }
    state.set_allowed_root(root.clone());
    // 大目录遍历放到阻塞线程，别卡住 async 运行时。
    tauri::async_runtime::spawn_blocking(move || collect_images(&root))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_non_images() {
        assert!(is_image(Path::new("a.JPG")));
        assert!(is_image(Path::new("a.heic")));
        assert!(!is_image(Path::new("a.mov")));
        assert!(!is_image(Path::new("a.txt")));
        assert!(!is_image(Path::new("noext")));
    }

    #[test]
    fn collects_and_sorts_recursively() {
        let dir = std::env::temp_dir().join("lpm_scan_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("b.png"), b"b").unwrap();
        std::fs::write(dir.join("sub/a.jpg"), b"aa").unwrap();
        std::fs::write(dir.join("skip.mov"), b"x").unwrap();

        let items = collect_images(&dir);
        assert_eq!(items.len(), 2);
        // 按完整路径升序：同级目录下 b.png < sub\a.jpg
        assert!(items[0].path.ends_with("b.png"));
        assert!(items[1].path.ends_with("a.jpg"));
        assert!(items[1].path.contains("sub"));
    }
}
