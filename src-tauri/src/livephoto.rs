//! 实况照片的配对标识（ContentIdentifier）读取与完整性判定。
//!
//! 静态图：Apple MakerNote 的 tag `0x0011`（type=ASCII）。ffprobe 读不到，改为
//! 在文件字节里扫描签名 `"Apple iOS\0"`，再按 TIFF 风格的 IFD 解析。
//! 视频：`moov/meta` 的 `com.apple.quicktime.content.identifier`，由 ffprobe 输出（见 `ffmpeg.rs`）。
//!
//! 该方案已在真实 iPhone 素材上验证：两侧读出的 UUID 完全一致。
//! 见 `docs/superpowers/notes/2026-09-18-content-id-probe.md`。

pub const INTEGRITY_OK: i64 = 0;
pub const INTEGRITY_MISMATCH: i64 = 1;
pub const INTEGRITY_PARTIAL: i64 = 2;
pub const INTEGRITY_STILL_ONLY: i64 = 3;
pub const INTEGRITY_VIDEO_ONLY: i64 = 4;
pub const INTEGRITY_SUSPECT: i64 = 5;

/// 「体积明显偏小」的阈值（字节）。iPhone HEIC 通常 1~3 MB；200 KB 以下视为可疑。
pub const SUSPECT_STILL_MAX: u64 = 200 * 1024;

pub const APPLE_MAKERNOTE_SIGNATURE: &[u8] = b"Apple iOS\0";
const CONTENT_IDENTIFIER_TAG: u16 = 0x0011;

/// 从静态图字节里找 Apple MakerNote 并取出 ContentIdentifier。
pub fn content_id_from_still(bytes: &[u8]) -> Option<String> {
    let mut start = 0usize;
    while let Some(pos) = find_subslice(&bytes[start..], APPLE_MAKERNOTE_SIGNATURE) {
        let at = start + pos;
        if let Some(id) = parse_apple_makernote(&bytes[at..]) {
            return Some(id);
        }
        start = at + 1;
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 解析一个以 `"Apple iOS\0"` 开头的 MakerNote blob。
fn parse_apple_makernote(blob: &[u8]) -> Option<String> {
    if blob.len() < 16 || !blob.starts_with(APPLE_MAKERNOTE_SIGNATURE) {
        return None;
    }
    let big_endian = match &blob[12..14] {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |p: usize| -> Option<u16> {
        let b: [u8; 2] = blob.get(p..p + 2)?.try_into().ok()?;
        Some(if big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |p: usize| -> Option<u32> {
        let b: [u8; 4] = blob.get(p..p + 4)?.try_into().ok()?;
        Some(if big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };

    let entries = u16_at(14)? as usize;
    for i in 0..entries {
        let entry = 16 + i * 12;
        if u16_at(entry)? != CONTENT_IDENTIFIER_TAG {
            continue;
        }
        // TIFF type 2 = ASCII
        if u16_at(entry + 2)? != 2 {
            return None;
        }
        let len = u32_at(entry + 4)? as usize;
        let bytes = if len <= 4 {
            blob.get(entry + 8..entry + 8 + len)?
        } else {
            let off = u32_at(entry + 8)? as usize;
            blob.get(off..off.checked_add(len)?)?
        };
        return clean_id(bytes);
    }
    None
}

/// 清洗从字节/ffprobe 文本里取到的标识：去 NUL 与空白，拒绝空与过长。
pub fn clean_id(raw: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(raw);
    let s = s.trim_end_matches('\0').trim();
    if s.is_empty() || s.len() > 128 {
        return None;
    }
    Some(s.to_string())
}

/// 判定一个条目的 integrity（设计 §10.1）。
///
/// `cloud_hint`：用户勾选了「本次的原件可能不在手机上」时为真——只有此时才做
/// 「体积明显偏小 → 疑似非原件」的判定（设计 §6.4 v1）。
/// `still_size` 仅在「无标识」时用于体积合理性兜底。
pub fn classify(
    has_still: bool,
    has_movie: bool,
    still_id: Option<&str>,
    movie_id: Option<&str>,
    still_size: u64,
    cloud_hint: bool,
) -> i64 {
    let base = match (has_still, has_movie) {
        (true, true) => match (still_id, movie_id) {
            (Some(a), Some(b)) if a == b => INTEGRITY_OK,
            (Some(_), Some(_)) => INTEGRITY_MISMATCH,
            // 有一侧读不到标识：无法确认配对，按残缺实况保守处理
            _ => INTEGRITY_PARTIAL,
        },
        (true, false) => {
            if still_id.is_some() {
                INTEGRITY_PARTIAL // 有标识但缺视频 → 残缺实况
            } else {
                INTEGRITY_STILL_ONLY // 普通照片
            }
        }
        (false, true) => INTEGRITY_VIDEO_ONLY,
        (false, false) => INTEGRITY_PARTIAL,
    };

    // 疑似 iCloud 占位副本：仅静态、无标识、体积明显偏小、且用户提示过 → 优先级最高。
    if cloud_hint && has_still && !has_movie && still_id.is_none() && still_size < SUSPECT_STILL_MAX
    {
        INTEGRITY_SUSPECT
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个 Apple MakerNote：一条 tag 0x0011 的 ASCII 值。
    fn makernote(id: &str, big_endian: bool) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(APPLE_MAKERNOTE_SIGNATURE);
        b.extend_from_slice(&[0, 1]); // version
        b.extend_from_slice(if big_endian { b"MM" } else { b"II" });
        let push16 = |v: &mut Vec<u8>, x: u16| {
            v.extend_from_slice(&if big_endian {
                x.to_be_bytes()
            } else {
                x.to_le_bytes()
            })
        };
        let push32 = |v: &mut Vec<u8>, x: u32| {
            v.extend_from_slice(&if big_endian {
                x.to_be_bytes()
            } else {
                x.to_le_bytes()
            })
        };
        push16(&mut b, 1); // 1 entry
        push16(&mut b, CONTENT_IDENTIFIER_TAG);
        push16(&mut b, 2); // ASCII
        push32(&mut b, (id.len() + 1) as u32);
        // 值放在目录之后：16 + 12 = 28 起
        push32(&mut b, 28);
        b.extend_from_slice(id.as_bytes());
        b.push(0);
        b
    }

    #[test]
    fn finds_identifier_after_signature() {
        let mut file = vec![0u8; 100];
        file.extend_from_slice(&makernote("F0652AEA-5229-4BF7-A366-B4C79E90CA1C", true));
        file.extend_from_slice(&[0u8; 100]);
        assert_eq!(
            content_id_from_still(&file).as_deref(),
            Some("F0652AEA-5229-4BF7-A366-B4C79E90CA1C")
        );
    }

    #[test]
    fn handles_little_endian() {
        let blob = makernote("ABC-123", false);
        assert_eq!(parse_apple_makernote(&blob).as_deref(), Some("ABC-123"));
    }

    #[test]
    fn missing_signature_returns_none() {
        assert!(content_id_from_still(b"not an apple file").is_none());
    }

    #[test]
    fn classify_rules() {
        assert_eq!(
            classify(true, true, Some("X"), Some("X"), 1_000_000, false),
            INTEGRITY_OK
        );
        assert_eq!(
            classify(true, true, Some("X"), Some("Y"), 1_000_000, false),
            INTEGRITY_MISMATCH
        );
        assert_eq!(
            classify(true, false, Some("X"), None, 1_000_000, false),
            INTEGRITY_PARTIAL
        );
        assert_eq!(
            classify(true, false, None, None, 1_000_000, false),
            INTEGRITY_STILL_ONLY
        );
        assert_eq!(
            classify(false, true, None, Some("Y"), 0, false),
            INTEGRITY_VIDEO_ONLY
        );
        // 无标识 + 体积明显偏小，但**未**勾选 cloud_hint → 不判 5
        assert_eq!(
            classify(true, false, None, None, 1000, false),
            INTEGRITY_STILL_ONLY
        );
        // 勾选 cloud_hint + 体积偏小 → 疑似非原件
        assert_eq!(
            classify(true, false, None, None, 1000, true),
            INTEGRITY_SUSPECT
        );
    }
}
