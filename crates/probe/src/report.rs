use crate::model::RemoteFile;
use crate::pairing::{Pair, PairStatus};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct ExtensionCount {
    pub extension: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    pub device_name: Option<String>,
    pub device_model: Option<String>,
    pub device_serial_masked: Option<String>,
    pub total_files: usize,
    pub extension_counts: Vec<ExtensionCount>,
    pub paired: usize,
    pub still_only: usize,
    pub video_only: usize,
    /// 判定结论：true 表示 MOV 确实暴露、配对成立
    pub go: bool,
}

pub fn build_report(
    device_name: Option<String>,
    device_model: Option<String>,
    device_serial_masked: Option<String>,
    files: &[RemoteFile],
    pairs: &[Pair],
) -> ProbeReport {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in files {
        *counts.entry(file.extension()).or_insert(0) += 1;
    }

    let paired = pairs
        .iter()
        .filter(|p| p.status == PairStatus::Paired)
        .count();
    let still_only = pairs
        .iter()
        .filter(|p| p.status == PairStatus::StillOnly)
        .count();
    let video_only = pairs
        .iter()
        .filter(|p| p.status == PairStatus::VideoOnly)
        .count();

    ProbeReport {
        device_name,
        device_model,
        device_serial_masked,
        total_files: files.len(),
        extension_counts: counts
            .into_iter()
            .map(|(extension, count)| ExtensionCount { extension, count })
            .collect(),
        paired,
        still_only,
        video_only,
        go: paired > 0,
    }
}

pub fn render_human(report: &ProbeReport) -> String {
    let mut out = String::new();
    out.push_str("== LivePorter 阶段 0 探测报告 ==\n\n");
    out.push_str(&format!(
        "设备: {} / 型号: {}\n",
        report.device_name.as_deref().unwrap_or("(未知)"),
        report.device_model.as_deref().unwrap_or("(未知)")
    ));
    out.push_str(&format!(
        "序列号: {}\n\n",
        report.device_serial_masked.as_deref().unwrap_or("(未知)")
    ));
    out.push_str(&format!("文件总数: {}\n", report.total_files));
    out.push_str("按扩展名统计:\n");
    for e in &report.extension_counts {
        out.push_str(&format!("  {:<8} {}\n", e.extension, e.count));
    }
    out.push_str(&format!(
        "\n成对(实况): {}\n仅静态图: {}\n仅视频: {}\n",
        report.paired, report.still_only, report.video_only
    ));
    out.push_str("\n== 结论 ==\n");
    if report.go {
        out.push_str("通道可行：MOV 已暴露且能按主名配对。可以继续。\n");
    } else if report.extension_counts.iter().all(|e| e.extension != "mov") {
        out.push_str("未发现任何 .MOV 文件——实况照片的动态部分未通过 MTP 暴露。\n");
        out.push_str("结论：USB 直连通道不可行，需要改用本地备份解析或 iCloud API。\n");
    } else {
        out.push_str("发现了 .MOV 但配对数为 0——文件命名规则与预期不符。\n");
        out.push_str("下一步：抄录前 20 个文件名，重新设计配对策略。\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileKind;

    fn f(name: &str, kind: FileKind) -> RemoteFile {
        RemoteFile {
            object_id: name.into(),
            name: name.into(),
            size: 100,
            date_created: None,
            kind,
        }
    }

    fn sample() -> (Vec<RemoteFile>, Vec<Pair>) {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
            f("IMG_0002.HEIC", FileKind::Image),
            f("IMG_0003.MOV", FileKind::Video),
        ];
        let pairs = crate::pairing::pair(&files);
        (files, pairs)
    }

    #[test]
    fn counts_extensions_case_insensitively() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        let heic = r
            .extension_counts
            .iter()
            .find(|e| e.extension == "heic")
            .unwrap();
        assert_eq!(heic.count, 2);
        let mov = r
            .extension_counts
            .iter()
            .find(|e| e.extension == "mov")
            .unwrap();
        assert_eq!(mov.count, 2);
    }

    #[test]
    fn tallies_pair_statuses() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        assert_eq!(r.paired, 1);
        assert_eq!(r.still_only, 1);
        assert_eq!(r.video_only, 1);
        assert_eq!(r.total_files, 4);
    }

    #[test]
    fn go_is_true_when_any_pair_succeeds() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        assert!(r.go);
    }

    #[test]
    fn go_is_false_when_mov_is_never_exposed() {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0002.HEIC", FileKind::Image),
        ];
        let pairs = crate::pairing::pair(&files);
        let r = build_report(None, None, None, &files, &pairs);
        assert!(!r.go);
    }

    #[test]
    fn human_output_warns_when_no_mov_found() {
        let files = vec![f("IMG_0001.HEIC", FileKind::Image)];
        let pairs = crate::pairing::pair(&files);
        let r = build_report(None, None, None, &files, &pairs);
        let text = render_human(&r);
        assert!(text.contains("未发现任何 .MOV"), "实际输出:\n{text}");
    }

    #[test]
    fn human_output_does_not_warn_when_paired() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        let text = render_human(&r);
        assert!(!text.contains("未发现任何 .MOV"), "实际输出:\n{text}");
    }
}
