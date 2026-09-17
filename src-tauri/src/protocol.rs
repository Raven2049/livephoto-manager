use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use tauri::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
};
use tauri::http::status::StatusCode;
use tauri::http::Response;

/// 常见图片/视频扩展名到 MIME。只做最小映射，够探针与网格用。
pub fn mime_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "heic" | "heif" => "image/heic",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        _ => "application/octet-stream",
    }
}

/// 把请求路径解析成允许根内的真实路径。越界或不存在返回 None。
pub fn resolve_allowed(raw_path: &str, allowed_root: &Path) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(raw_path).decode_utf8_lossy();
    // 去掉前导 '/'，兼容 Windows 盘符（/C:/...）
    let trimmed = decoded.trim_start_matches('/');
    let candidate = PathBuf::from(trimmed);

    let canonical = std::fs::canonicalize(&candidate).ok()?;
    let root = std::fs::canonicalize(allowed_root).ok()?;
    if canonical.starts_with(&root) {
        Some(canonical)
    } else {
        None
    }
}

/// ETag：用「修改时间纳秒 + 文件大小」拼出来。文件一改，ETag 必变。
fn etag_for(meta: &std::fs::Metadata) -> String {
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("\"{mtime:x}-{size:x}\"")
}

/// 构造响应：不做 Range 时全量返回，做 Range 时返回 206。
///
/// 带 ETag + `Cache-Control: no-cache`：浏览器每次会带上 `If-None-Match` 来校验，
/// 命中就回 304（不回文件内容），从而避免回划时重复读盘与解码。
pub fn build_response(
    path: &Path,
    range_header: Option<&str>,
    if_none_match: Option<&str>,
) -> std::io::Result<Response<Vec<u8>>> {
    let mut file = File::open(path)?;
    let meta = file.metadata()?;
    let len = meta.len();
    let mime = mime_from_path(path);
    let etag = etag_for(&meta);

    if if_none_match == Some(etag.as_str()) {
        return Ok(Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(ETAG, etag.as_str())
            .header(CACHE_CONTROL, "no-cache")
            .body(Vec::new())
            .expect("static headers are valid"));
    }

    let builder = Response::builder()
        .header(CONTENT_TYPE, mime)
        .header(ETAG, etag.as_str())
        .header(CACHE_CONTROL, "no-cache");

    let Some(range_header) = range_header else {
        let mut buf = Vec::with_capacity(len as usize);
        file.read_to_end(&mut buf)?;
        return Ok(builder
            .header(CONTENT_LENGTH, len)
            .body(buf)
            .expect("static headers are valid"));
    };

    let ranges = match http_range::HttpRange::parse(range_header, len) {
        Ok(r) => r,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(CONTENT_RANGE, format!("bytes */{len}"))
                .body(Vec::new())
                .expect("static headers are valid"));
        }
    };

    // 单区间是浏览器 <img>/<video> 的常见情形；多区间直接拒绝（网格场景用不到）。
    if ranges.len() != 1 {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .expect("static headers are valid"));
    }

    let r = ranges[0];
    let start = r.start;
    let end = start + r.length - 1;
    if start >= len || end >= len || end < start {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .expect("static headers are valid"));
    }

    let nbytes = end + 1 - start;
    let mut buf = vec![0u8; nbytes as usize];
    file.seek(SeekFrom::Start(start))?;
    file.read_exact(&mut buf)?;

    Ok(builder
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
        .header(CONTENT_LENGTH, nbytes)
        .status(StatusCode::PARTIAL_CONTENT)
        .body(buf)
        .expect("static headers are valid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn mime_maps_known_extensions() {
        assert_eq!(mime_from_path(Path::new("a.JPG")), "image/jpeg");
        assert_eq!(mime_from_path(Path::new("a.mov")), "video/quicktime");
        assert_eq!(
            mime_from_path(Path::new("a.bin")),
            "application/octet-stream"
        );
    }

    #[test]
    fn full_response_has_length_and_mime() {
        let dir = temp_dir("lpm_proto_test");
        let f = dir.join("x.png");
        std::fs::write(&f, b"hello").unwrap();

        let resp = build_response(&f, None, None).unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.body().as_slice(), b"hello");
        assert!(resp.headers().get(ETAG).is_some());
    }

    #[test]
    fn range_response_is_partial() {
        let dir = temp_dir("lpm_proto_test2");
        let f = dir.join("y.bin");
        std::fs::write(&f, b"0123456789").unwrap();

        let resp = build_response(&f, Some("bytes=2-4"), None).unwrap();
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(resp.body().as_slice(), b"234");
        assert_eq!(resp.headers().get(CONTENT_RANGE).unwrap(), "bytes 2-4/10");
    }

    #[test]
    fn matching_etag_returns_304_without_body() {
        let dir = temp_dir("lpm_proto_test3");
        let f = dir.join("z.jpg");
        std::fs::write(&f, b"hello").unwrap();

        let etag = build_response(&f, None, None)
            .unwrap()
            .headers()
            .get(ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let resp = build_response(&f, None, Some(&etag)).unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert!(resp.body().is_empty());
        assert_eq!(resp.headers().get(ETAG).unwrap(), etag.as_str());
    }

    #[test]
    fn resolve_rejects_path_outside_root() {
        let root = temp_dir("lpm_proto_root");
        let outside = std::env::temp_dir().join("lpm_proto_outside.txt");
        std::fs::write(&outside, b"x").unwrap();

        let escaped = format!("/{}", outside.to_string_lossy().replace('\\', "/"));
        assert!(resolve_allowed(&escaped, &root).is_none());
    }

    #[test]
    fn resolve_accepts_path_inside_root() {
        let root = temp_dir("lpm_proto_root2");
        let inside = root.join("a.jpg");
        std::fs::write(&inside, b"x").unwrap();

        let requested = format!("/{}", inside.to_string_lossy().replace('\\', "/"));
        assert!(resolve_allowed(&requested, &root).is_some());
    }
}
