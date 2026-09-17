use std::collections::BTreeMap;

pub const KIND_PHOTO: i64 = 1;
pub const KIND_VIDEO: i64 = 2;
pub const KIND_LIVE: i64 = 3;

pub const INTEGRITY_OK: i64 = 0;
pub const INTEGRITY_STILL_ONLY: i64 = 3;
pub const INTEGRITY_VIDEO_ONLY: i64 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRef {
    pub path: String,
    pub ext: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedAsset {
    pub base_name: String,
    pub kind: i64,
    pub integrity: i64,
    pub still: Option<FileRef>,
    pub movie: Option<FileRef>,
}

fn is_still_ext(ext: &str) -> bool {
    matches!(
        ext,
        "heic" | "heif" | "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif"
    )
}

fn is_movie_ext(ext: &str) -> bool {
    matches!(ext, "mov" | "mp4" | "m4v" | "avi" | "3gp" | "mkv")
}

/// 主名：去掉最后一个扩展名（`.` 开头且无其他点视为无扩展名）。
pub fn base_name(file_name: &str) -> &str {
    match file_name.rfind('.') {
        Some(i) if i > 0 => &file_name[..i],
        _ => file_name,
    }
}

fn ext_of(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(i) if i > 0 => file_name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// 把「同一目录下的一批文件」按主名配对。输入只含已过滤的媒体文件。
pub fn pair_files(files: &[(String, u64)]) -> Vec<PairedAsset> {
    let mut map: BTreeMap<String, PairedAsset> = BTreeMap::new();

    for (path, size) in files {
        let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let ext = ext_of(file_name);
        let is_still = is_still_ext(&ext);
        let is_movie = is_movie_ext(&ext);
        // 非媒体文件（.txt / .AAE / 无扩展名等）一律不建条目。
        if !is_still && !is_movie {
            continue;
        }

        let base = base_name(file_name).to_string();
        let entry = map.entry(base.clone()).or_insert_with(|| PairedAsset {
            base_name: base,
            kind: KIND_PHOTO,
            integrity: INTEGRITY_OK,
            still: None,
            movie: None,
        });
        let fr = FileRef {
            path: path.clone(),
            ext: ext.clone(),
            size: *size,
        };
        if is_still {
            entry.still = Some(fr);
        } else {
            entry.movie = Some(fr);
        }
    }

    map.into_values()
        .map(|mut a| {
            a.kind = match (&a.still, &a.movie) {
                (Some(_), Some(_)) => KIND_LIVE,
                (Some(_), None) => KIND_PHOTO,
                (None, Some(_)) => KIND_VIDEO,
                (None, None) => KIND_PHOTO,
            };
            a.integrity = match (&a.still, &a.movie) {
                (Some(_), Some(_)) => INTEGRITY_OK,
                (Some(_), None) => INTEGRITY_STILL_ONLY,
                (None, Some(_)) => INTEGRITY_VIDEO_ONLY,
                (None, None) => INTEGRITY_STILL_ONLY,
            };
            a
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str) -> (String, u64) {
        (format!("originals/dev/2024/{name}"), 10)
    }

    #[test]
    fn pairs_live_photo() {
        let assets = pair_files(&[f("IMG_0001.HEIC"), f("IMG_0001.MOV")]);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].kind, KIND_LIVE);
        assert_eq!(assets[0].integrity, INTEGRITY_OK);
    }

    #[test]
    fn classifies_still_only_and_video_only() {
        let still = pair_files(&[f("IMG_0002.JPG")]);
        assert_eq!(still[0].kind, KIND_PHOTO);
        assert_eq!(still[0].integrity, INTEGRITY_STILL_ONLY);

        let video = pair_files(&[f("IMG_0003.MOV")]);
        assert_eq!(video[0].kind, KIND_VIDEO);
        assert_eq!(video[0].integrity, INTEGRITY_VIDEO_ONLY);
    }

    #[test]
    fn ignores_aae_and_sorts_by_base_name() {
        let assets = pair_files(&[f("IMG_0003.MOV"), f("IMG_0001.HEIC"), f("IMG_0001.AAE")]);
        let names: Vec<_> = assets.iter().map(|a| a.base_name.as_str()).collect();
        assert_eq!(names, vec!["IMG_0001", "IMG_0003"]);
    }

    #[test]
    fn edited_variants_are_separate_assets() {
        // IMG_E0102 与 IMG_0102 是不同主名，不能互相配对
        let assets = pair_files(&[f("IMG_0102.JPG"), f("IMG_E0102.JPG"), f("IMG_E0102.MOV")]);
        assert_eq!(assets.len(), 2);
        let e = assets.iter().find(|a| a.base_name == "IMG_E0102").unwrap();
        assert_eq!(e.kind, KIND_LIVE);
    }

    #[test]
    fn ignores_non_media_files_even_with_unique_base_name() {
        let assets = pair_files(&[f("notes.txt"), f("IMG_0101.AAE"), f("IMG_0001.JPG")]);
        assert_eq!(assets.len(), 1, "非媒体文件不应产生条目");
        assert_eq!(assets[0].base_name, "IMG_0001");
    }
}
