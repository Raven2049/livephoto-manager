//! ffmpeg 定位、参数构造与执行（子进程）。
//!
//! 设计 §3/§11：用裁剪版静态构建、子进程调用。开发期二进制不提交进仓库，
//! 通过环境变量 `LIVEPORTER_FFMPEG` 或放在主程序同目录 / `resources/` 下。

use std::path::{Path, PathBuf};

/// 定位 `ffmpeg.exe`。顺序：
/// 1. 环境变量 `LIVEPORTER_FFMPEG`；
/// 2. 主程序同目录的 `ffmpeg.exe`；
/// 3. 主程序同目录下 `resources/ffmpeg.exe`。
pub fn find_ffmpeg() -> anyhow::Result<PathBuf> {
    if let Some(p) = std::env::var_os("LIVEPORTER_FFMPEG") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        anyhow::bail!("LIVEPORTER_FFMPEG 指向的文件不存在: {p:?}");
    }

    let exe_dir = std::env::current_exe()?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    for cand in [
        exe_dir.join("ffmpeg.exe"),
        exe_dir.join("resources").join("ffmpeg.exe"),
    ] {
        if cand.is_file() {
            return Ok(cand);
        }
    }
    anyhow::bail!("未找到 ffmpeg.exe（可设 LIVEPORTER_FFMPEG 指定路径）")
}

/// 从静态图生成 512px WebP 缩略图的参数（不含程序名）。
///
/// 用 `-filter_complex` 而非 `-vf`：iPhone 的 HEIC 会暴露多个图像流，ffmpeg 内部会
/// 构建复杂滤镜图，此时再用 `-vf`（简单滤镜）会报
/// "Filtergraph ... was specified for a stream fed from a complex filtergraph"。
pub fn still_thumb_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        "scale=512:-2".into(),
        "-frames:v".into(),
        "1".into(),
        "-c:v".into(),
        "libwebp".into(),
        "-q:v".into(),
        "80".into(),
        "-f".into(),
        "webp".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 从视频取首帧生成缩略图的参数。`-ss` 放在 `-i` 前以快速定位。
pub fn movie_thumb_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-ss".into(),
        "0".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        "scale=512:-2".into(),
        "-frames:v".into(),
        "1".into(),
        "-c:v".into(),
        "libwebp".into(),
        "-q:v".into(),
        "80".into(),
        "-f".into(),
        "webp".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 执行 ffmpeg，失败时带上 stderr。
pub fn run(ffmpeg: &Path, args: &[String]) -> anyhow::Result<()> {
    let output = std::process::Command::new(ffmpeg).args(args).output()?;
    if !output.status.success() {
        anyhow::bail!(
            "ffmpeg 失败 ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_args_scale_to_512_and_webp() {
        let a = still_thumb_args(Path::new("in.heic"), Path::new("out.webp"));
        assert!(a.contains(&"scale=512:-2".to_string()));
        assert!(a.contains(&"libwebp".to_string()));
        assert_eq!(a.last().unwrap(), "out.webp");
    }

    #[test]
    fn movie_args_seek_before_input() {
        let a = movie_thumb_args(Path::new("in.mov"), Path::new("out.webp"));
        let ss = a.iter().position(|x| x == "-ss").unwrap();
        let i = a.iter().position(|x| x == "-i").unwrap();
        assert!(ss < i, "-ss 应在 -i 之前以快速定位");
    }
}
