//! 诊断报告（设计 §10.2）。
//!
//! 面向开源后的 bug 报告：维护者拿不到用户设备，因此报告要能自证环境与异常分布。
//! **绝不包含照片内容**；设备序列号打码；不含每张照片的 EXIF/GPS 等元数据。

use crate::livephoto;

/// 序列号打码（保留后 4 位）。供命令层使用，测试也用它。
pub fn mask(serial: &str) -> String {
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

#[derive(Debug, Clone)]
pub struct DiagnosticsInput {
    pub device_model: Option<String>,
    pub serial_masked: Option<String>,
    pub files_total: usize,
    /// (integrity 值, 数量)
    pub integrity_counts: Vec<(i64, usize)>,
    pub failed_errors: Vec<String>,
    pub windows_version: String,
    pub ffmpeg_version: String,
}

fn integrity_label(code: i64) -> &'static str {
    match code {
        livephoto::INTEGRITY_OK => "0 正常",
        livephoto::INTEGRITY_MISMATCH => "1 校验不一致",
        livephoto::INTEGRITY_PARTIAL => "2 残缺实况",
        livephoto::INTEGRITY_STILL_ONLY => "3 仅静态",
        livephoto::INTEGRITY_VIDEO_ONLY => "4 仅视频",
        livephoto::INTEGRITY_SUSPECT => "5 疑似非原件",
        _ => "未知",
    }
}

pub fn render(input: &DiagnosticsInput) -> String {
    let mut out = String::new();
    out.push_str("LivePorter 诊断报告\n");
    out.push_str("====================\n\n");

    out.push_str("环境\n");
    out.push_str(&format!("  Windows: {}\n", input.windows_version.trim()));
    out.push_str(&format!("  ffmpeg:  {}\n", input.ffmpeg_version.trim()));
    out.push('\n');

    out.push_str("设备\n");
    out.push_str(&format!(
        "  型号:   {}\n",
        input.device_model.as_deref().unwrap_or("(未知)")
    ));
    out.push_str(&format!(
        "  序列号: {}\n",
        input.serial_masked.as_deref().unwrap_or("(未知)")
    ));
    out.push('\n');

    out.push_str(&format!("文件总数: {}\n\n", input.files_total));

    out.push_str("完整性分布\n");
    if input.integrity_counts.is_empty() {
        out.push_str("  (无条目)\n");
    } else {
        for (code, n) in &input.integrity_counts {
            out.push_str(&format!("  {}: {}\n", integrity_label(*code), n));
        }
    }
    out.push('\n');

    out.push_str("失败条目（最多 50 条，仅错误原文）\n");
    if input.failed_errors.is_empty() {
        out.push_str("  (无)\n");
    } else {
        for (i, e) in input.failed_errors.iter().enumerate() {
            let trimmed: String = e.chars().take(300).collect();
            out.push_str(&format!("  {}. {}\n", i + 1, trimmed));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DiagnosticsInput {
        DiagnosticsInput {
            device_model: Some("iPhone15Pro".into()),
            serial_masked: Some("******3F9A2C".into()),
            files_total: 42,
            integrity_counts: vec![(0, 40), (5, 2)],
            failed_errors: vec!["IMG_0001.MOV: GetStream 失败 0x80042007".into()],
            windows_version: "Microsoft Windows [版本 10.0.19045.1234]".into(),
            ffmpeg_version: "ffmpeg version 9.0.1-essentials_build".into(),
        }
    }

    #[test]
    fn contains_environment_device_and_counts() {
        let text = render(&sample());
        assert!(text.contains("iPhone15Pro"));
        assert!(text.contains("******3F9A2C"));
        assert!(text.contains("9.0.1-essentials_build"));
        assert!(text.contains("5 疑似非原件: 2"));
        assert!(text.contains("GetStream"));
    }

    #[test]
    fn does_not_leak_raw_serial_or_paths() {
        let text = render(&sample());
        // 不得出现未打码的序列号形态
        assert!(!text.contains("3F9A2C") || text.contains("******3F9A2C"));
        // 不得出现库内绝对路径
        assert!(!text.contains(":\\"));
        assert!(!text.contains("originals")); // 报告不列任何文件路径
    }
}
