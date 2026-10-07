mod distances;
mod review;
mod source;
mod wikipedia;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use galaxy_catalog::{POLICY_VERSION, Record, ZODIAC, in_scope, meets_dimensions, score, select};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use source::{Fetcher, discover_ids, parse_page, sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(about = "Reproducible galaxy-image catalog prototype")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Join a completed distance snapshot into a curated SQLite catalog.
    DistanceApply {
        #[arg(long, default_value = "crates/galaxy-scraper/data-curated")]
        data: PathBuf,
        #[arg(
            long,
            default_value = "crates/galaxy-scraper/data-distances/curated-distances.json"
        )]
        distances: PathBuf,
    },
    /// Rebuild a curated catalog from photos left in the visual review folder.
    ReviewApply {
        #[arg(long, default_value = "crates/galaxy-scraper/data-wikipedia")]
        data: PathBuf,
        #[arg(long, default_value = "crates/galaxy-scraper/review")]
        review: PathBuf,
        #[arg(long, default_value = "crates/galaxy-scraper/data-curated")]
        output: PathBuf,
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=50))]
        cap: u16,
    },
    /// Add NED distance estimates to a separate, resumable object catalog.
    DistanceEnrich {
        #[arg(long, default_value = "crates/galaxy-scraper/data-wikipedia")]
        data: PathBuf,
        #[arg(
            long,
            default_value = "crates/galaxy-scraper/data-distances/distances.json"
        )]
        output: PathBuf,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        offline: bool,
    },
    /// Discover articles from the galaxies sections of constellation navigation boxes.
    WikipediaDiscover {
        #[arg(long, value_delimiter = ',')]
        constellations: Vec<String>,
        #[arg(long, default_value = "data/wikipedia-manifest.json")]
        output: PathBuf,
        #[arg(long, default_value = "data/wikipedia-cache")]
        cache: PathBuf,
        #[arg(long)]
        offline: bool,
    },
    /// Download eligible article images and optionally merge an existing ESA catalog.
    WikipediaRun {
        #[arg(long, default_value = "data/wikipedia-manifest.json")]
        manifest: PathBuf,
        #[arg(long, default_value = "data/wikipedia")]
        data: PathBuf,
        /// Existing catalog to preserve and merge into a different output directory.
        #[arg(long)]
        base: Option<PathBuf>,
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=50))]
        cap: u16,
        #[arg(long)]
        offline: bool,
        /// Optional bounded smoke run; omitted means every article in the manifest.
        #[arg(long)]
        limit: Option<usize>,
        /// Bounded worker count; each worker paces requests and shares a URL cache lock.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
        jobs: u8,
    },
    /// Freeze image IDs from ranked ESA/Hubble galaxy archive pages.
    Discover {
        #[arg(long, default_value_t = 1)]
        pages: u32,
        #[arg(long, default_value = "data/discovered.json")]
        output: PathBuf,
        #[arg(long, default_value = "data/cache")]
        cache: PathBuf,
        #[arg(long)]
        offline: bool,
    },
    /// Resolve objects, download images, validate, rank, and write SQLite + JSON.
    Run {
        #[arg(long, default_value = "config/demo.json")]
        manifest: PathBuf,
        #[arg(long, default_value = "data/demo")]
        data: PathBuf,
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=50))]
        cap: u16,
        #[arg(long)]
        include_ophiuchus: bool,
        #[arg(long)]
        offline: bool,
    },
    /// Package all candidate images, SQLite database, JSON and credits in a ZIP.
    Pack {
        #[arg(long, default_value = "data/demo")]
        data: PathBuf,
        #[arg(long, default_value = "data/galaxies.zip")]
        output: PathBuf,
    },
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    source: String,
    image_ids: Vec<String>,
    #[serde(default)]
    note: String,
}
#[derive(Serialize)]
struct Outcome {
    image_id: String,
    status: String,
    reason: String,
}
#[derive(Serialize)]
struct Report {
    policy_version: &'static str,
    input_sha256: String,
    cap: u16,
    include_ophiuchus: bool,
    counts: BTreeMap<String, usize>,
    outcomes: Vec<Outcome>,
    candidate_counts_by_source: BTreeMap<String, usize>,
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path.with_extension("part"), bytes)?;
    fs::rename(path.with_extension("part"), path)?;
    Ok(())
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

enum Ingest {
    Candidate(Box<Record>),
    Skip(String),
}
fn ingest(id: &str, fetch: &Fetcher, data: &Path, include_ophiuchus: bool) -> Result<Ingest> {
    ensure!(valid_id(id), "invalid image ID");
    let source_page = format!("https://esahubble.org/images/{id}/");
    let page = parse_page(&fetch.html(&source_page)?)?;
    ensure!(page.id == id, "source image ID mismatch");
    if !in_scope(&page.constellation, include_ophiuchus) {
        return Ok(Ingest::Skip(format!(
            "outside configured constellations: {}",
            page.constellation
        )));
    }
    if page.kind != "Observation"
        || !(page.object_type.contains("Galaxy")
            || page.category.split_whitespace().any(|s| s == "Galaxies"))
    {
        return Ok(Ingest::Skip("not an observed galaxy/system".into()));
    }
    let title = page.title.to_ascii_lowercase();
    if ["annotated", "compass", "artist", "illustration", "chart"]
        .iter()
        .any(|s| title.contains(s))
    {
        return Ok(Ingest::Skip(
            "annotation or non-photographic presentation".into(),
        ));
    }
    if !meets_dimensions(page.width, page.height) {
        return Ok(Ingest::Skip(format!(
            "source dimensions {}x{} below threshold",
            page.width, page.height
        )));
    }
    ensure!(!page.credit.is_empty(), "missing attribution");
    ensure!(
        !page.image_url.is_empty(),
        "missing Large JPEG; alternate image format needs review"
    );
    ensure!(
        !page.names.is_empty(),
        "no linked SIMBAD identifiers; manual resolution needed"
    );
    let mut objects = BTreeMap::new();
    let mut unresolved = Vec::new();
    for alias in &page.names {
        match fetch.resolve(alias) {
            Ok(object) => {
                objects.insert(object.id.clone(), object);
            }
            Err(e) => unresolved.push(format!("{alias}: {e:#}")),
        }
    }
    // Do not silently discard an unresolved component of a galaxy pair.
    ensure!(
        unresolved.is_empty(),
        "identifier review needed: {}",
        unresolved.join("; ")
    );
    let objects: Vec<_> = objects.into_values().collect();
    let bytes = fetch.get(&page.image_url)?;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().context("decode downloaded image")?;
    let (width, height) = (decoded.width(), decoded.height());
    if !meets_dimensions(width, height) {
        return Ok(Ingest::Skip(format!(
            "download dimensions {width}x{height} below threshold"
        )));
    }
    // RGB encoding alone doesn't prove an image is colored. Sample its actual pixels.
    let thumb = decoded.thumbnail(128, 128).to_rgb8();
    let colored = thumb
        .pixels()
        .filter(|p| p.0.iter().max().unwrap() - p.0.iter().min().unwrap() > 12)
        .count();
    let fraction = colored as f64 / (thumb.width() * thumb.height()) as f64;
    if fraction < 0.01 {
        return Ok(Ingest::Skip(
            "monochrome/low-color heuristic; manual review needed".into(),
        ));
    }
    let hash = sha256(&bytes);
    let local_path = format!("images/{hash}.jpg");
    fs::create_dir_all(data.join("images"))?;
    fs::write(data.join(&local_path), &bytes)?;
    let score = score(&page.object_type, &objects);
    Ok(Ingest::Candidate(Box::new(Record {
        image_id: page.id,
        title: page.title,
        source_page,
        image_url: page.image_url,
        source_constellation: page.constellation,
        constellation_method: "archive_label_unverified_by_boundary".into(),
        source_object_type: page.object_type,
        source_names: page.names,
        objects,
        credit: page.credit,
        credit_html: page.credit_html,
        license_url: "https://creativecommons.org/licenses/by/4.0/".into(),
        source_policy_url: "https://esahubble.org/copyright/".into(),
        source_width: page.width,
        source_height: page.height,
        width,
        height,
        colored_pixel_fraction: fraction,
        sha256: hash,
        local_path,
        image_center_ra_text: page.ra_text,
        image_center_dec_text: page.dec_text,
        field_of_view_text: page.fov,
        filters_text: page.filters,
        score,
        review_status: "pending_visual_identity_and_license_review".into(),
        wikipedia: None,
    })))
}

fn database(path: &Path, records: &[Record], selected: &[Record], report: &Report) -> Result<()> {
    let mut db = Connection::open(path)?;
    db.execute_batch(include_str!("schema.sql"))?;
    let tx = db.transaction()?;
    // The database is a rebuildable projection of this run, never an append-only mixture.
    tx.execute_batch("DELETE FROM selections; DELETE FROM image_objects; DELETE FROM images; DELETE FROM object_distances; DELETE FROM objects; DELETE FROM run_metadata;")?;
    for r in records {
        tx.execute(
            "INSERT INTO images VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                r.image_id,
                r.source_constellation,
                r.source_page,
                r.local_path,
                r.sha256,
                r.width,
                r.height,
                r.credit,
                r.score,
                serde_json::to_string(r)?
            ],
        )?;
        for o in &r.objects {
            tx.execute(
                "INSERT OR IGNORE INTO objects VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    o.id,
                    o.ra_deg,
                    o.dec_deg,
                    o.coordinate_frame,
                    o.object_type,
                    o.morphology,
                    o.bibliography_count
                ],
            )?;
            tx.execute(
                "INSERT INTO image_objects VALUES (?1,?2)",
                params![r.image_id, o.id],
            )?;
        }
    }
    for (rank, r) in selected.iter().enumerate() {
        tx.execute(
            "INSERT INTO selections VALUES (?1,?2)",
            params![r.image_id, rank],
        )?;
    }
    tx.execute(
        "INSERT INTO run_metadata VALUES ('report',?1)",
        [serde_json::to_string(report)?],
    )?;
    tx.commit()?;
    Ok(())
}

fn run(
    manifest_path: PathBuf,
    data: PathBuf,
    cap: u16,
    include_ophiuchus: bool,
    offline: bool,
) -> Result<()> {
    let input = fs::read(&manifest_path)?;
    let manifest: Manifest = serde_json::from_slice(&input)?;
    ensure!(manifest.source == "esa-hubble", "unsupported source");
    let mut ids = manifest.image_ids;
    ids.sort();
    ids.dedup();
    ensure!(!ids.is_empty(), "empty input manifest");
    let fetch = Fetcher::new(data.join("cache"), offline)?;
    let _policy = fetch
        .html("https://esahubble.org/copyright/")
        .context("snapshot source reuse policy")?;
    let mut records = Vec::new();
    let mut outcomes = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        eprintln!("[{}/{}] {id}", i + 1, ids.len());
        let (status, reason) = match ingest(id, &fetch, &data, include_ophiuchus) {
            Ok(Ingest::Candidate(r)) => {
                records.push(*r);
                (
                    "candidate",
                    "automatic checks passed; review pending".into(),
                )
            }
            Ok(Ingest::Skip(why)) => ("excluded", why),
            Err(e) => ("error", format!("{e:#}")),
        };
        eprintln!("  {status}: {reason}");
        outcomes.push(Outcome {
            image_id: id.clone(),
            status: status.into(),
            reason,
        });
    }
    finish(&data, &input, &records, outcomes, cap, include_ophiuchus)
}

fn finish(
    data: &Path,
    input: &[u8],
    records: &[Record],
    outcomes: Vec<Outcome>,
    cap: u16,
    include_ophiuchus: bool,
) -> Result<()> {
    let selected = select(records, cap as usize);
    let mut counts: BTreeMap<String, usize> = ZODIAC.into_iter().map(|s| (s.into(), 0)).collect();
    if include_ophiuchus {
        counts.insert("Ophiuchus".into(), 0);
    }
    for r in &selected {
        *counts.entry(r.source_constellation.clone()).or_default() += 1;
    }
    let errors = outcomes.iter().filter(|o| o.status == "error").count();
    let deferred = outcomes.iter().filter(|o| o.status == "deferred").count();
    let mut candidate_counts_by_source = BTreeMap::new();
    for record in records {
        *candidate_counts_by_source
            .entry(if record.wikipedia.is_some() {
                "wikipedia".into()
            } else {
                "esa-hubble".into()
            })
            .or_default() += 1;
    }
    let report = Report {
        policy_version: POLICY_VERSION,
        input_sha256: sha256(input),
        cap,
        include_ophiuchus,
        counts,
        outcomes,
        candidate_counts_by_source,
    };
    fs::write(data.join("input.json"), input)?;
    write_json(&data.join("candidates.json"), &records)?;
    write_json(&data.join("selected.json"), &selected)?;
    write_json(&data.join("report.json"), &report)?;
    database(&data.join("catalog.sqlite"), records, &selected, &report)?;
    let credits = records
        .iter()
        .map(|r| {
            let wiki_details = r
                .wikipedia
                .as_ref()
                .map(|w| {
                    format!(
                        "File: {}\nDownload variant: {}\n",
                        w.file_title, w.download_variant
                    )
                })
                .unwrap_or_default();
            format!(
                "{} ({})\n{}\n{}\nLicense: {}\nPolicy: {}\n{}",
                r.title,
                r.image_id,
                r.credit,
                r.source_page,
                r.license_url,
                r.source_policy_url,
                wiki_details
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(data.join("CREDITS.txt"), credits)?;
    println!(
        "{} candidates; {} provisional selections. Output: {}",
        records.len(),
        selected.len(),
        data.display()
    );
    for (name, count) in &report.counts {
        println!("{name}: {count}");
    }
    ensure!(
        errors == 0 && deferred == 0,
        "{errors} inputs failed; {deferred} deferred; partial output saved with reasons in report.json"
    );
    Ok(())
}

fn pack(data: &Path, output: &Path) -> Result<()> {
    let selected: Vec<Record> = serde_json::from_slice(&fs::read(data.join("candidates.json"))?)?;
    let mut files = vec![
        "catalog.sqlite".into(),
        "input.json".into(),
        "selected.json".into(),
        "candidates.json".into(),
        "report.json".into(),
        "CREDITS.txt".into(),
    ];
    for r in selected {
        ensure!(
            [
                format!("images/{}.jpg", r.sha256),
                format!("images/{}.png", r.sha256)
            ]
            .contains(&r.local_path)
                && r.sha256.len() == 64
                && r.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid local image path"
        );
        ensure!(
            sha256(&fs::read(data.join(&r.local_path))?) == r.sha256,
            "image checksum mismatch: {}",
            r.image_id
        );
        files.push(r.local_path);
    }
    files.sort();
    files.dedup();
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let partial = output.with_extension("part");
    let mut zip = zip::ZipWriter::new(fs::File::create(&partial)?);
    for name in files {
        zip.start_file(
            &name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )?;
        zip.write_all(&fs::read(data.join(&name))?)?;
    }
    zip.finish()?;
    fs::rename(partial, output)?;
    println!(
        "Bundle written: {} (all candidate images; cache stays local)",
        output.display()
    );
    Ok(())
}
fn main() -> Result<()> {
    match Cli::parse().command {
        Command::DistanceApply { data, distances } => distances::apply(&data, &distances),
        Command::ReviewApply {
            data,
            review,
            output,
            cap,
        } => review::apply(&data, &review, &output, cap),
        Command::DistanceEnrich {
            data,
            output,
            limit,
            offline,
        } => distances::run(&data, &output, limit, offline),
        Command::WikipediaDiscover {
            constellations,
            output,
            cache,
            offline,
        } => wikipedia::discover(constellations, output, cache, offline),
        Command::WikipediaRun {
            manifest,
            data,
            base,
            cap,
            offline,
            limit,
            jobs,
        } => wikipedia::run(manifest, data, base, cap, offline, limit, jobs),
        Command::Discover {
            pages,
            output,
            cache,
            offline,
        } => {
            ensure!(
                (1..=100).contains(&pages),
                "pages must be between 1 and 100"
            );
            let fetch = Fetcher::new(cache, offline)?;
            let mut ids = Vec::new();
            for page in 1..=pages {
                let url =
                    format!("https://esahubble.org/images/archive/category/galaxies/page/{page}/");
                let found = discover_ids(&fetch.html(&url)?);
                ensure!(
                    !found.is_empty(),
                    "no image IDs on page {page}; check source layout/end of archive"
                );
                ids.extend(found);
            }
            ids.sort();
            ids.dedup();
            println!("{} image IDs frozen in {}", ids.len(), output.display());
            write_json(
                &output,
                &Manifest {
                    source: "esa-hubble".into(),
                    image_ids: ids,
                    note: format!(
                        "Ranked archive pages 1..={pages}; incomplete unless all pages were requested. Not galaxy counts."
                    ),
                },
            )
        }
        Command::Run {
            manifest,
            data,
            cap,
            include_ophiuchus,
            offline,
        } => run(manifest, data, cap, include_ophiuchus, offline),
        Command::Pack { data, output } => pack(&data, &output),
    }
}
