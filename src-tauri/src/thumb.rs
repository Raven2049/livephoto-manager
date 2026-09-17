//! 缩略图命名与生成。
//!
//! 命名用「内容派生的廉价哈希」：`文件大小 + 前 64KiB` 的 SHA-256 前 24 个十六进制字符。
//! 不读全文件（视频可能很大），但内容一变哈希必变；同一内容天然去重。

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::ffmpeg;

pub fn content_hash(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).with_context(|| format!("打开 {path:?} 失败"))?;
    let size = f.metadata()?.len();
    let mut head = vec![0u8; 64 * 1024];
    let n = f.read(&mut head)?;

    let mut h = Sha256::new();
    h.update(size.to_le_bytes());
    h.update(&head[..n]);
    let digest = h.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok(hex[..24].to_string())
}

/// 缩略图文件名：`<hash>.webp`。
pub fn thumb_file_name(hash: &str) -> String {
    format!("{hash}.webp")
}

fn make(ffmpeg_bin: &Path, input: &Path, thumbs_dir: &Path, movie: bool) -> Result<PathBuf> {
    std::fs::create_dir_all(thumbs_dir)?;
    let out = thumbs_dir.join(thumb_file_name(&content_hash(input)?));
    // 已存在（同一内容）则直接复用，天然去重。
    if out.is_file() {
        return Ok(out);
    }
    let mut tmp = out.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);

    let args = if movie {
        ffmpeg::movie_thumb_args(input, &tmp)
    } else {
        ffmpeg::still_thumb_args(input, &tmp)
    };
    if let Err(e) = ffmpeg::run(ffmpeg_bin, &args) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &out)?;
    Ok(out)
}

/// 为静态图生成缩略图。
pub fn make_thumb_for_still(ffmpeg_bin: &Path, input: &Path, thumbs_dir: &Path) -> Result<PathBuf> {
    make(ffmpeg_bin, input, thumbs_dir, false)
}

/// 为视频取首帧生成缩略图。
pub fn make_thumb_for_movie(ffmpeg_bin: &Path, input: &Path, thumbs_dir: &Path) -> Result<PathBuf> {
    make(ffmpeg_bin, input, thumbs_dir, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_content_sensitive() {
        let dir = std::env::temp_dir().join("lpm_thumb_hash");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        std::fs::write(&a, b"hello").unwrap();
        std::fs::write(&b, b"hello").unwrap();
        assert_eq!(content_hash(&a).unwrap(), content_hash(&b).unwrap());

        std::fs::write(&b, b"hellp").unwrap();
        assert_ne!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn thumb_name_has_webp_extension() {
        assert_eq!(thumb_file_name("abc123"), "abc123.webp");
    }

    /// 需要本机有 ffmpeg：设 LIVEPORTER_FFMPEG 与 LPM_THUMB_INPUT 后跑
    ///   cargo test -p liveporter real_thumb_smoke -- --ignored --nocapture
    #[test]
    #[ignore = "requires ffmpeg"]
    fn real_thumb_smoke() {
        let bin = ffmpeg::find_ffmpeg().unwrap();
        let input =
            std::env::var("LPM_THUMB_INPUT").expect("设 LPM_THUMB_INPUT 指向一个 HEIC/JPG/MOV");
        let out_dir = std::env::temp_dir().join("lpm_thumb_smoke");
        let _ = std::fs::remove_dir_all(&out_dir);

        let p = if input.to_ascii_lowercase().ends_with(".mov") {
            make_thumb_for_movie(&bin, Path::new(&input), &out_dir).unwrap()
        } else {
            make_thumb_for_still(&bin, Path::new(&input), &out_dir).unwrap()
        };
        println!(
            "thumb = {p:?} ({} bytes)",
            std::fs::metadata(&p).unwrap().len()
        );
        assert!(std::fs::metadata(&p).unwrap().len() > 0);
    }
}
