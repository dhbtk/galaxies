//! Rebuild a catalog from a frozen review manifest and the files the user kept.
use crate::{Outcome, finish, source::sha256};
use anyhow::{Context, Result, ensure};
use galaxy_catalog::Record;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};

#[derive(Deserialize)]
struct ReviewManifest {
    source_data: String,
    candidates_sha256: String,
    photos: Vec<ReviewPhoto>,
}

#[derive(Deserialize)]
struct ReviewPhoto {
    review_path: String,
    source_path: String,
    sha256: String,
    constellation: String,
    records: Vec<ReviewRecord>,
}

#[derive(Deserialize)]
struct ReviewRecord {
    image_id: String,
    title: String,
}

fn safe_review_path(path: &str, constellation: &str) -> bool {
    let components: Vec<_> = Path::new(path).components().collect();
    matches!(components.as_slice(),
        [Component::Normal(a), Component::Normal(b), Component::Normal(_)]
        if *a == "images" && *b == constellation)
}

pub fn apply(data: &Path, review: &Path, output: &Path, cap: u16) -> Result<()> {
    ensure!(
        !output.exists(),
        "output already exists; use a new output path to preserve review decisions"
    );
    let data = fs::canonicalize(data)?;
    let review = fs::canonicalize(review)?;
    let manifest_bytes = fs::read(review.join("manifest.json"))?;
    let manifest: ReviewManifest = serde_json::from_slice(&manifest_bytes)?;
    ensure!(
        fs::canonicalize(&manifest.source_data)? == data,
        "review manifest points to a different source catalog"
    );
    let candidate_bytes = fs::read(data.join("candidates.json"))?;
    ensure!(
        sha256(&candidate_bytes) == manifest.candidates_sha256,
        "source candidates changed since review began"
    );
    let candidates: Vec<Record> = serde_json::from_slice(&candidate_bytes)?;
    let by_id: BTreeMap<_, _> = candidates
        .iter()
        .map(|r| (r.image_id.as_str(), r))
        .collect();
    ensure!(by_id.len() == candidates.len(), "duplicate candidate IDs");
    let mut seen_ids = BTreeSet::new();
    let mut seen_paths = BTreeSet::new();
    let mut kept = Vec::new();
    let mut rejected = Vec::new();
    let mut outcomes = Vec::new();
    for photo in &manifest.photos {
        ensure!(
            safe_review_path(&photo.review_path, &photo.constellation),
            "unsafe review path"
        );
        ensure!(
            seen_paths.insert(&photo.review_path),
            "duplicate review path"
        );
        ensure!(!photo.records.is_empty(), "review photo without records");
        ensure!(
            photo.sha256.len() == 64 && photo.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid photo checksum"
        );
        let review_file = review.join(&photo.review_path);
        let retained = match fs::symlink_metadata(&review_file) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_file(),
                    "review image is not a regular file: {}",
                    review_file.display()
                );
                ensure!(
                    sha256(&fs::read(&review_file)?) == photo.sha256,
                    "review image was modified: {}",
                    review_file.display()
                );
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        for linked in &photo.records {
            ensure!(
                seen_ids.insert(linked.image_id.as_str()),
                "duplicate review record ID"
            );
            let record = by_id
                .get(linked.image_id.as_str())
                .context("review ID absent from source catalog")?;
            ensure!(
                record.title == linked.title
                    && record.sha256 == photo.sha256
                    && record.local_path == photo.source_path
                    && record.source_constellation == photo.constellation,
                "review mapping disagrees with source catalog: {}",
                linked.image_id
            );
            if retained {
                let mut record = (*record).clone();
                record.review_status.push_str("; kept_after_visual_review");
                kept.push(record);
            } else {
                rejected.push(linked.image_id.clone());
                outcomes.push(Outcome {
                    image_id: linked.image_id.clone(),
                    status: "rejected_visual_review".into(),
                    reason: format!("review image removed: {}", photo.review_path),
                });
            }
        }
    }
    ensure!(
        seen_ids.len() == candidates.len(),
        "review manifest does not cover every candidate"
    );
    let retained_images: BTreeMap<_, _> = kept
        .iter()
        .map(|r| (r.local_path.as_str(), r.sha256.as_str()))
        .collect();
    for (local_path, hash) in &retained_images {
        ensure!(
            matches!(Path::new(local_path).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(a), Component::Normal(_)] if *a == "images"),
            "unsafe source image path"
        );
        ensure!(
            sha256(&fs::read(data.join(local_path))?) == *hash,
            "source image checksum mismatch: {local_path}"
        );
    }
    let input = serde_json::to_vec_pretty(&json!({
        "source": "visual-review-v1", "source_catalog": data,
        "source_candidates_sha256": manifest.candidates_sha256,
        "review_manifest_sha256": sha256(&manifest_bytes),
        "retained_review_photos": retained_images.len(),
        "rejected_image_ids": rejected,
    }))?;
    let staging = output.with_extension("part");
    ensure!(
        !staging.exists(),
        "staging output already exists: {}",
        staging.display()
    );
    fs::create_dir_all(staging.join("images"))?;
    for local_path in retained_images.keys() {
        fs::copy(data.join(local_path), staging.join(local_path))?;
    }
    finish(&staging, &input, &kept, outcomes, cap, false)?;
    fs::write(staging.join("review-manifest.json"), manifest_bytes)?;
    fs::rename(&staging, output)?;
    println!(
        "Curated catalog: {} kept records; {} removed; {}",
        kept.len(),
        candidates.len() - kept.len(),
        output.display()
    );
    Ok(())
}
