use anyhow::{Context, Result, bail, ensure};
use galaxy_catalog::Object;
use scraper::{Html, Selector};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime},
};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn css(s: &str) -> Selector {
    Selector::parse(s).expect("static CSS selector")
}
pub fn text(e: scraper::ElementRef<'_>) -> String {
    e.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const USER_AGENT: &str = "GalaxyCollageBot/0.1 (mailto:galaxy-bot@dianahorbatiuk.com)";

fn retry_delay(header: Option<&str>, attempt: u32, now: SystemTime) -> Duration {
    let fallback = Duration::from_secs(5 << attempt.min(6));
    let requested = header
        .and_then(|s| {
            s.trim()
                .parse::<u64>()
                .ok()
                .map(Duration::from_secs)
                .or_else(|| {
                    httpdate::parse_http_date(s)
                        .ok()
                        .map(|date| date.duration_since(now).unwrap_or_default())
                })
        })
        .unwrap_or_default();
    requested.max(fallback)
}

pub struct Fetcher {
    client: reqwest::blocking::Client,
    cache: PathBuf,
    offline: bool,
    fallback_cache: Option<PathBuf>,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    rate_limited: AtomicBool,
    network: Mutex<()>,
}
impl Fetcher {
    pub fn new(cache: PathBuf, offline: bool) -> Result<Self> {
        fs::create_dir_all(&cache)?;
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(90))
                .build()?,
            cache,
            offline,
            fallback_cache: None,
            locks: Mutex::new(HashMap::new()),
            rate_limited: AtomicBool::new(false),
            network: Mutex::new(()),
        })
    }
    pub fn with_fallback_cache(mut self, path: PathBuf) -> Self {
        self.fallback_cache = Some(path);
        self
    }
    pub fn is_cached(&self, url: &str) -> bool {
        let key = sha256(url.as_bytes());
        self.cache.join(&key).is_file()
            || self
                .fallback_cache
                .as_ref()
                .is_some_and(|p| p.join(key).is_file())
    }
    pub fn get(&self, url: &str) -> Result<Vec<u8>> {
        let lock = self
            .locks
            .lock()
            .expect("cache lock")
            .entry(url.into())
            .or_default()
            .clone();
        let _guard = lock.lock().expect("request lock");
        let path = self.cache.join(sha256(url.as_bytes()));
        let fallback = self
            .fallback_cache
            .as_ref()
            .map(|p| p.join(sha256(url.as_bytes())));
        let cached = if path.exists() {
            Some(&path)
        } else {
            fallback.as_ref().filter(|p| p.exists())
        };
        if let Some(cached) = cached {
            let bytes = fs::read(cached)?;
            let meta: serde_json::Value =
                serde_json::from_slice(&fs::read(cached.with_extension("json"))?)?;
            ensure!(
                meta["sha256"].as_str() == Some(&sha256(&bytes)),
                "cache checksum mismatch: {url}"
            );
            return Ok(bytes);
        }
        ensure!(!self.offline, "offline cache miss: {url}");
        // Serialize uncached requests, including cooldowns, across every worker.
        let _network = self.network.lock().expect("network lock");
        ensure!(
            !self.is_rate_limited(),
            "rate limited; retry this run later"
        );
        const MAX_BYTES: u64 = 80 * 1024 * 1024;
        for attempt in 0..6 {
            thread::sleep(Duration::from_millis(750));
            ensure!(
                !self.is_rate_limited(),
                "rate limited; retry this run later"
            );
            let response = self.client.get(url).send();
            let mut response = match response {
                Ok(r) => r,
                Err(_) if attempt < 5 => {
                    thread::sleep(Duration::from_secs(2 << attempt));
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            if response.status().as_u16() == 429 || response.status().is_server_error() {
                let status = response.status();
                let wait = retry_delay(
                    response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok()),
                    attempt,
                    SystemTime::now(),
                );
                if attempt == 5 || wait > Duration::from_secs(300) {
                    self.rate_limited.store(true, Ordering::Relaxed);
                    bail!(
                        "HTTP {status}; rate limited or server unavailable; retry delay {}s, retry budget exhausted or delay exceeds 300s; stopping new requests",
                        wait.as_secs()
                    );
                }
                eprintln!(
                    "HTTP {status}: pausing all network requests for {}s before retry {}/5",
                    wait.as_secs(),
                    attempt + 1
                );
                drop(response);
                // Keep waits interruptible by process termination and print progress on long pauses.
                let mut remaining = wait;
                while !remaining.is_zero() {
                    let chunk = remaining.min(Duration::from_secs(30));
                    thread::sleep(chunk);
                    remaining = remaining.saturating_sub(chunk);
                    if !remaining.is_zero() {
                        eprintln!("Network cooldown: {}s remaining", remaining.as_secs());
                    }
                }
                continue;
            }
            response.error_for_status_ref()?;
            ensure!(
                response.content_length().unwrap_or(0) <= MAX_BYTES,
                "download exceeds 80 MiB"
            );
            let mut bytes = Vec::new();
            (&mut response)
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= MAX_BYTES, "download exceeds 80 MiB");
            fs::write(path.with_extension("part"), &bytes)?;
            fs::rename(path.with_extension("part"), &path)?;
            fs::write(
                path.with_extension("json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "url": url, "sha256": sha256(&bytes),
                    "fetched_unix_seconds": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs()
                }))?,
            )?;
            return Ok(bytes);
        }
        bail!("request failed: {url}")
    }
    pub fn html(&self, url: &str) -> Result<String> {
        Ok(String::from_utf8(self.get(url)?)?)
    }
    pub fn is_rate_limited(&self) -> bool {
        self.rate_limited.load(Ordering::Relaxed)
    }
    pub fn resolve(&self, alias: &str) -> Result<Object> {
        let query_alias = alias.replace(['−', '–', '—'], "-").replace('\u{a0}', " ");
        let query = format!(
            "SELECT b.main_id,b.ra,b.dec,b.otype,b.morph_type,b.nbref FROM basic AS b JOIN ident AS i ON b.oid=i.oidref WHERE i.id='{}'",
            query_alias.replace('\'', "''")
        );
        let mut url = reqwest::Url::parse("https://simbad.cds.unistra.fr/simbad/sim-tap/sync")?;
        url.query_pairs_mut().extend_pairs([
            ("request", "doQuery"),
            ("lang", "adql"),
            ("format", "json"),
            ("query", &query),
        ]);
        #[derive(Deserialize)]
        struct TapRow(
            String,
            Option<f64>,
            Option<f64>,
            String,
            Option<String>,
            Option<u64>,
        );
        #[derive(Deserialize)]
        struct Tap {
            data: Vec<TapRow>,
        }
        let tap: Tap =
            serde_json::from_slice(&self.get(url.as_str())?).context("SIMBAD TAP response")?;
        ensure!(
            tap.data.len() == 1,
            "SIMBAD alias must resolve uniquely: {alias}"
        );
        let TapRow(name, ra, dec, kind, morphology, refs) = tap.data.into_iter().next().unwrap();
        let ra = ra.context("missing RA")?;
        let dec = dec.context("missing Dec")?;
        ensure!(
            ra.is_finite()
                && (0.0..360.0).contains(&ra)
                && dec.is_finite()
                && (-90.0..=90.0).contains(&dec),
            "invalid coordinates"
        );
        // SIMBAD galaxy, active-galaxy and galaxy-system types. Unknown types go to review.
        ensure!(
            [
                "G", "GiP", "GiG", "GiC", "PairG", "GrG", "ClG", "IG", "EmG", "LSB", "bCG", "SBG",
                "H2G", "AGN", "AG?", "PaG", "SyG", "Sy1", "Sy2", "LIN", "QSO", "Bla", "BLL"
            ]
            .contains(&kind.as_str()),
            "non-galaxy or unhandled SIMBAD type {kind}"
        );
        Ok(Object {
            id: name.split_whitespace().collect::<Vec<_>>().join(" "),
            queried_alias: alias.into(),
            ra_deg: ra,
            dec_deg: dec,
            coordinate_frame: "ICRS".into(),
            coordinate_epoch: "J2000 (SIMBAD metadata)".into(),
            object_type: kind,
            morphology,
            bibliography_count: refs.unwrap_or(0),
        })
    }
}

pub struct Page {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub object_type: String,
    pub category: String,
    pub names: Vec<String>,
    pub constellation: String,
    pub credit: String,
    pub credit_html: String,
    pub image_url: String,
    pub width: u32,
    pub height: u32,
    pub ra_text: String,
    pub dec_text: String,
    pub fov: String,
    pub filters: String,
}
fn field(doc: &Html, table: &str, key: &str) -> String {
    let rows = css("tr");
    doc.select(&css("table"))
        .filter(|t| t.value().attr("aria-describedby") == Some(table))
        .flat_map(|t| t.select(&rows))
        .find(|r| r.select(&css("th")).next().is_some_and(|h| text(h) == key))
        .and_then(|r| r.select(&css("td")).next())
        .map(text)
        .unwrap_or_default()
}
pub fn parse_page(html: &str) -> Result<Page> {
    let doc = Html::parse_document(html);
    let links = css("a");
    let id = field(&doc, "About the Image", "Id:");
    ensure!(
        !id.is_empty(),
        "missing image metadata (source layout may have changed)"
    );
    let names: Vec<_> = doc
        .select(&css("table[aria-describedby='About the Object'] tr"))
        .filter(|r| {
            r.select(&css("th"))
                .next()
                .is_some_and(|h| text(h) == "Name:")
        })
        .flat_map(|r| r.select(&links))
        .filter_map(|a| {
            let url = reqwest::Url::parse(a.value().attr("href")?).ok()?;
            url.query_pairs()
                .find(|(k, _)| k.eq_ignore_ascii_case("ident"))
                .map(|(_, v)| v.into_owned())
        })
        .collect();
    let dimensions = field(&doc, "About the Image", "Size:");
    let mut numbers = dimensions
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<u32>().ok());
    let width = numbers.next().context("missing image width")?;
    let height = numbers.next().context("missing image height")?;
    let credit = doc
        .select(&css(".credit"))
        .next()
        .context("missing credit")?;
    let image_url = doc
        .select(&css("a[href]"))
        .find(|a| text(*a) == "Large JPEG")
        .and_then(|a| a.value().attr("href"))
        .unwrap_or_default()
        .to_string();
    if !image_url.is_empty() {
        let url = reqwest::Url::parse(&image_url)?;
        ensure!(
            url.scheme() == "https" && url.host_str() == Some("cdn.esahubble.org"),
            "unexpected image host"
        );
    }
    Ok(Page {
        id,
        title: doc.select(&css("h1")).next().map(text).unwrap_or_default(),
        kind: field(&doc, "About the Image", "Type:"),
        object_type: field(&doc, "About the Object", "Type:"),
        category: field(&doc, "About the Object", "Category:"),
        names,
        constellation: field(&doc, "About the Object", "Constellation:"),
        credit: text(credit),
        credit_html: credit.inner_html(),
        image_url,
        width,
        height,
        ra_text: field(&doc, "Object coordinates", "Position (RA):"),
        dec_text: field(&doc, "Object coordinates", "Position (Dec):"),
        fov: field(&doc, "Object coordinates", "Field of view:"),
        filters: doc
            .select(&css("table[aria-describedby='Colours & filters']"))
            .next()
            .map(text)
            .unwrap_or_default(),
    })
}

pub fn discover_ids(html: &str) -> Vec<String> {
    // The archive embeds a JavaScript gallery, so <a> traversal alone misses images.
    let re =
        regex::Regex::new(r#"(?:url:\s*['"]|href=['"])/images/([a-z]+[0-9]+[a-z0-9_-]*)/['"]"#)
            .unwrap();
    let mut ids: Vec<_> = re.captures_iter(html).map(|c| c[1].to_owned()).collect();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_after_supports_seconds_dates_and_backoff() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(retry_delay(Some("42"), 0, now).as_secs(), 42);
        let date = httpdate::fmt_http_date(now + Duration::from_secs(90));
        assert_eq!(retry_delay(Some(&date), 0, now).as_secs(), 90);
        assert_eq!(retry_delay(Some("invalid"), 1, now).as_secs(), 10);
        assert_eq!(retry_delay(None, 0, now).as_secs(), 5);
        let past = httpdate::fmt_http_date(now - Duration::from_secs(60));
        assert_eq!(retry_delay(Some(&past), 0, now).as_secs(), 5);
    }

    #[test]
    fn rate_limit_retries_with_contact_user_agent_and_caches_success() {
        use std::{io::Write, net::TcpListener, time::Instant};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/test", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for status in ["429 Too Many Requests", "200 OK"] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(15)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                assert!(String::from_utf8(request).unwrap().contains(USER_AGENT));
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: 2\r\nRetry-After: 0\r\nConnection: close\r\n\r\nok").unwrap();
            }
        });
        let cache = std::env::temp_dir().join(format!(
            "galaxy-retry-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let fetch = Fetcher::new(cache.clone(), false).unwrap();
        let started = Instant::now();
        assert_eq!(fetch.get(&url).unwrap(), b"ok");
        assert!(started.elapsed() >= Duration::from_secs(5));
        assert!(!fetch.is_rate_limited());
        server.join().unwrap();
        assert_eq!(fetch.get(&url).unwrap(), b"ok");
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn discovers_js_gallery_and_links_without_duplicates() {
        assert_eq!(
            discover_ids(
                "url: '/images/potw1945a/', href=\"/images/potw1945a/\" url: '/images/archive/'"
            ),
            ["potw1945a"]
        );
    }
    #[test]
    fn refuses_unrecognized_page_instead_of_empty_record() {
        assert!(parse_page("<html>maintenance</html>").is_err());
    }
    #[test]
    fn metadata_parser_keeps_identifiers_credits_and_image_center_separate() {
        let page = parse_page(r#"
            <h1>Example pair</h1>
            <table aria-describedby="About the Image">
              <tr><th>Id:</th><td>test123a</td></tr><tr><th>Type:</th><td>Observation</td></tr>
              <tr><th>Size:</th><td>2200 x 1200 px</td></tr>
            </table>
            <table aria-describedby="About the Object">
              <tr><th>Name:</th><td><a href="https://simbad.cds.unistra.fr/simbad/sim-id?Ident=NGC+1">One</a>
                <a href="https://simbad.cds.unistra.fr/simbad/sim-id?Ident=NGC+2">Two</a></td></tr>
              <tr><th>Constellation:</th><td>Aries</td></tr>
              <tr><th>Category:</th><td>Galaxies</td></tr>
            </table>
            <table aria-describedby="Object coordinates"><tr><th>Position (RA):</th><td>1 2 3</td></tr></table>
            <div class="credit"><a href="https://example.org">A</a> &amp; B</div>
            <a href="https://cdn.esahubble.org/archives/images/large/test123a.jpg">Large JPEG</a>
        "#).unwrap();
        assert_eq!(page.names, ["NGC 1", "NGC 2"]);
        assert_eq!(page.credit, "A & B");
        assert_eq!(page.ra_text, "1 2 3");
        assert_eq!(page.width, 2200);
        assert_eq!(page.category, "Galaxies");
        assert!(page.object_type.is_empty());
    }
}
