//! 导出：把选中的逻辑条目按「原样拷贝」写到另一个目录（设计 §9）。
//! 只做文件拷贝，不转码、不改索引。

use std::path::{Path, PathBuf};

/// 在目标目录里找一个不冲突的「主名」，返回配对的静态图/视频目标路径。
///
/// 规则同设计 §5.3：首个可用主名是原名，之后依次 `_1`、`_2`……
/// 配对的两个文件必须用同一个后缀，否则实况配对被拆开。
pub fn unique_export_paths(
    dest_dir: &Path,
    base: &str,
    still_ext: Option<&str>,
    movie_ext: Option<&str>,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut n = 0u32;
    loop {
        let stem = if n == 0 {
            base.to_string()
        } else {
            format!("{base}_{n}")
        };
        let still = still_ext.map(|e| dest_dir.join(format!("{stem}.{e}")));
        let movie = movie_ext.map(|e| dest_dir.join(format!("{stem}.{e}")));
        let free = still.as_ref().is_none_or(|p| !p.exists())
            && movie.as_ref().is_none_or(|p| !p.exists());
        if free || n > 10_000 {
            return (still, movie);
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn uses_base_name_when_free() {
        let dir = temp_dir("lpm_export_t1");
        let (s, m) = unique_export_paths(&dir, "IMG_1", Some("heic"), Some("mov"));
        assert_eq!(s.unwrap().file_name().unwrap(), "IMG_1.heic");
        assert_eq!(m.unwrap().file_name().unwrap(), "IMG_1.mov");
    }

    #[test]
    fn adds_same_suffix_to_both_when_colliding() {
        let dir = temp_dir("lpm_export_t2");
        std::fs::write(dir.join("IMG_1.heic"), b"x").unwrap();
        let (s, m) = unique_export_paths(&dir, "IMG_1", Some("heic"), Some("mov"));
        assert_eq!(s.unwrap().file_name().unwrap(), "IMG_1_1.heic");
        assert_eq!(m.unwrap().file_name().unwrap(), "IMG_1_1.mov");
    }

    #[test]
    fn still_only_has_no_movie_dest() {
        let dir = temp_dir("lpm_export_t3");
        let (s, m) = unique_export_paths(&dir, "IMG_2", Some("jpg"), None);
        assert!(s.is_some());
        assert!(m.is_none());
    }
}
