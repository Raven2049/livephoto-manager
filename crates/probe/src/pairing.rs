use crate::model::{FileKind, RemoteFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairStatus {
    /// 静态图 + 视频都有 → 实况照片
    Paired,
    /// 只有静态图，且是 HEIC/JPG
    StillOnly,
    /// 只有视频
    VideoOnly,
}

#[derive(Debug, Clone)]
pub struct Pair {
    // 目前仅测试读取；后续阶段（配对展示、重命名）会用到。
    #[allow(dead_code)]
    pub base_name: String,
    pub still: Option<RemoteFile>,
    pub movie: Option<RemoteFile>,
    pub status: PairStatus,
}

pub fn pair(files: &[RemoteFile]) -> Vec<Pair> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<String, Pair> = BTreeMap::new();

    for file in files {
        if file.kind == FileKind::Other {
            continue;
        }
        let base = file.base_name().to_string();
        let entry = map.entry(base.clone()).or_insert_with(|| Pair {
            base_name: base,
            still: None,
            movie: None,
            status: PairStatus::StillOnly,
        });
        match file.kind {
            FileKind::Image => entry.still = Some(file.clone()),
            FileKind::Video => entry.movie = Some(file.clone()),
            FileKind::Other => {}
        }
    }

    map.into_values()
        .map(|mut p| {
            p.status = match (&p.still, &p.movie) {
                (Some(_), Some(_)) => PairStatus::Paired,
                (Some(_), None) => PairStatus::StillOnly,
                (None, Some(_)) => PairStatus::VideoOnly,
                (None, None) => PairStatus::StillOnly,
            };
            p
        })
        .collect()
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

    #[test]
    fn pairs_still_with_movie_of_same_base_name() {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
        ];
        let pairs = pair(&files);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].base_name, "IMG_0001");
        assert_eq!(pairs[0].status, PairStatus::Paired);
        assert!(pairs[0].still.is_some());
        assert!(pairs[0].movie.is_some());
    }

    #[test]
    fn classifies_lone_image_as_still_only() {
        let pairs = pair(&[f("IMG_0002.HEIC", FileKind::Image)]);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].status, PairStatus::StillOnly);
        assert!(pairs[0].movie.is_none());
    }

    #[test]
    fn classifies_lone_video_as_video_only() {
        let pairs = pair(&[f("IMG_0003.MOV", FileKind::Video)]);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].status, PairStatus::VideoOnly);
        assert!(pairs[0].still.is_none());
    }

    #[test]
    fn keeps_result_sorted_by_base_name() {
        let files = vec![
            f("IMG_0003.MOV", FileKind::Video),
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
        ];
        let pairs = pair(&files);
        let names: Vec<_> = pairs.iter().map(|p| p.base_name.as_str()).collect();
        assert_eq!(names, vec!["IMG_0001", "IMG_0003"]);
    }

    #[test]
    fn ignores_unrelated_file_types() {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
            f("IMG_0001.AAE", FileKind::Other),
        ];
        let pairs = pair(&files);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].status, PairStatus::Paired);
    }

    #[test]
    fn jpg_still_pairs_too() {
        let files = vec![
            f("IMG_0004.JPG", FileKind::Image),
            f("IMG_0004.MOV", FileKind::Video),
        ];
        let pairs = pair(&files);
        assert_eq!(pairs[0].status, PairStatus::Paired);
    }
}
