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

/// 从视频生成 480p H.264 预览片的参数（不含程序名）。
/// 无音轨、截取前 6 秒、`faststart` 便于快速起播。
///
/// 与缩略图一样用 `-filter_complex`（保持与 HEIC 多流场景一致；短视频单流亦适用）。
pub fn preview_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-t".into(),
        "6".into(),
        "-filter_complex".into(),
        "scale=480:-2".into(),
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-crf".into(),
        "28".into(),
        "-preset".into(),
        "veryfast".into(),
        "-movflags".into(),
        "+faststart".into(),
        "-f".into(),
        "mp4".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 生成单列浏览用的大图：宽度最多 2000px、WebP q90；比原图小就不放大。
pub fn large_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        "scale=w='min(2000,iw)':h=-2".into(),
        "-frames:v".into(),
        "1".into(),
        "-c:v".into(),
        "libwebp".into(),
        "-q:v".into(),
        "90".into(),
        "-f".into(),
        "webp".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 单张查看用的大图：原分辨率、WebP 高质量（不缩放）。
/// 用 libwebp（裁剪构建已启用），不用 mjpeg（未启用）。
pub fn view_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-frames:v".into(),
        "1".into(),
        "-c:v".into(),
        "libwebp".into(),
        "-q:v".into(),
        "95".into(),
        "-f".into(),
        "webp".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 单张查看里播放实况/视频用的代理：H.264、最长边 ≤1920、**静音**（裁剪版无 aac 编码器）。
/// `-faststart` 便于快速起播。
pub fn view_video_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vf".into(),
        "scale=w='min(1920,iw)':h=-2".into(),
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-crf".into(),
        "26".into(),
        "-preset".into(),
        "veryfast".into(),
        "-movflags".into(),
        "+faststart".into(),
        "-f".into(),
        "mp4".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 与 ffmpeg 同目录的 ffprobe。
pub fn find_ffprobe() -> anyhow::Result<PathBuf> {
    let ff = find_ffmpeg()?;
    let probe = ff.with_file_name("ffprobe.exe");
    if probe.is_file() {
        return Ok(probe);
    }
    anyhow::bail!("未找到 ffprobe.exe（应与 ffmpeg.exe 同目录）")
}

/// 读取视频 ContentIdentifier 的 ffprobe 参数。
pub fn movie_content_id_args(input: &Path) -> Vec<String> {
    vec![
        "-v".into(),
        "error".into(),
        "-show_entries".into(),
        "format_tags=com.apple.quicktime.content.identifier".into(),
        "-of".into(),
        "default=nw=1:nk=1".into(),
        input.to_string_lossy().into_owned(),
    ]
}

/// 构造子进程命令。Windows 上加 `CREATE_NO_WINDOW`，否则 GUI 程序
/// （`windows_subsystem = "windows"`）每 spawn 一个 ffmpeg/ffprobe 都会闪一下黑框。
pub(crate) fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 运行 ffprobe 并返回 stdout 文本。
pub fn run_capture(probe: &Path, args: &[String]) -> anyhow::Result<String> {
    let out = command(probe).args(args).output()?;
    if !out.status.success() {
        anyhow::bail!(
            "ffprobe 失败 ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// 执行 ffmpeg，失败时带上 stderr。
pub fn run(ffmpeg: &Path, args: &[String]) -> anyhow::Result<()> {
    let output = command(ffmpeg).args(args).output()?;
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

    #[test]
    fn preview_args_are_480p_h264_muted() {
        let a = preview_args(Path::new("in.mov"), Path::new("out.mp4.part"));
        assert!(a.contains(&"scale=480:-2".to_string()));
        assert!(a.contains(&"libx264".to_string()));
        assert!(a.contains(&"-an".to_string()));
        assert!(a.contains(&"6".to_string()));
        assert_eq!(a.last().unwrap(), "out.mp4.part");
    }

    #[test]
    fn large_args_cap_width_and_encode_webp() {
        let a = large_args(Path::new("in.heic"), Path::new("out.webp.part"));
        assert!(a.iter().any(|x| x.contains("min(2000,iw)")));
        assert!(a.contains(&"libwebp".to_string()));
        assert_eq!(a.last().unwrap(), "out.webp.part");
    }

    #[test]
    fn view_args_are_full_res_webp() {
        let a = view_args(Path::new("in.heic"), Path::new("out.webp.part"));
        assert!(a.contains(&"libwebp".to_string()));
        assert!(a.contains(&"-frames:v".to_string()));
        assert!(!a.iter().any(|x| x.starts_with("scale=")), "查看图不缩放");
        assert_eq!(a.last().unwrap(), "out.webp.part");
    }

    #[test]
    fn view_video_args_are_h264_muted_faststart() {
        let a = view_video_args(Path::new("in.mov"), Path::new("out.mp4.part"));
        assert!(a.contains(&"libx264".to_string()));
        assert!(a.contains(&"-an".to_string()));
        assert!(a.contains(&"+faststart".to_string()));
        assert_eq!(a.last().unwrap(), "out.mp4.part");
    }

    #[test]
    fn movie_id_args_request_only_the_key() {
        let a = movie_content_id_args(Path::new("in.mov"));
        assert!(a
            .iter()
            .any(|x| x.contains("com.apple.quicktime.content.identifier")));
        assert_eq!(a.last().unwrap(), "in.mov");
    }

    #[cfg(windows)]
    #[test]
    fn command_helper_hides_console_and_runs() {
        let out = command("cmd").args(["/c", "echo", "lpm"]).output().unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("lpm"));
    }

    /// 需要本机有 ffmpeg/ffprobe：设 LIVEPORTER_FFMPEG 与 LPM_MOVIE_ID_INPUT 后跑
    ///   cargo test -p liveporter real_movie_content_id_smoke -- --ignored --nocapture
    #[test]
    #[ignore = "requires ffprobe"]
    fn real_movie_content_id_smoke() {
        let probe = find_ffprobe().unwrap();
        let input = std::env::var("LPM_MOVIE_ID_INPUT").expect("设 LPM_MOVIE_ID_INPUT");
        let text = run_capture(&probe, &movie_content_id_args(Path::new(&input))).unwrap();
        println!("content id = {:?}", text.trim());
        assert!(!text.trim().is_empty(), "ffprobe 未输出 content identifier");
    }
}
