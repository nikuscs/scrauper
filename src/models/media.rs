use super::region::Region;
use serde::{Deserialize, Serialize};

/// Media types that can be downloaded from ScreenScraper and stored in ES-DE's directory structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Screenshot,
    TitleScreen,
    Video,
    BoxFront,
    BoxBack,
    Box3d,
    Marquee,
    PhysicalMedia,
    FanArt,
    Manual,
    Miximage,
}

impl MediaType {
    /// The ES-DE media subdirectory name for this media type.
    pub fn es_de_dirname(&self) -> &'static str {
        match self {
            MediaType::Screenshot => "screenshots",
            MediaType::TitleScreen => "titlescreens",
            MediaType::Video => "videos",
            MediaType::BoxFront => "covers",
            MediaType::BoxBack => "backcovers",
            MediaType::Box3d => "3dboxes",
            MediaType::Marquee => "marquees",
            MediaType::PhysicalMedia => "physicalmedia",
            MediaType::FanArt => "fanart",
            MediaType::Manual => "manuals",
            MediaType::Miximage => "miximages",
        }
    }

    /// The file extension for downloaded media of this type.
    pub fn extension(&self) -> &'static str {
        match self {
            MediaType::Video => "mp4",
            MediaType::Manual => "pdf",
            _ => "png",
        }
    }

    /// The ScreenScraper API media identifier, with optional region suffix.
    pub fn api_media_id(&self, region: &Region) -> String {
        let suffix = region.api_suffix();
        match self {
            MediaType::Screenshot => format!("ss({suffix})"),
            MediaType::TitleScreen => format!("sstitle({suffix})"),
            MediaType::Video => "video".to_string(),
            MediaType::BoxFront => format!("box-2D({suffix})"),
            MediaType::BoxBack => format!("box-2D-back({suffix})"),
            MediaType::Box3d => format!("box-3D({suffix})"),
            MediaType::Marquee => format!("wheel-hd({suffix})"),
            MediaType::PhysicalMedia => format!("support-texture({suffix})"),
            MediaType::FanArt => "fanart".to_string(),
            MediaType::Manual => format!("manuel({suffix})"),
            MediaType::Miximage => "mixrbv2".to_string(),
        }
    }

    /// Whether this media type varies by region.
    #[allow(dead_code)]
    pub fn is_regionalized(&self) -> bool {
        matches!(
            self,
            MediaType::BoxFront
                | MediaType::BoxBack
                | MediaType::Box3d
                | MediaType::Marquee
                | MediaType::PhysicalMedia
                | MediaType::Manual
                | MediaType::Screenshot
                | MediaType::TitleScreen
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_es_de_dirname() {
        assert_eq!(MediaType::Screenshot.es_de_dirname(), "screenshots");
        assert_eq!(MediaType::BoxFront.es_de_dirname(), "covers");
        assert_eq!(MediaType::Box3d.es_de_dirname(), "3dboxes");
        assert_eq!(MediaType::Marquee.es_de_dirname(), "marquees");
        assert_eq!(MediaType::Video.es_de_dirname(), "videos");
        assert_eq!(MediaType::Manual.es_de_dirname(), "manuals");
        assert_eq!(MediaType::Miximage.es_de_dirname(), "miximages");
        assert_eq!(MediaType::PhysicalMedia.es_de_dirname(), "physicalmedia");
        assert_eq!(MediaType::FanArt.es_de_dirname(), "fanart");
    }

    #[test]
    fn test_extension() {
        assert_eq!(MediaType::Screenshot.extension(), "png");
        assert_eq!(MediaType::BoxFront.extension(), "png");
        assert_eq!(MediaType::Video.extension(), "mp4");
        assert_eq!(MediaType::Manual.extension(), "pdf");
        assert_eq!(MediaType::Marquee.extension(), "png");
    }

    #[test]
    fn test_api_media_id_with_region() {
        let us = Region::Us;
        assert_eq!(MediaType::Screenshot.api_media_id(&us), "ss(us)");
        assert_eq!(MediaType::BoxFront.api_media_id(&us), "box-2D(us)");
        assert_eq!(MediaType::Box3d.api_media_id(&us), "box-3D(us)");
        assert_eq!(MediaType::Marquee.api_media_id(&us), "wheel-hd(us)");
    }

    #[test]
    fn test_api_media_id_non_regionalized() {
        let us = Region::Us;
        assert_eq!(MediaType::Video.api_media_id(&us), "video");
        assert_eq!(MediaType::FanArt.api_media_id(&us), "fanart");
        assert_eq!(MediaType::Miximage.api_media_id(&us), "mixrbv2");
    }

    #[test]
    fn test_api_media_id_different_regions() {
        assert_eq!(MediaType::BoxFront.api_media_id(&Region::Jp), "box-2D(jp)");
        assert_eq!(MediaType::BoxFront.api_media_id(&Region::Eu), "box-2D(eu)");
    }

    #[test]
    fn test_is_regionalized() {
        assert!(MediaType::Screenshot.is_regionalized());
        assert!(MediaType::BoxFront.is_regionalized());
        assert!(MediaType::Marquee.is_regionalized());
        assert!(!MediaType::Video.is_regionalized());
        assert!(!MediaType::FanArt.is_regionalized());
        assert!(!MediaType::Miximage.is_regionalized());
    }

    #[test]
    fn test_serde_roundtrip() {
        let mt = MediaType::Screenshot;
        let json = serde_json::to_string(&mt).unwrap();
        assert_eq!(json, "\"screenshot\"");
        let back: MediaType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, MediaType::Screenshot);

        // Box3d serializes as "box3d" (snake_case collapses the number)
        let mt2 = MediaType::Box3d;
        let json2 = serde_json::to_string(&mt2).unwrap();
        let back2: MediaType = serde_json::from_str(&json2).unwrap();
        assert_eq!(back2, MediaType::Box3d);
    }

    #[test]
    fn test_es_de_dirname_all_types() {
        assert_eq!(MediaType::TitleScreen.es_de_dirname(), "titlescreens");
        assert_eq!(MediaType::BoxBack.es_de_dirname(), "backcovers");
    }

    #[test]
    fn test_extension_all_types() {
        assert_eq!(MediaType::TitleScreen.extension(), "png");
        assert_eq!(MediaType::BoxBack.extension(), "png");
        assert_eq!(MediaType::Box3d.extension(), "png");
        assert_eq!(MediaType::PhysicalMedia.extension(), "png");
        assert_eq!(MediaType::FanArt.extension(), "png");
        assert_eq!(MediaType::Miximage.extension(), "png");
    }

    #[test]
    fn test_api_media_id_all_regionalized() {
        let jp = Region::Jp;
        assert_eq!(MediaType::TitleScreen.api_media_id(&jp), "sstitle(jp)");
        assert_eq!(MediaType::BoxBack.api_media_id(&jp), "box-2D-back(jp)");
        assert_eq!(MediaType::PhysicalMedia.api_media_id(&jp), "support-texture(jp)");
        assert_eq!(MediaType::Manual.api_media_id(&jp), "manuel(jp)");
    }

    #[test]
    fn test_is_regionalized_all_types() {
        assert!(MediaType::TitleScreen.is_regionalized());
        assert!(MediaType::BoxBack.is_regionalized());
        assert!(MediaType::Box3d.is_regionalized());
        assert!(MediaType::PhysicalMedia.is_regionalized());
        assert!(MediaType::Manual.is_regionalized());
    }

    #[test]
    fn test_serde_all_variants() {
        let types = vec![
            MediaType::Screenshot,
            MediaType::TitleScreen,
            MediaType::Video,
            MediaType::BoxFront,
            MediaType::BoxBack,
            MediaType::Box3d,
            MediaType::Marquee,
            MediaType::PhysicalMedia,
            MediaType::FanArt,
            MediaType::Manual,
            MediaType::Miximage,
        ];
        for mt in types {
            let json = serde_json::to_string(&mt).unwrap();
            let back: MediaType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, mt);
        }
    }
}
