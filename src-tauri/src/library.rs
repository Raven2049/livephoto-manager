use std::path::{Path, PathBuf};

/// 一个库 = 用户指定的一个目录，该目录**就是**照片目录（可含用户自己的子目录）。
/// 应用不在其中创建任何可见子目录；全部派生数据都放在隐藏的 `.lpm/` 内。
pub struct Library {
    root: PathBuf,
}

/// 隐藏的元数据目录（含索引与全部派生缓存）。
pub const META_DIR: &str = ".lpm";
pub const THUMBS: &str = "thumbs";
pub const PREVIEWS: &str = "previews";
/// 单列浏览用的高分大图缓存（按需生成）。
pub const LARGES: &str = "larges";
/// 单张查看的临时图目录（在 `.lpm` 下，属可随时删除的缓存）。
pub const VIEW_TMP: &str = "view";
pub const DB_FILE: &str = "index.db";

impl Library {
    /// 打开一个库；只在其中确保隐藏的 `.lpm/` 结构存在。
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let lib = Library { root: root.into() };
        lib.ensure_meta()?;
        Ok(lib)
    }

    /// 库根——照片就放在这里（`lpm://` 协议允许根的递归）。
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn meta_dir(&self) -> PathBuf {
        self.root.join(META_DIR)
    }
    pub fn db_path(&self) -> PathBuf {
        self.meta_dir().join(DB_FILE)
    }
    pub fn thumbs_dir(&self) -> PathBuf {
        self.meta_dir().join(THUMBS)
    }
    pub fn previews_dir(&self) -> PathBuf {
        self.meta_dir().join(PREVIEWS)
    }
    pub fn larges_dir(&self) -> PathBuf {
        self.meta_dir().join(LARGES)
    }
    pub fn view_tmp_dir(&self) -> PathBuf {
        self.meta_dir().join(VIEW_TMP)
    }

    fn ensure_meta(&self) -> std::io::Result<()> {
        let meta = self.meta_dir();
        std::fs::create_dir_all(&meta)?;
        for sub in [THUMBS, PREVIEWS, LARGES, VIEW_TMP] {
            std::fs::create_dir_all(meta.join(sub))?;
        }
        hide_dir(&meta);
        Ok(())
    }
}

/// 尽力把目录标记为 Windows 隐藏；失败不影响功能（只是资源管理器里可见）。
#[cfg(windows)]
fn hide_dir(dir: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_FLAGS_AND_ATTRIBUTES,
        INVALID_FILE_ATTRIBUTES,
    };

    let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
    wide.push(0);
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(wide.as_ptr()));
        if attrs == INVALID_FILE_ATTRIBUTES {
            return;
        }
        let with_hidden = FILE_FLAGS_AND_ATTRIBUTES(attrs | FILE_ATTRIBUTE_HIDDEN.0);
        let _ = SetFileAttributesW(PCWSTR(wide.as_ptr()), with_hidden);
    }
}

#[cfg(not(windows))]
fn hide_dir(_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn open_creates_only_hidden_meta() {
        let root = temp_root("lpm_lib_test");
        std::fs::create_dir_all(&root).unwrap();
        let lib = Library::open(&root).unwrap();

        assert!(lib.meta_dir().is_dir());
        assert!(lib.thumbs_dir().is_dir());
        assert!(lib.previews_dir().is_dir());
        assert!(lib.larges_dir().is_dir());
        assert_eq!(lib.db_path(), root.join(".lpm").join("index.db"));

        // 不得创建任何可见子目录（例如旧的 originals/）。
        let visible: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name() != META_DIR)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(visible.is_empty(), "库根不应出现可见条目: {visible:?}");
    }
}
