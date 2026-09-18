//! 从库删除：移入 Windows 回收站并同步清理索引与缓存。
//! 真实的回收站调用放在 ignored 测试里（会动系统回收站），默认测试只测纯逻辑。

use std::path::Path;

use crate::db::AssetFiles;

/// 删除一个条目涉及的文件：静态图 + 视频（实况两半一起）。
pub fn source_paths(a: &AssetFiles) -> Vec<String> {
    [a.still_path.clone(), a.movie_path.clone()]
        .into_iter()
        .flatten()
        .collect()
}

/// 删除后是否可安全移除缓存文件：引用计数为 0。
pub fn cache_is_orphan(ref_count: i64) -> bool {
    ref_count == 0
}

/// 源文件是否位于库根内（防御性检查；正常路径都由本程序写入）。
pub fn inside_root(path: &Path, root: &Path) -> bool {
    let Ok(p) = std::fs::canonicalize(path) else {
        return false;
    };
    let Ok(r) = std::fs::canonicalize(root) else {
        return false;
    };
    p.starts_with(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(still: Option<&str>, movie: Option<&str>) -> AssetFiles {
        AssetFiles {
            id: 1,
            base_name: "IMG_1".into(),
            still_path: still.map(String::from),
            still_ext: still.map(|_| "heic".into()),
            movie_path: movie.map(String::from),
            movie_ext: movie.map(|_| "mov".into()),
            thumb_path: None,
            preview_path: None,
        }
    }

    #[test]
    fn source_paths_includes_both_halves_of_a_live_photo() {
        assert_eq!(
            source_paths(&asset(Some("s.heic"), Some("m.mov"))),
            vec!["s.heic", "m.mov"]
        );
    }

    #[test]
    fn source_paths_handles_still_only() {
        assert_eq!(source_paths(&asset(Some("s.jpg"), None)), vec!["s.jpg"]);
    }

    #[test]
    fn only_unreferenced_cache_is_removed() {
        assert!(cache_is_orphan(0));
        assert!(!cache_is_orphan(1));
    }

    #[test]
    fn inside_root_distinguishes_inside_and_outside() {
        let root = std::env::temp_dir().join("lpm_delete_root");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let inside = root.join("a.bin");
        std::fs::write(&inside, b"x").unwrap();
        let outside = std::env::temp_dir().join("lpm_delete_outside.bin");
        std::fs::write(&outside, b"x").unwrap();

        assert!(inside_root(&inside, &root));
        assert!(!inside_root(&outside, &root));
    }

    #[test]
    #[ignore = "writes to the real recycle bin"]
    fn trash_moves_a_temp_file() {
        let dir = std::env::temp_dir().join("lpm_trash_probe");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("lpm-probe.txt");
        std::fs::write(&f, b"x").unwrap();

        trash::delete(&f).unwrap();
        assert!(!f.exists(), "文件应已从原位置消失");
    }
}
