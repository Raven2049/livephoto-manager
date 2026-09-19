//! 从磁盘文件读取拍摄时间（Phase 4）。
//!
//! 约定：`taken_at` 是**真实 Unix epoch（UTC 瞬时）**，与前端本地时间显示一致。
//! - 静态图：EXIF `DateTimeOriginal`（无时区）按**本机本地时间**解释；
//! - 视频：`moov/mvhd` 的 `creation_time`（1904 基准，UTC）；
//! - 回退：文件修改时间（`taken_src=1`）。
//!
//! 静态图不解析完整的 ISO BMFF：实测 Apple HEIC 与 Android JPG 的文件里都能直接
//! 扫到 TIFF 头（`MM\0*` / `II*\0`），逐个候选尝试解析日期即可（见 §10 实测）。

use std::path::Path;
use std::sync::OnceLock;

use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};

use crate::device::days_from_civil;

pub const SRC_UNKNOWN: i64 = 0;
/// 来自文件修改时间。
pub const SRC_MTIME: i64 = 1;
/// 来自 EXIF / 视频元数据。
pub const SRC_META: i64 = 2;

/// 先读这么多字节找 TIFF 头 / `mvhd`；不够再读整文件（或文件尾部）。
const PREFIX: usize = 128 * 1024;

/// 返回 `(taken_at, taken_src)`。读不到返回 `(0, SRC_UNKNOWN)`。
pub fn capture_time(path: &Path) -> (i64, i64) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let meta = match ext.as_str() {
        "jpg" | "jpeg" | "heic" | "heif" | "tif" | "tiff" => still_exif_time(path),
        "mov" | "mp4" | "m4v" => mov_creation_time(path),
        _ => None,
    };
    if let Some(t) = meta {
        return (t, SRC_META);
    }
    if let Some(t) = mtime_epoch(path) {
        return (t, SRC_MTIME);
    }
    (0, SRC_UNKNOWN)
}

/// 本机相对 UTC 的偏移（秒）。借 `GetLocalTime` 与 `GetSystemTime` 的民用时间差求得。
pub fn local_offset_secs() -> i64 {
    #[cfg(windows)]
    unsafe {
        let l = GetLocalTime();
        let u = GetSystemTime();
        let le = civil(
            l.wYear as i32,
            l.wMonth as i32,
            l.wDay as i32,
            l.wHour as i64,
            l.wMinute as i64,
            l.wSecond as i64,
        );
        let ue = civil(
            u.wYear as i32,
            u.wMonth as i32,
            u.wDay as i32,
            u.wHour as i64,
            u.wMinute as i64,
            u.wSecond as i64,
        );
        le - ue
    }
    #[cfg(not(windows))]
    {
        0
    }
}

fn offset_cached() -> i64 {
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(local_offset_secs)
}

fn civil(y: i32, m: i32, d: i32, hh: i64, mm: i64, ss: i64) -> i64 {
    days_from_civil(y, m, d) * 86400 + hh * 3600 + mm * 60 + ss
}

fn mtime_epoch(path: &Path) -> Option<i64> {
    let m = std::fs::metadata(path).ok()?;
    m.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

// ---------------------------------------------------------------------------
// 静态图：EXIF
// ---------------------------------------------------------------------------

fn still_exif_time(path: &Path) -> Option<i64> {
    let prefix = read_prefix(path, PREFIX)?;
    if let Some(t) = tiff_time_in(&prefix) {
        return Some(t);
    }
    // 前缀里没找到：HEIC 的 Exif item 可能靠后，读整文件再试。
    let size = std::fs::metadata(path).ok()?.len() as usize;
    if size > prefix.len() {
        let full = std::fs::read(path).ok()?;
        return tiff_time_in(&full);
    }
    None
}

/// 在字节流里逐个尝试 TIFF 头候选，直到某个能解析出日期。
fn tiff_time_in(buf: &[u8]) -> Option<i64> {
    let mut pos = 0usize;
    while let Some(rel) = find_tiff_magic(&buf[pos..]) {
        let start = pos + rel;
        if let Some(t) = tiff_datetime(&buf[start..]) {
            return Some(t);
        }
        pos = start + 4;
    }
    None
}

fn find_tiff_magic(b: &[u8]) -> Option<usize> {
    b.windows(4)
        .position(|w| w == b"MM\x00\x2a" || w == b"II\x2a\x00")
}

fn tiff_datetime(t: &[u8]) -> Option<i64> {
    if t.len() < 8 {
        return None;
    }
    let be = match &t[0..2] {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    if rd16(t, 2, be)? != 42 {
        return None;
    }
    let ifd0 = rd32(t, 4, be)? as usize;
    // ExifIFD 指针（0x8769）→ 优先 DateTimeOriginal / DateTimeDigitized；再退 IFD0 的 DateTime。
    if let Some(p) = ifd_value(t, ifd0, be, 0x8769) {
        if let Some(tm) = ifd_datetime(t, p as usize, be, 0x9003) {
            return Some(tm);
        }
        if let Some(tm) = ifd_datetime(t, p as usize, be, 0x9004) {
            return Some(tm);
        }
    }
    ifd_datetime(t, ifd0, be, 0x0132)
}

fn ifd_datetime(t: &[u8], off: usize, be: bool, tag: u16) -> Option<i64> {
    let n = rd16(t, off, be)? as usize;
    for i in 0..n {
        let e = off.checked_add(2 + i * 12)?;
        if rd16(t, e, be)? != tag {
            continue;
        }
        if rd16(t, e + 2, be)? != 2 {
            return None; // 非 ASCII
        }
        let count = rd32(t, e + 4, be)? as usize;
        let (start, len) = if count <= 4 {
            (e + 8, count)
        } else {
            (rd32(t, e + 8, be)? as usize, count)
        };
        let s = t.get(start..start.checked_add(len)?)?;
        let s = std::str::from_utf8(s).ok()?.trim_end_matches(['\0', ' ']);
        return parse_exif_dt(s);
    }
    None
}

/// 取 IFD 中某个数值型 tag 的 u32 值（ExifIFD 指针用）。
fn ifd_value(t: &[u8], off: usize, be: bool, tag: u16) -> Option<u32> {
    let n = rd16(t, off, be)? as usize;
    for i in 0..n {
        let e = off.checked_add(2 + i * 12)?;
        if rd16(t, e, be)? != tag {
            continue;
        }
        match rd16(t, e + 2, be)? {
            4 => return rd32(t, e + 8, be),
            3 => return rd16(t, e + 8, be).map(|v| v as u32),
            _ => return None,
        }
    }
    None
}

fn rd16(t: &[u8], o: usize, be: bool) -> Option<u16> {
    let b: [u8; 2] = t.get(o..o + 2)?.try_into().ok()?;
    Some(if be {
        u16::from_be_bytes(b)
    } else {
        u16::from_le_bytes(b)
    })
}

fn rd32(t: &[u8], o: usize, be: bool) -> Option<u32> {
    let b: [u8; 4] = t.get(o..o + 4)?.try_into().ok()?;
    Some(if be {
        u32::from_be_bytes(b)
    } else {
        u32::from_le_bytes(b)
    })
}

/// 解析 EXIF 的 `YYYY:MM:DD HH:MM:SS`。无时区 → 按本机本地时间解释。
fn parse_exif_dt(s: &str) -> Option<i64> {
    let (date, time) = s.split_once(' ')?;
    let mut d = date.split(':');
    let y: i32 = d.next()?.trim().parse().ok()?;
    let mo: i32 = d.next()?.trim().parse().ok()?;
    let da: i32 = d.next()?.trim().parse().ok()?;
    let mut t = time.split(':');
    let hh: i64 = t.next()?.trim().parse().ok()?;
    let mi: i64 = t.next()?.trim().parse().ok()?;
    let ss: i64 = t.next()?.trim().parse().ok()?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || !(0..=23).contains(&hh) {
        return None;
    }
    Some(civil(y, mo, da, hh, mi, ss) - offset_cached())
}

// ---------------------------------------------------------------------------
// 视频：moov/mvhd
// ---------------------------------------------------------------------------

fn mov_creation_time(path: &Path) -> Option<i64> {
    let prefix = read_prefix(path, PREFIX)?;
    if let Some(t) = mvhd_in(&prefix) {
        return Some(t);
    }
    let size = std::fs::metadata(path).ok()?.len() as usize;
    if size > PREFIX {
        // 无 faststart 的 moov 常在文件尾部。
        let tail = read_suffix(path, PREFIX)?;
        return mvhd_in(&tail);
    }
    None
}

fn mvhd_in(b: &[u8]) -> Option<i64> {
    let mut pos = 0usize;
    while let Some(rel) = b[pos..].windows(4).position(|w| w == b"mvhd") {
        let at = pos + rel + 4;
        if let Some(t) = parse_mvhd(b, at) {
            return Some(t);
        }
        pos = at;
    }
    None
}

/// `at` 指向 "mvhd" 之后（version 字节）。
fn parse_mvhd(b: &[u8], at: usize) -> Option<i64> {
    let version = *b.get(at)?;
    let c = match version {
        0 => rd32(b, at + 4, true)? as i64,
        1 => {
            let hi = rd32(b, at + 4, true)? as i64;
            let lo = rd32(b, at + 8, true)? as i64;
            (hi << 32) | lo
        }
        _ => return None,
    };
    // 1904-01-01 → 1970-01-01 相差 2082844800 秒；0 视为无效。
    let unix = c - 2_082_844_800;
    if unix <= 0 {
        return None;
    }
    Some(unix)
}

// ---------------------------------------------------------------------------
// 读取辅助
// ---------------------------------------------------------------------------

fn read_prefix(path: &Path, n: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; n];
    let mut read = 0usize;
    while read < n {
        match f.read(&mut buf[read..]) {
            Ok(0) => break,
            Ok(k) => read += k,
            Err(_) => break,
        }
    }
    buf.truncate(read);
    Some(buf)
}

fn read_suffix(path: &Path, n: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let start = size.saturating_sub(n as u64);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(n);
    f.take(n as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_datetime_as_local() {
        let e = parse_exif_dt("2020:08:04 10:03:03").unwrap();
        assert_eq!(e, civil(2020, 8, 4, 10, 3, 3) - offset_cached());
    }

    #[test]
    fn rejects_garbage_datetime() {
        assert!(parse_exif_dt("not a date").is_none());
        assert!(parse_exif_dt("2020:13:40 99:00:00").is_none());
    }

    #[test]
    fn parses_mvhd_v0_creation_time() {
        // size(4) + "mvhd" + version(1)+flags(3) + creation_time(4, BE)
        let unix = 1_713_052_607i64; // 2024-04-13T23:56:47Z
        let c = (unix + 2_082_844_800) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(&100u32.to_be_bytes());
        b.extend_from_slice(b"mvhd");
        b.push(0);
        b.extend_from_slice(&[0, 0, 0]);
        b.extend_from_slice(&c.to_be_bytes());
        assert_eq!(mvhd_in(&b), Some(unix));
    }

    #[test]
    fn ignores_zero_mvhd() {
        let mut b = Vec::new();
        b.extend_from_slice(&100u32.to_be_bytes());
        b.extend_from_slice(b"mvhd");
        b.push(0);
        b.extend_from_slice(&[0, 0, 0]);
        b.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(mvhd_in(&b), None);
    }

    /// 造一个最小 TIFF：IFD0 有 ExifIFD 指针，ExifIFD 里放 DateTimeOriginal。
    fn synthetic_tiff(dt: &str) -> Vec<u8> {
        // 布局：头(8) | IFD0(2 + 12 + 4) | ExifIFD(2 + 12 + 4) | 字符串
        let ifd0 = 8usize;
        let exif_ifd = ifd0 + 2 + 12 + 4; // 26
        let str_off = exif_ifd + 2 + 12 + 4; // 44
        let mut b = vec![0u8; str_off];
        b[0..2].copy_from_slice(b"II");
        b[2..4].copy_from_slice(&42u16.to_le_bytes());
        b[4..8].copy_from_slice(&(ifd0 as u32).to_le_bytes());

        // IFD0: 1 entry: ExifIFD 指针 (0x8769, LONG)
        b[ifd0..ifd0 + 2].copy_from_slice(&1u16.to_le_bytes());
        let e0 = ifd0 + 2;
        b[e0..e0 + 2].copy_from_slice(&0x8769u16.to_le_bytes());
        b[e0 + 2..e0 + 4].copy_from_slice(&4u16.to_le_bytes());
        b[e0 + 4..e0 + 8].copy_from_slice(&1u32.to_le_bytes());
        b[e0 + 8..e0 + 12].copy_from_slice(&(exif_ifd as u32).to_le_bytes());

        // ExifIFD: 1 entry: DateTimeOriginal (0x9003, ASCII)
        b[exif_ifd..exif_ifd + 2].copy_from_slice(&1u16.to_le_bytes());
        let e1 = exif_ifd + 2;
        b[e1..e1 + 2].copy_from_slice(&0x9003u16.to_le_bytes());
        b[e1 + 2..e1 + 4].copy_from_slice(&2u16.to_le_bytes());
        b[e1 + 4..e1 + 8].copy_from_slice(&((dt.len() + 1) as u32).to_le_bytes());
        b[e1 + 8..e1 + 12].copy_from_slice(&(str_off as u32).to_le_bytes());

        b.extend_from_slice(dt.as_bytes());
        b.push(0);
        b
    }

    #[test]
    fn reads_datetime_original_from_tiff() {
        let t = synthetic_tiff("2020:08:04 10:03:03");
        assert_eq!(
            tiff_datetime(&t),
            Some(civil(2020, 8, 4, 10, 3, 3) - offset_cached())
        );
    }

    #[test]
    fn finds_tiff_after_junk_prefix() {
        let mut buf = vec![0xFFu8; 20];
        buf.extend_from_slice(&synthetic_tiff("2021:01:02 03:04:05"));
        assert_eq!(
            tiff_time_in(&buf),
            Some(civil(2021, 1, 2, 3, 4, 5) - offset_cached())
        );
    }
}
