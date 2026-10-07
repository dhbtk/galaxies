//! Shared catalog model and deterministic, provisional selection policy.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY_VERSION: &str = "wiki-supplement-v2-720-long-edge";
pub const ZODIAC: [&str; 12] = [
    "Aries",
    "Taurus",
    "Gemini",
    "Cancer",
    "Leo",
    "Virgo",
    "Libra",
    "Scorpius",
    "Sagittarius",
    "Capricornus",
    "Aquarius",
    "Pisces",
];

pub fn in_scope(name: &str, include_ophiuchus: bool) -> bool {
    ZODIAC.contains(&name) || (include_ophiuchus && name == "Ophiuchus")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    pub id: String,
    pub queried_alias: String,
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub coordinate_frame: String,
    pub coordinate_epoch: String,
    pub object_type: String,
    pub morphology: Option<String>,
    pub bibliography_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub image_id: String,
    pub title: String,
    pub source_page: String,
    pub image_url: String,
    pub source_constellation: String,
    pub constellation_method: String,
    pub source_object_type: String,
    pub source_names: Vec<String>,
    pub objects: Vec<Object>,
    pub credit: String,
    pub credit_html: String,
    pub license_url: String,
    pub source_policy_url: String,
    pub source_width: u32,
    pub source_height: u32,
    pub width: u32,
    pub height: u32,
    pub colored_pixel_fraction: f64,
    pub sha256: String,
    pub local_path: String,
    pub image_center_ra_text: String,
    pub image_center_dec_text: String,
    pub field_of_view_text: String,
    pub filters_text: String,
    pub score: u64,
    pub review_status: String,
    /// Optional source evidence; old ESA catalogs remain readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikipedia: Option<WikipediaProvenance>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WikipediaProvenance {
    pub article_title: String,
    pub article_revision: u64,
    pub template_title: String,
    pub template_revision: u64,
    pub file_title: String,
    pub file_description_url: String,
    pub file_timestamp: String,
    pub file_sha1: String,
    pub license_name: String,
    pub license_metadata_json: String,
    pub image_context: String,
    #[serde(default)]
    pub original_image_url: String,
    #[serde(default)]
    pub download_variant: String,
}

/// A transparent priority heuristic, not an objective popularity measurement.
/// Structured archive morphology outranks scientific reference counts.
pub fn score(object_type: &str, objects: &[Object]) -> u64 {
    let kind = object_type.to_ascii_lowercase();
    let distinct = ["interact", "ring", "merg", "pair", "peculiar"]
        .iter()
        .any(|tag| kind.contains(tag));
    let spiral = kind.contains("spiral")
        || objects
            .iter()
            .any(|o| o.morphology.as_deref().is_some_and(is_spiral_morphology));
    let refs = objects
        .iter()
        .map(|o| o.bibliography_count)
        .max()
        .unwrap_or(0);
    u64::from(spiral) * 20_000 + u64::from(distinct) * 10_000 + refs.min(9_999)
}

pub fn is_spiral_morphology(value: &str) -> bool {
    let value = value.trim_start_matches(['(', ' ']);
    ["Sa", "Sb", "Sc", "Sd", "Sm", "SA", "SB", "SAB"]
        .iter()
        .any(|p| value.starts_with(p))
        && !value.contains('0')
}

pub fn meets_dimensions(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && width.max(height) >= 720
}

/// Stable ordering, one tile per canonical object/system and per exact image.
/// Every input must already have passed ingestion gates. Selections need visual review.
pub fn select(records: &[Record], cap: usize) -> Vec<Record> {
    let mut sorted: Vec<_> = records
        .iter()
        .filter(|r| !r.objects.is_empty())
        .cloned()
        .collect();
    sorted.sort_by(|a, b| b.score.cmp(&a.score).then(a.image_id.cmp(&b.image_id)));
    let mut counts = BTreeMap::<String, usize>::new();
    let mut objects = BTreeSet::new();
    let mut images = BTreeSet::new();
    sorted
        .into_iter()
        .filter(|r| {
            let count = counts.entry(r.source_constellation.clone()).or_default();
            if *count >= cap
                || images.contains(&r.sha256)
                || r.objects.iter().any(|o| objects.contains(&o.id))
            {
                return false;
            }
            *count += 1;
            images.insert(r.sha256.clone());
            objects.extend(r.objects.iter().map(|o| o.id.clone()));
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(id: &str, object: &str, constellation: &str, score: u64) -> Record {
        Record {
            image_id: id.into(),
            title: String::new(),
            source_page: String::new(),
            image_url: String::new(),
            source_constellation: constellation.into(),
            constellation_method: String::new(),
            source_object_type: String::new(),
            source_names: vec![],
            objects: vec![Object {
                id: object.into(),
                queried_alias: object.into(),
                ra_deg: 0.0,
                dec_deg: 0.0,
                coordinate_frame: "ICRS".into(),
                coordinate_epoch: String::new(),
                object_type: "G".into(),
                morphology: None,
                bibliography_count: 0,
            }],
            credit: String::new(),
            credit_html: String::new(),
            license_url: String::new(),
            source_policy_url: String::new(),
            source_width: 1920,
            source_height: 1080,
            width: 1920,
            height: 1080,
            colored_pixel_fraction: 1.0,
            sha256: id.into(),
            local_path: String::new(),
            image_center_ra_text: String::new(),
            image_center_dec_text: String::new(),
            field_of_view_text: String::new(),
            filters_text: String::new(),
            score,
            review_status: String::new(),
            wikipedia: None,
        }
    }
    #[test]
    fn selection_is_order_independent_capped_and_deduplicated() {
        let mut input = vec![
            fixture("b", "NGC 1", "Aries", 20),
            fixture("a", "NGC 1", "Aries", 20),
            fixture("c", "NGC 2", "Aries", 10),
            fixture("d", "NGC 3", "Virgo", 10),
        ];
        let ids = |rs: Vec<Record>| rs.into_iter().map(|r| r.image_id).collect::<Vec<_>>();
        assert_eq!(ids(select(&input, 1)), ["a", "d"]);
        let expected = ids(select(&input, 50));
        input.reverse();
        assert_eq!(ids(select(&input, 50)), expected);
        assert_eq!(expected, ["a", "c", "d"]);
        input[0].sha256 = "a".into(); // Identical image bytes under a different object ID.
        assert_eq!(ids(select(&input, 50)), ["a", "c"]);
    }
    #[test]
    fn resolution_is_orientation_independent_and_does_not_upscale() {
        assert!(meets_dimensions(1920, 1080));
        assert!(meets_dimensions(1080, 1920));
        assert!(meets_dimensions(4000, 900));
        assert!(meets_dimensions(720, 400));
        assert!(meets_dimensions(400, 720));
        assert!(!meets_dimensions(719, 719));
        assert!(!meets_dimensions(720, 0));
    }
    #[test]
    fn constellation_names_are_exact() {
        assert!(in_scope("Scorpius", false));
        assert!(!in_scope("Pisces Austrinus", true));
        assert!(!in_scope("Ophiuchus", false));
        assert!(in_scope("Ophiuchus", true));
    }
}
