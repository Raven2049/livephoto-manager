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

/// 读取图片尺寸（仅 WebP）。用于瀑布流布局；只读文件头，不解码。
pub fn read_image_size(path: &Path) -> Option<(u32, u32)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 64];
    let n = f.read(&mut head).ok()?;
    webp_size(&head[..n])
}

/// 解析 WebP 尺寸，支持 VP8（有损）/ VP8L（无损）/ VP8X（扩展）。
pub fn webp_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    let mut i = 12usize;
    while i + 8 <= b.len() {
        let fourcc = &b[i..i + 4];
        let size = u32::from_le_bytes(b[i + 4..i + 8].try_into().ok()?) as usize;
        let start = i + 8;
        let end = start.checked_add(size)?;
        if end > b.len() {
            return None;
        }
        let data = &b[start..end];
        match fourcc {
            b"VP8 " => return vp8_size(data),
            b"VP8L" => return vp8l_size(data),
            b"VP8X" => return vp8x_size(data),
            _ => {}
        }
        // RIFF 块按偶数字节对齐
        i = end + (size & 1);
    }
    None
}

fn vp8_size(d: &[u8]) -> Option<(u32, u32)> {
    if d.len() < 10 || d[3..6] != [0x9d, 0x01, 0x2a] {
        return None;
    }
    let w = u16::from_le_bytes([d[6], d[7]]) & 0x3fff;
    let h = u16::from_le_bytes([d[8], d[9]]) & 0x3fff;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w as u32, h as u32))
}

fn vp8l_size(d: &[u8]) -> Option<(u32, u32)> {
    if d.len() < 5 || d[0] != 0x2f {
        return None;
    }
    let bits = u32::from_le_bytes([d[1], d[2], d[3], d[4]]);
    let w = (bits & 0x3fff) + 1;
    let h = ((bits >> 14) & 0x3fff) + 1;
    Some((w, h))
}

fn vp8x_size(d: &[u8]) -> Option<(u32, u32)> {
    if d.len() < 10 {
        return None;
    }
    let w = 1 + (d[4] as u32 | (d[5] as u32) << 8 | (d[6] as u32) << 16);
    let h = 1 + (d[7] as u32 | (d[8] as u32) << 8 | (d[9] as u32) << 16);
    Some((w, h))
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

/// 单张查看的临时大图文件名：`<hash>.webp`。
pub fn view_file_name(hash: &str) -> String {
    format!("{hash}.webp")
}

/// 单张查看缓存总量上限（超出按最旧清理）。
const VIEW_CACHE_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// 生成/复用「单张查看」用的原分辨率大图（JPEG，临时缓存 + LRU）。
pub fn make_view(ffmpeg_bin: &Path, input: &Path, view_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(view_dir)?;
    let out = view_dir.join(view_file_name(&content_hash(input)?));
    if out.is_file() {
        return Ok(out);
    }
    let mut tmp = out.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);

    if let Err(e) = ffmpeg::run(ffmpeg_bin, &ffmpeg::view_args(input, &tmp)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &out)?;
    prune_view_cache(view_dir, VIEW_CACHE_MAX_BYTES);
    Ok(out)
}

/// 超出上限时删除最旧（按修改时间）的查看图（不改目录结构）。
fn prune_view_cache(dir: &Path, max_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    for e in entries.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        total += meta.len();
        let atime = meta.accessed().or_else(|_| meta.modified()).ok();
        if let Some(atime) = atime {
            files.push((atime, meta.len(), e.path()));
        }
    }
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|(t, _, _)| *t);
    for (_, size, path) in files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

/// 高分大图文件名：`<hash>.webp`（放在 `larges/`）。
pub fn large_file_name(hash: &str) -> String {
    format!("{hash}.webp")
}

/// 生成单列浏览用的大图（按需）。已存在则复用。
pub fn make_large(ffmpeg_bin: &Path, input: &Path, larges_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(larges_dir)?;
    let out = larges_dir.join(large_file_name(&content_hash(input)?));
    if out.is_file() {
        return Ok(out);
    }
    let mut tmp = out.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);

    if let Err(e) = ffmpeg::run(ffmpeg_bin, &ffmpeg::large_args(input, &tmp)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &out)?;
    Ok(out)
}

/// 预览片文件名：`<hash>.mp4`。
pub fn preview_file_name(hash: &str) -> String {
    format!("{hash}.mp4")
}

/// 为视频生成 480p H.264 预览片，返回路径。已存在则直接复用。
pub fn make_preview_for_movie(
    ffmpeg_bin: &Path,
    input: &Path,
    previews_dir: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(previews_dir)?;
    let out = previews_dir.join(preview_file_name(&content_hash(input)?));
    if out.is_file() {
        return Ok(out);
    }
    let mut tmp = out.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);

    if let Err(e) = ffmpeg::run(ffmpeg_bin, &ffmpeg::preview_args(input, &tmp)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(fourcc: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(fourcc);
        c.extend_from_slice(&(data.len() as u32).to_le_bytes());
        c.extend_from_slice(data);
        if data.len() % 2 == 1 {
            c.push(0);
        }
        c
    }
    fn riff(ch: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&((4 + ch.len()) as u32).to_le_bytes());
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(ch);
        v
    }

    #[test]
    fn webp_size_reads_vp8x() {
        let mut data = vec![0u8; 4]; // flags + reserved
        data.extend_from_slice(&511u32.to_le_bytes()[..3]); // width-1 = 511
        data.extend_from_slice(&383u32.to_le_bytes()[..3]); // height-1 = 383
        assert_eq!(webp_size(&riff(&chunk(b"VP8X", &data))), Some((512, 384)));
    }

    #[test]
    fn webp_size_reads_vp8() {
        let mut data = vec![0u8; 3]; // frame tag
        data.extend_from_slice(&[0x9d, 0x01, 0x2a]);
        data.extend_from_slice(&320u16.to_le_bytes());
        data.extend_from_slice(&240u16.to_le_bytes());
        assert_eq!(webp_size(&riff(&chunk(b"VP8 ", &data))), Some((320, 240)));
    }

    #[test]
    fn webp_size_reads_vp8l() {
        let bits: u32 = 199 | (99 << 14);
        let mut data = vec![0x2f];
        data.extend_from_slice(&bits.to_le_bytes());
        assert_eq!(webp_size(&riff(&chunk(b"VP8L", &data))), Some((200, 100)));
    }

    #[test]
    fn webp_size_rejects_non_webp() {
        assert_eq!(webp_size(b"not a webp file"), None);
        assert_eq!(webp_size(b""), None);
    }

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

    #[test]
    fn preview_name_has_mp4_extension() {
        assert_eq!(preview_file_name("abc123"), "abc123.mp4");
    }

    /// 需要本机有 ffmpeg：设 LIVEPORTER_FFMPEG 与 LPM_PREVIEW_INPUT 后跑
    ///   cargo test -p liveporter real_preview_smoke -- --ignored --nocapture
    #[test]
    #[ignore = "requires ffmpeg"]
    fn real_preview_smoke() {
        let bin = ffmpeg::find_ffmpeg().unwrap();
        let input =
            std::env::var("LPM_PREVIEW_INPUT").expect("设 LPM_PREVIEW_INPUT 指向一个 MOV/MP4");
        let out_dir = std::env::temp_dir().join("lpm_preview_smoke");
        let _ = std::fs::remove_dir_all(&out_dir);
        let p = make_preview_for_movie(&bin, Path::new(&input), &out_dir).unwrap();
        println!(
            "preview = {p:?} ({} bytes)",
            std::fs::metadata(&p).unwrap().len()
        );
        assert!(std::fs::metadata(&p).unwrap().len() > 0);
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
