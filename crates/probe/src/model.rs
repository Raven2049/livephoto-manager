use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    Image,
    Video,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub object_id: String,
    pub name: String,
    pub size: u64,
    pub date_created: Option<String>,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    pub object_id: String,
    pub name: String,
    pub capacity: Option<u64>,
    pub free_space: Option<u64>,
}

/// 序列号打码：保留后 4 位，其余用 * 替代。诊断报告用。
pub fn mask_serial(serial: &str) -> String {
    let chars: Vec<char> = serial.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len());
    }
    let keep = chars.len() - 4;
    format!(
        "{}{}",
        "*".repeat(keep),
        chars[keep..].iter().collect::<String>()
    )
}

impl RemoteFile {
    /// 主名：去掉最后一个扩展名。`.` 开头且无其他点号的视为无扩展名。
    pub fn base_name(&self) -> &str {
        match self.name.rfind('.') {
            Some(i) if i > 0 => &self.name[..i],
            _ => &self.name,
        }
    }

    /// 扩展名，小写，不含点。无扩展名返回空串。
    pub fn extension(&self) -> String {
        match self.name.rfind('.') {
            Some(i) if i > 0 => self.name[i + 1..].to_ascii_lowercase(),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str) -> RemoteFile {
        RemoteFile {
            object_id: "o1".into(),
            name: name.into(),
            size: 1,
            date_created: None,
            kind: FileKind::Image,
        }
    }

    #[test]
    fn base_name_strips_extension() {
        assert_eq!(f("IMG_1234.HEIC").base_name(), "IMG_1234");
    }

    #[test]
    fn base_name_handles_name_without_extension() {
        assert_eq!(f("NOEXT").base_name(), "NOEXT");
    }

    #[test]
    fn base_name_handles_leading_dot() {
        assert_eq!(f(".hidden").base_name(), ".hidden");
    }

    #[test]
    fn extension_is_lowercased() {
        assert_eq!(f("IMG_1234.HEIC").extension(), "heic");
        assert_eq!(f("IMG_1234.MOV").extension(), "mov");
    }

    #[test]
    fn extension_empty_when_absent() {
        assert_eq!(f("NOEXT").extension(), "");
    }
}

#[cfg(test)]
mod mask_tests {
    use super::mask_serial;

    #[test]
    fn masks_all_but_last_four() {
        assert_eq!(mask_serial("F17ABC3F9A2C"), "********9A2C");
    }

    #[test]
    fn masks_short_serial_entirely() {
        assert_eq!(mask_serial("AB"), "**");
        assert_eq!(mask_serial(""), "");
    }
}
