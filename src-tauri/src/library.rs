use std::path::{Path, PathBuf};

/// 一个库 = 用户指定的一个目录。应用不维护第二份副本。
pub struct Library {
    root: PathBuf,
}

pub const ORIGINALS: &str = "originals";
pub const THUMBS: &str = "thumbs";
pub const PREVIEWS: &str = "previews";
/// 高分大图缓存（单列浏览用，按需生成）。
pub const LARGES: &str = "larges";
pub const META_DIR: &str = ".lpm";
pub const DB_FILE: &str = "index.db";

impl Library {
    /// 打开（不存在则创建）一个库，并确保目录结构齐全。
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let lib = Library { root: root.into() };
        lib.ensure_structure()?;
        Ok(lib)
    }

    /// `lpm://` 协议的允许根 = 库的根目录。
    /// 路径越界防护在 `protocol::resolve_allowed` 里统一做。
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn originals_dir(&self) -> PathBuf {
        self.root.join(ORIGINALS)
    }
    pub fn thumbs_dir(&self) -> PathBuf {
        self.root.join(THUMBS)
    }
    pub fn previews_dir(&self) -> PathBuf {
        self.root.join(PREVIEWS)
    }
    pub fn larges_dir(&self) -> PathBuf {
        self.root.join(LARGES)
    }
    pub fn meta_dir(&self) -> PathBuf {
        self.root.join(META_DIR)
    }
    pub fn db_path(&self) -> PathBuf {
        self.meta_dir().join(DB_FILE)
    }

    fn ensure_structure(&self) -> std::io::Result<()> {
        for dir in [
            self.originals_dir(),
            self.thumbs_dir(),
            self.previews_dir(),
            self.larges_dir(),
            self.meta_dir(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn open_creates_structure() {
        let root = temp_root("lpm_lib_test");
        let lib = Library::open(&root).unwrap();
        assert!(lib.originals_dir().is_dir());
        assert!(lib.thumbs_dir().is_dir());
        assert!(lib.previews_dir().is_dir());
        assert!(lib.meta_dir().is_dir());
        assert_eq!(lib.db_path(), root.join(".lpm").join("index.db"));
    }
}
