//! Wikipedia navboxes provide discovery; article images and per-file licenses provide assets.
use crate::{
    Outcome, finish,
    source::{Fetcher, css, sha256, text},
    write_json,
};
use anyhow::{Context, Result, bail, ensure};
use galaxy_catalog::{Record, WikipediaProvenance, ZODIAC, in_scope, meets_dimensions, score};
use scraper::Html;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ArticleSeed {
    title: String,
    constellation: String,
    template_title: String,
    template_revision: u64,
}
#[derive(Serialize, Deserialize)]
struct WikiManifest {
    source: String,
    articles: Vec<ArticleSeed>,
}

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    policy_version: String,
    manifest_sha256: String,
    records: BTreeMap<String, Record>,
}
fn seed_key(seed: &ArticleSeed) -> String {
    format!("{}:{}", seed.constellation, seed.title)
}

fn api_url(params: &[(&str, &str)]) -> Result<String> {
    let mut url = reqwest::Url::parse("https://en.wikipedia.org/w/api.php")?;
    url.query_pairs_mut()
        .extend_pairs(params)
        .extend_pairs([("format", "json"), ("formatversion", "2")]);
    Ok(url.to_string())
}
fn api(fetch: &Fetcher, params: &[(&str, &str)]) -> Result<Value> {
    let value: Value = serde_json::from_slice(&fetch.get(&api_url(params)?)?)?;
    ensure!(
        value.get("error").is_none(),
        "MediaWiki API: {}",
        value["error"]
    );
    Ok(value)
}

/// Only navbox galaxy parameters: not every link on the constellation page.
fn galaxy_links(wikitext: &str) -> Vec<String> {
    let field = regex::Regex::new(r"(?m)^\s*\|\s*([a-zA-Z0-9_]+)\s*=").unwrap();
    let links = regex::Regex::new(r"\[\[([^\]|#]+)(?:[^\]]*)\]\]").unwrap();
    let fields: Vec<_> = field.captures_iter(wikitext).collect();
    let mut titles = BTreeSet::new();
    for (i, f) in fields.iter().enumerate() {
        if f[1] != *"galaxies" && !f[1].starts_with("galaxies_") {
            continue;
        }
        let start = f.get(0).unwrap().end();
        let end = fields
            .get(i + 1)
            .map(|m| m.get(0).unwrap().start())
            .unwrap_or(wikitext.len());
        for link in links.captures_iter(&wikitext[start..end]) {
            let title = link[1].trim().replace('_', " ");
            if !title.contains(':') {
                titles.insert(title);
            }
        }
    }
    titles.into_iter().collect()
}

pub fn discover(
    mut constellations: Vec<String>,
    output: PathBuf,
    cache: PathBuf,
    offline: bool,
) -> Result<()> {
    if constellations.is_empty() {
        constellations = ZODIAC.iter().map(|s| (*s).into()).collect();
    }
    constellations.sort();
    constellations.dedup();
    let fetch = Fetcher::new(cache, offline)?;
    let mut articles = Vec::new();
    for constellation in constellations {
        ensure!(
            in_scope(&constellation, true),
            "unknown constellation: {constellation}"
        );
        let mut template = format!("Template:{constellation} (constellation)");
        let request = |title: &str| {
            api(
                &fetch,
                &[
                    ("action", "parse"),
                    ("page", title),
                    ("prop", "wikitext|revid"),
                ],
            )
        };
        let value = match request(&template) {
            Ok(value) => value,
            Err(e) if e.to_string().contains("missingtitle") => {
                template = format!("Template:{constellation}");
                request(&template)?
            }
            Err(e) => return Err(e),
        };
        let parsed = &value["parse"];
        let revision = parsed["revid"]
            .as_u64()
            .context("missing template revision")?;
        let names = galaxy_links(
            parsed["wikitext"]
                .as_str()
                .context("missing template wikitext")?,
        );
        ensure!(
            !names.is_empty(),
            "no galaxy parameters found for {constellation}; inspect template layout"
        );
        println!("{constellation}: {} linked articles", names.len());
        articles.extend(names.into_iter().map(|title| ArticleSeed {
            title,
            constellation: constellation.clone(),
            template_title: template.clone(),
            template_revision: revision,
        }));
    }
    write_json(
        &output,
        &WikiManifest {
            source: "wikipedia-navboxes-v1".into(),
            articles,
        },
    )
}

fn clean_text(html: &str) -> String {
    text(Html::parse_fragment(html).root_element())
}
fn info(doc: &Html, label: &str) -> String {
    let value = doc
        .select(&css("table.infobox tr"))
        .find(|row| {
            row.select(&css("th"))
                .next()
                .is_some_and(|h| text(h).eq_ignore_ascii_case(label))
        })
        .and_then(|r| r.select(&css("td")).next())
        .map(text)
        .unwrap_or_default();
    regex::Regex::new(r"\[\s*\d+\s*\]")
        .unwrap()
        .replace_all(&value, "")
        .trim()
        .to_string()
}
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug)]
struct Article {
    constellation: String,
    kind: String,
    aliases: Vec<String>,
    images: Vec<(String, String)>,
}
fn parse_article(html: &str, title: &str) -> Article {
    let doc = Html::parse_document(html);
    let mut names = vec![title.to_string()];
    // Catalog aliases only from the infobox, never unrelated objects in the navbox.
    let infobox = doc.select(&css("table.infobox")).next();
    if let Some(box_) = infobox {
        for a in box_.select(&css("a[href]")) {
            if let Ok(url) = reqwest::Url::parse(a.value().attr("href").unwrap_or_default())
                && url.host_str().is_some_and(|h| h.starts_with("simbad."))
            {
                for (k, v) in url.query_pairs() {
                    if k.eq_ignore_ascii_case("ident") {
                        names.push(v.into_owned());
                    }
                }
            }
        }
        let aliases =
            regex::Regex::new(r"\b(?:NGC|IC|UGC|PGC|Arp|Messier)\s*\d+[A-Za-z]?\b").unwrap();
        names.extend(
            aliases
                .find_iter(&text(box_))
                .map(|m| m.as_str().to_owned()),
        );
    }
    let mut seen = BTreeSet::new();
    names.retain(|n| seen.insert(n.clone()));
    let intro = doc
        .select(&css(".shortdescription"))
        .next()
        .map(text)
        .unwrap_or_default();
    // Restrict classification context to the infobox + lead, not descriptions of companions.
    let lead = doc
        .select(&css(".mw-parser-output > p"))
        .next()
        .map(text)
        .unwrap_or_default();
    let morphology = info(&doc, "Type");
    let kind = format!(
        "{intro}; {morphology}; {}",
        lead.chars().take(350).collect::<String>()
    );
    let mut images = Vec::new();
    let mut seen = BTreeSet::new();
    for selector in [
        "table.infobox .infobox-image a.mw-file-description",
        "figure a.mw-file-description, .thumb a.mw-file-description, .gallerybox a.mw-file-description",
    ] {
        for a in doc.select(&css(selector)) {
            let href = a.value().attr("href").unwrap_or_default();
            let Some(file) = href.strip_prefix("/wiki/File:") else {
                continue;
            };
            let file = percent_encoding::percent_decode_str(file)
                .decode_utf8_lossy()
                .replace('_', " ");
            let context = a
                .ancestors()
                .filter_map(scraper::ElementRef::wrap)
                .find(|e| {
                    e.value().has_class(
                        "infobox-image",
                        scraper::CaseSensitivity::AsciiCaseInsensitive,
                    ) || e.value().name() == "figure"
                        || e.value()
                            .has_class("thumb", scraper::CaseSensitivity::AsciiCaseInsensitive)
                        || e.value()
                            .has_class("gallerybox", scraper::CaseSensitivity::AsciiCaseInsensitive)
                })
                .map(text)
                .unwrap_or_default();
            let low = format!("{file} {context}").to_lowercase();
            if [
                "annotated",
                "artist",
                "diagram",
                "spectrum",
                "light curve",
                "finder chart",
                "location map",
            ]
            .iter()
            .any(|s| low.contains(s))
            {
                continue;
            }
            let is_infobox = selector.starts_with("table");
            if !is_infobox
                && !names
                    .iter()
                    .any(|n| normalize(&low).contains(&normalize(n)))
            {
                continue;
            }
            let file = format!("File:{file}");
            if seen.insert(file.clone()) {
                images.push((file, context));
            }
        }
    }
    Article {
        constellation: info(&doc, "Constellation"),
        kind,
        aliases: names,
        images,
    }
}

fn ext<'a>(metadata: &'a Value, key: &str) -> &'a str {
    metadata[key]["value"].as_str().unwrap_or_default()
}

/// Conservative, explicit redistribution allowlist. No inference from "on Wikipedia".
fn license(metadata: &Value) -> Result<(String, String)> {
    let short = ext(metadata, "LicenseShortName");
    let raw_url = ext(metadata, "LicenseUrl");
    ensure!(
        !ext(metadata, "NonFree").eq_ignore_ascii_case("true"),
        "non-free image"
    );
    ensure!(
        ext(metadata, "Restrictions").trim().is_empty(),
        "additional restrictions require review"
    );
    let id = ext(metadata, "License").to_ascii_lowercase();
    if short.eq_ignore_ascii_case("Public domain")
        && ext(metadata, "Copyrighted").eq_ignore_ascii_case("false")
        && id == "pd"
    {
        return Ok((
            short.into(),
            "https://creativecommons.org/publicdomain/mark/1.0/".into(),
        ));
    }
    let normalized_url = if raw_url.starts_with("//") {
        format!("https:{raw_url}")
    } else {
        raw_url.to_string()
    };
    let url = reqwest::Url::parse(&normalized_url).context("missing/unrecognized license URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && matches!(
                url.host_str(),
                Some("creativecommons.org" | "www.creativecommons.org")
            ),
        "license is not in redistribution allowlist"
    );
    let path = url
        .path()
        .split("/deed.")
        .next()
        .unwrap()
        .trim_end_matches('/');
    let allowed =
        regex::Regex::new(r"^/licenses/(by|by-sa)/(1\.0|2\.0|2\.5|3\.0|4\.0)(/[a-z]{2})?$")
            .unwrap();
    let cc0 = path == "/publicdomain/zero/1.0";
    ensure!(
        allowed.is_match(path) || cc0,
        "unsupported license: {short} {raw_url}"
    );
    // Reject contradictory metadata instead of trusting the URL alone.
    let expected = if cc0 {
        "cc-zero".to_string()
    } else {
        format!(
            "cc-{}",
            path.trim_start_matches("/licenses/").replace('/', "-")
        )
    };
    ensure!(
        id == expected,
        "conflicting license identifier: {id} vs {expected}"
    );
    Ok((short.into(), format!("https://creativecommons.org{path}/")))
}

fn file_info(fetch: &Fetcher, title: &str) -> Result<Value> {
    let original_params = [
        ("action", "query"),
        ("titles", title),
        ("prop", "imageinfo"),
        ("iiprop", "url|size|mime|extmetadata|timestamp|sha1"),
    ];
    // Preserve fully cached original downloads from earlier runs without another request.
    if fetch.is_cached(&api_url(&original_params)?) {
        let value = api(fetch, &original_params)?;
        let file = &value["query"]["pages"][0]["imageinfo"][0];
        if file["url"].as_str().is_some_and(|url| fetch.is_cached(url))
            || file["width"]
                .as_u64()
                .zip(file["height"].as_u64())
                .is_some_and(|(w, h)| w.max(h) < 720)
        {
            return Ok(file.clone());
        }
    }
    let value = api(
        fetch,
        &[
            ("action", "query"),
            ("titles", title),
            ("prop", "imageinfo"),
            ("iiprop", "url|size|mime|extmetadata|timestamp|sha1"),
            ("iiurlwidth", "1280"),
        ],
    )?;
    value["query"]["pages"][0]["imageinfo"][0]
        .as_object()
        .context("file metadata missing")
        .map(|v| Value::Object(v.clone()))
}

fn image_bytes(
    fetch: &Fetcher,
    data: &Path,
    file: &Value,
) -> Result<(String, String, u32, u32, f64)> {
    ensure!(
        matches!(file["mime"].as_str(), Some("image/jpeg" | "image/png")),
        "only JPEG/PNG photographs supported"
    );
    let width = file["width"].as_u64().context("missing width")?;
    let height = file["height"].as_u64().context("missing height")?;
    ensure!(
        width <= u32::MAX as u64 && height <= u32::MAX as u64,
        "invalid source dimensions"
    );
    ensure!(
        meets_dimensions(width as u32, height as u32),
        "below 720 pixels on both axes"
    );
    let url = download_url(file)?;
    let parsed = reqwest::Url::parse(url)?;
    ensure!(
        parsed.scheme() == "https"
            && matches!(
                parsed.host_str(),
                Some("upload.wikimedia.org" | "thumb.wikimedia.org")
            ),
        "unexpected download host"
    );
    let bytes = fetch.get(url)?;
    let format = image::guess_format(&bytes)?;
    let extension = match format {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        _ => bail!("unsupported decoded format"),
    };
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode()?;
    ensure!(
        decoded.width() as u64 <= width && decoded.height() as u64 <= height,
        "upscaled derivative refused"
    );
    ensure!(
        meets_dimensions(decoded.width(), decoded.height()),
        "decoded image below threshold"
    );
    let thumb = decoded.thumbnail(128, 128).to_rgb8();
    let colored = thumb
        .pixels()
        .filter(|p| p.0.iter().max().unwrap() - p.0.iter().min().unwrap() > 12)
        .count();
    let fraction = colored as f64 / (thumb.width() * thumb.height()) as f64;
    ensure!(fraction >= 0.01, "monochrome/low-color image");
    let hash = sha256(&bytes);
    let local_path = format!("images/{hash}.{extension}");
    fs::create_dir_all(data.join("images"))?;
    fs::write(data.join(&local_path), &bytes)?;
    Ok((
        local_path,
        hash,
        decoded.width(),
        decoded.height(),
        fraction,
    ))
}

enum Ingest {
    Candidate(Box<Record>),
    Excluded(String),
}
fn download_url(file: &Value) -> Result<&str> {
    file["thumburl"]
        .as_str()
        .or_else(|| file["url"].as_str())
        .context("missing download URL")
}
fn ingest(seed: &ArticleSeed, fetch: &Fetcher, data: &Path) -> Result<Ingest> {
    let value = api(
        fetch,
        &[
            ("action", "parse"),
            ("page", &seed.title),
            ("prop", "text|revid"),
            ("redirects", "1"),
        ],
    )?;
    let parsed = &value["parse"];
    let title = parsed["title"].as_str().context("article title missing")?;
    let article = parse_article(
        parsed["text"].as_str().context("article HTML missing")?,
        title,
    );
    if normalize(&article.constellation) != normalize(&seed.constellation) {
        return Ok(Ingest::Excluded(format!(
            "article constellation {:?} does not match navbox {}",
            article.constellation, seed.constellation
        )));
    }
    if article.images.is_empty() {
        return Ok(Ingest::Excluded(
            "no eligible subject photograph in infobox/gallery".into(),
        ));
    }
    if ["globular cluster", "open cluster"]
        .iter()
        .any(|s| article.kind.to_lowercase().contains(s))
    {
        return Ok(Ingest::Excluded(
            "article describes a star cluster, not a galaxy".into(),
        ));
    }
    // A navbox alone is not evidence of galaxy identity. SIMBAD remains authoritative here.
    let mut object = None;
    let mut failures = Vec::new();
    for alias in &article.aliases {
        match fetch.resolve(alias) {
            Ok(o) => {
                object = Some(o);
                break;
            }
            Err(e) => failures.push(format!("{alias}: {e:#}")),
        }
    }
    let Some(object) = object else {
        bail!("identity resolution: {}", failures.join("; "))
    };
    let mut attempts = Vec::new();
    let mut transport_error = false;
    for (file_title, image_context) in &article.images {
        let attempt = (|| -> Result<Record> {
            let file = file_info(fetch, file_title)?;
            let metadata = &file["extmetadata"];
            let (license_name, license_url) = license(metadata)?;
            let artist = ext(metadata, "Artist");
            ensure!(!artist.trim().is_empty(), "missing creator credit");
            let credit_html = [
                artist,
                ext(metadata, "Credit"),
                ext(metadata, "Attribution"),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
            let (local_path, hash, width, height, fraction) = image_bytes(fetch, data, &file)?;
            let objects = vec![object.clone()];
            let source_page = format!(
                "https://en.wikipedia.org/w/index.php?oldid={}",
                parsed["revid"]
                    .as_u64()
                    .context("missing article revision")?
            );
            let file_description_url = file["descriptionurl"]
                .as_str()
                .context("missing file description URL")?
                .to_string();
            Ok(Record {
                image_id: format!(
                    "wiki-{}-{}",
                    parsed["pageid"].as_u64().context("missing page ID")?,
                    &sha256(file_title.as_bytes())[..12]
                ),
                title: title.into(),
                source_page,
                image_url: download_url(&file)?.into(),
                source_constellation: seed.constellation.clone(),
                constellation_method: "wikipedia_navbox_and_article_label_unverified_by_boundary"
                    .into(),
                source_object_type: article.kind.clone(),
                source_names: article.aliases.clone(),
                score: score(&article.kind, &objects),
                objects,
                credit: clean_text(&credit_html),
                credit_html,
                license_url,
                source_policy_url: file_description_url.clone(),
                source_width: file["width"].as_u64().unwrap() as u32,
                source_height: file["height"].as_u64().unwrap() as u32,
                width,
                height,
                colored_pixel_fraction: fraction,
                sha256: hash,
                local_path,
                image_center_ra_text: String::new(),
                image_center_dec_text: String::new(),
                field_of_view_text: String::new(),
                filters_text: String::new(),
                review_status: "license_allowlisted_pending_visual_review".into(),
                wikipedia: Some(WikipediaProvenance {
                    article_title: title.into(),
                    article_revision: parsed["revid"].as_u64().unwrap(),
                    template_title: seed.template_title.clone(),
                    template_revision: seed.template_revision,
                    file_title: file_title.clone(),
                    file_description_url,
                    file_timestamp: file["timestamp"].as_str().unwrap_or_default().into(),
                    file_sha1: file["sha1"].as_str().unwrap_or_default().into(),
                    license_name,
                    license_metadata_json: serde_json::to_string(metadata)?,
                    image_context: image_context.clone(),
                    original_image_url: file["url"].as_str().unwrap_or_default().into(),
                    download_variant: if file["thumburl"].is_string() {
                        "Wikimedia standard 1280px-width derivative"
                    } else {
                        "original"
                    }
                    .into(),
                }),
            })
        })();
        match attempt {
            Ok(record) => return Ok(Ingest::Candidate(Box::new(record))),
            Err(e) => {
                transport_error |= e.to_string().contains("rate limited")
                    || e.chain().any(|c| c.is::<reqwest::Error>())
                    || e.to_string().starts_with("MediaWiki API:")
                    || e.to_string().contains("offline cache miss");
                attempts.push(format!("{file_title}: {e:#}"));
                if fetch.is_rate_limited() {
                    break;
                }
            }
        }
    }
    ensure!(
        !transport_error,
        "image request failed (retryable): {}",
        attempts.join("; ")
    );
    Ok(Ingest::Excluded(format!(
        "no usable licensed color image: {}",
        attempts.join("; ")
    )))
}

fn merge_base(base: &Path, data: &Path) -> Result<(Vec<Record>, String)> {
    ensure!(
        fs::canonicalize(base)? != fs::canonicalize(data)?,
        "base and output must be different directories"
    );
    let bytes = fs::read(base.join("candidates.json"))?;
    let records: Vec<Record> = serde_json::from_slice(&bytes)?;
    for r in &records {
        ensure!(
            [
                format!("images/{}.jpg", r.sha256),
                format!("images/{}.png", r.sha256)
            ]
            .contains(&r.local_path)
                && r.sha256.len() == 64
                && r.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid base image path"
        );
        let path = data.join(&r.local_path);
        let bytes = fs::read(base.join(&r.local_path))?;
        ensure!(
            sha256(&bytes) == r.sha256,
            "base image checksum mismatch: {}",
            r.image_id
        );
        if !path.exists() {
            fs::write(&path, &bytes)?;
        } else {
            ensure!(
                sha256(&fs::read(&path)?) == r.sha256,
                "output image checksum mismatch"
            );
        }
    }
    Ok((records, sha256(&bytes)))
}

pub fn run(
    manifest: PathBuf,
    data: PathBuf,
    base: Option<PathBuf>,
    cap: u16,
    offline: bool,
    limit: Option<usize>,
    jobs: u8,
) -> Result<()> {
    let input_bytes = fs::read(&manifest)?;
    let mut input: WikiManifest = serde_json::from_slice(&input_bytes)?;
    ensure!(
        input.source == "wikipedia-navboxes-v1",
        "unsupported manifest"
    );
    input.articles.sort_by(|a, b| {
        a.constellation
            .cmp(&b.constellation)
            .then(a.title.cmp(&b.title))
    });
    input
        .articles
        .dedup_by(|a, b| a.title == b.title && a.constellation == b.constellation);
    if let Some(n) = limit {
        input.articles.truncate(n);
    }
    ensure!(!input.articles.is_empty(), "no articles to ingest");
    fs::create_dir_all(data.join("images"))?;
    let (mut records, base_hash) = match &base {
        Some(path) => merge_base(path, &data)?,
        None => (vec![], String::new()),
    };
    let mut fetch = Fetcher::new(data.join("cache"), offline)?;
    if let Some(path) = &base {
        fetch = fetch.with_fallback_cache(path.join("cache"));
    }
    let base_count = records.len();
    let checkpoint_path = data.join("wikipedia-checkpoint.json");
    let mut checkpoint = Checkpoint {
        policy_version: galaxy_catalog::POLICY_VERSION.into(),
        manifest_sha256: sha256(&input_bytes),
        records: BTreeMap::new(),
    };
    if checkpoint_path.exists() {
        let saved: Checkpoint = serde_json::from_slice(&fs::read(&checkpoint_path)?)?;
        if saved.policy_version == checkpoint.policy_version
            && saved.manifest_sha256 == checkpoint.manifest_sha256
        {
            for record in saved.records.values() {
                ensure!(
                    [
                        format!("images/{}.jpg", record.sha256),
                        format!("images/{}.png", record.sha256)
                    ]
                    .contains(&record.local_path)
                        && record.sha256.len() == 64
                        && record.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                    "invalid checkpoint image path"
                );
                ensure!(
                    sha256(&fs::read(data.join(&record.local_path))?) == record.sha256,
                    "checkpoint image checksum mismatch"
                );
            }
            checkpoint = saved;
        }
    }
    for seed in &input.articles {
        if let Some(record) = checkpoint.records.get(&seed_key(seed)) {
            records.push(record.clone());
        }
    }
    let prior_records = checkpoint.records.clone();
    let mut outcomes = Vec::new();
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| -> Result<()> {
        let (sender, receiver) = std::sync::mpsc::channel();
        for _ in 0..jobs {
            let sender = sender.clone();
            let next = &next;
            let articles = &input.articles;
            let fetch = &fetch;
            let data = &data;
            let prior_records = &prior_records;
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(seed) = articles.get(i) else { break };
                    let result = match prior_records.get(&seed_key(seed)) {
                        Some(record) => Ok(Ingest::Candidate(Box::new(record.clone()))),
                        None => ingest(seed, fetch, data),
                    };
                    if sender.send((seed, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for (seed, result) in receiver {
            let (status, reason) = match result {
                Ok(Ingest::Candidate(r)) => {
                    checkpoint.records.insert(seed_key(seed), (*r).clone());
                    write_json(&checkpoint_path, &checkpoint)?;
                    records.push(*r);
                    (
                        "candidate",
                        "licensed color article image; visual review pending".into(),
                    )
                }
                Ok(Ingest::Excluded(reason)) => ("excluded", reason),
                Err(e) => {
                    let reason = format!("{e:#}");
                    let status = if reason.contains("rate limited")
                        || reason.contains("offline cache miss")
                    {
                        "deferred"
                    } else {
                        "error"
                    };
                    (status, reason)
                }
            };
            eprintln!(
                "[{}/{}] {} / {}: {status}: {reason}",
                outcomes.len() + 1,
                input.articles.len(),
                seed.constellation,
                seed.title
            );
            outcomes.push(Outcome {
                image_id: seed.title.clone(),
                status: status.into(),
                reason,
            });
            outcomes.sort_by(|a, b| a.image_id.cmp(&b.image_id));
            write_json(&data.join("wikipedia-progress.json"), &outcomes)?;
        }
        Ok(())
    })?;
    if fetch.is_rate_limited() {
        let completed: BTreeSet<_> = outcomes.iter().map(|o| o.image_id.clone()).collect();
        for seed in &input.articles {
            if !completed.contains(&seed.title) {
                let retained = prior_records.contains_key(&seed_key(seed));
                outcomes.push(Outcome {
                    image_id: seed.title.clone(),
                    status: if retained { "candidate" } else { "deferred" }.into(),
                    reason: if retained {
                        "retained previously downloaded candidate"
                    } else {
                        "rate limited; rerun the same command later to resume from cache"
                    }
                    .into(),
                });
            }
        }
        outcomes.sort_by(|a, b| a.image_id.cmp(&b.image_id));
        write_json(&data.join("wikipedia-progress.json"), &outcomes)?;
    }
    let mut unique = BTreeMap::new();
    for mut r in records {
        r.score = score(&r.source_object_type, &r.objects);
        unique.entry(r.image_id.clone()).or_insert(r);
    }
    let records: Vec<_> = unique.into_values().collect();
    let include_ophiuchus = records
        .iter()
        .any(|r| r.source_constellation == "Ophiuchus");
    let inputs = json!({"source": "merged-wikipedia-esa", "wikipedia_manifest_sha256": sha256(&input_bytes), "wikipedia": input,
        "base_directory": base, "base_candidates_sha256": base_hash, "base_candidate_count": base_count, "limit": limit});
    finish(
        &data,
        &serde_json::to_vec_pretty(&inputs)?,
        &records,
        outcomes,
        cap,
        include_ophiuchus,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navbox_only_discovers_galaxies_and_deduplicates_links() {
        let w = "| stars =\n* [[Alpha Arietis]]\n| galaxies_ngc =\n* [[NGC 772|772]]\n* [[NGC 772]]\n| galaxies_other =\n* [[Arp 1]]\n| gclusters =\n* [[Abell 1]]";
        assert_eq!(galaxy_links(w), ["Arp 1", "NGC 772"]);
    }
    #[test]
    fn allowlist_rejects_nonfree_nc_and_conflicting_metadata() {
        let meta = |id: &str, url: &str| json!({"License":{"value":id},"LicenseShortName":{"value":id},"LicenseUrl":{"value":url}});
        assert!(
            license(&meta(
                "cc-by-sa-4.0",
                "https://creativecommons.org/licenses/by-sa/4.0/"
            ))
            .is_ok()
        );
        assert!(
            license(&meta(
                "cc-by-nc-4.0",
                "https://creativecommons.org/licenses/by-nc/4.0/"
            ))
            .is_err()
        );
        assert!(
            license(&meta(
                "fairuse",
                "https://creativecommons.org/licenses/by/4.0/"
            ))
            .is_err()
        );
        assert!(license(&json!({})).is_err());
        let mut m = meta("cc-by-4.0", "https://creativecommons.org/licenses/by/4.0/");
        m["NonFree"] = json!({"value":"true"});
        assert!(license(&m).is_err());
    }
    #[test]
    fn article_images_exclude_unrelated_navbox_icons() {
        let a = parse_article(
            r#"<table class="infobox"><tr><th>Constellation</th><td>Aquarius</td></tr><tr><td class="infobox-image"><a class="mw-file-description" href="/wiki/File:NGC_1.jpg">photo</a></td></tr></table><div class="navbox"><a class="mw-file-description" href="/wiki/File:Other.jpg">Other</a></div>"#,
            "NGC 1",
        );
        assert_eq!(a.constellation, "Aquarius");
        assert_eq!(a.images, [("File:NGC 1.jpg".into(), "photo".into())]);
    }

    #[test]
    fn standard_thumbnail_is_preferred_and_original_is_fallback() {
        let file = json!({"url":"original", "thumburl":"standard-thumbnail"});
        assert_eq!(download_url(&file).unwrap(), "standard-thumbnail");
        assert_eq!(
            download_url(&json!({"url":"original"})).unwrap(),
            "original"
        );
        assert!(download_url(&json!({})).is_err());
    }

    #[test]
    fn ported_license_requires_matching_metadata() {
        let mut meta = json!({"License":{"value":"cc-by-sa-3.0-us"},
            "LicenseShortName":{"value":"CC BY-SA 3.0 US"},
            "LicenseUrl":{"value":"https://creativecommons.org/licenses/by-sa/3.0/us/"}});
        assert!(license(&meta).is_ok());
        meta["License"]["value"] = json!("cc-by-sa-4.0");
        assert!(license(&meta).is_err());
    }

    #[test]
    fn constellation_citations_are_not_part_of_the_name() {
        let article = parse_article(
            r#"<table class="infobox"><tr><th>Constellation</th><td>Aries <sup>[ 2 ]</sup></td></tr></table>"#,
            "NGC 772",
        );
        assert_eq!(article.constellation, "Aries");
    }
}
