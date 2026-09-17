use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::library::Library;

/// 全局状态：当前打开的库 + 它的索引连接。
/// 未打开库时 `inner` 为 None，库相关命令返回明确错误，`lpm://` 一律 403。
#[derive(Default)]
pub struct AppState {
    inner: Mutex<Option<Open>>,
}

struct Open {
    library: Library,
    conn: Connection,
}

impl AppState {
    pub fn open(&self, root: PathBuf) -> anyhow::Result<()> {
        let library = Library::open(root)?;
        let conn = crate::db::open(&library.db_path())?;
        *self.inner.lock().expect("poisoned") = Some(Open { library, conn });
        Ok(())
    }

    /// 在库打开时对其执行一段操作；未打开返回 None。
    pub fn with<T>(&self, f: impl FnOnce(&Library, &Connection) -> T) -> Option<T> {
        let guard = self.inner.lock().expect("poisoned");
        guard.as_ref().map(|o| f(&o.library, &o.conn))
    }

    /// `lpm://` 协议的允许根 = 当前库的根目录。未打开库时返回 None。
    pub fn library_root(&self) -> Option<PathBuf> {
        self.with(|lib, _| lib.root().to_path_buf())
    }
}
