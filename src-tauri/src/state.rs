use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Default)]
pub struct AppState {
    allowed_root: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn set_allowed_root(&self, path: PathBuf) {
        *self.allowed_root.lock().expect("poisoned") = Some(path);
    }

    pub fn allowed_root(&self) -> Option<PathBuf> {
        self.allowed_root.lock().expect("poisoned").clone()
    }
}
