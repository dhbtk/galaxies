//! Reproducible NED distance enrichment, kept separate from the image review snapshot.
use crate::source::{Fetcher, sha256};
use anyhow::{Context, Result, bail, ensure};
use galaxy_catalog::{Object, Record};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

const SOURCE: &str = "NED OverviewOfObject VOTable API";
const POLICY: &str = "ned-distance-v3-catalog-number-normalization";
const MPC_TO_MLY: f64 = 3.261_563_777;

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    source: String,
    policy: String,
    candidates_sha256: String,
    objects: BTreeMap<String, Estimate>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Estimate {
    status: String,
    distance_mpc: Option<f64>,
    distance_million_ly: Option<f64>,
    uncertainty_mpc: Option<f64>,
    method: Option<String>,
    redshift: Option<f64>,
    source_url: String,
    source_cross_ids: Option<String>,
    separation_arcsec: Option<f64>,
    note: String,
}

fn empty(status: &str, url: &str, note: &str) -> Estimate {
    Estimate {
        status: status.into(),
        distance_mpc: None,
        distance_million_ly: None,
        uncertainty_mpc: None,
        method: None,
        redshift: None,
        source_url: url.into(),
        source_cross_ids: None,
        separation_arcsec: None,
        note: note.into(),
    }
}

fn number(value: Option<&str>) -> Option<f64> {
    value?.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

fn normalize_name(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect();
    for prefix in ["NGC", "IC", "UGC", "PGC", "LEDA", "MESSIER", "ARP"] {
        if let Some(tail) = cleaned.strip_prefix(prefix)
            && tail.starts_with('0')
        {
            return format!("{prefix}{}", tail.trim_start_matches('0'));
        }
    }
    cleaned
}

fn separation_arcsec(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    let (a, b) = (dec1.to_radians(), dec2.to_radians());
    let dra = (ra1 - ra2).to_radians();
    let cos = (a.sin() * b.sin() + a.cos() * b.cos() * dra.cos()).clamp(-1.0, 1.0);
    cos.acos().to_degrees() * 3600.0
}

fn comoving_distance_mpc(redshift: f64) -> Option<f64> {
    if !(0.1..=20.0).contains(&redshift) {
        return None;
    }
    let steps = 2048;
    let width = redshift / steps as f64;
    let inverse_expansion = |z: f64| 1.0 / (0.308 * (1.0 + z).powi(3) + 0.692).sqrt();
    let mut weighted = inverse_expansion(0.0) + inverse_expansion(redshift);
    for i in 1..steps {
        weighted += (if i % 2 == 0 { 2.0 } else { 4.0 }) * inverse_expansion(i as f64 * width);
    }
    Some(299_792.458 / 67.8 * weighted * width / 3.0)
}

fn parse_overview(xml: &str, object: &Object, url: &str) -> Result<Estimate> {
    let doc = roxmltree::Document::parse(xml).context("NED VOTable XML")?;
    if let Some(status) = doc.descendants().find(|n| {
        n.is_element()
            && n.tag_name().name() == "PARAM"
            && n.attribute("name") == Some("QUERY_STATUS")
            && n.attribute("value") == Some("ERROR")
    }) {
        let description = status
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "DESCRIPTION")
            .and_then(|n| n.text())
            .unwrap_or("NED query failed")
            .trim();
        if description.contains("Failed to resolve input object name") {
            return Ok(empty("not_found", url, description));
        }
        bail!("NED query error: {description}");
    }
    let table = doc
        .descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "TABLE"
                && n.attribute("ID") == Some("allbyname")
        })
        .context("NED overview table missing")?;
    let fields: Vec<_> = table
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "FIELD")
        .map(|n| n.attribute("name").unwrap_or(""))
        .collect();
    let Some(row) = table
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "TR")
    else {
        return Ok(empty("not_found", url, "NED returned no object"));
    };
    let cells: Vec<_> = row
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "TD")
        .map(|n| n.text())
        .collect();
    ensure!(
        fields.len() == cells.len(),
        "NED VOTable field count mismatch"
    );
    let values: BTreeMap<_, _> = fields.into_iter().zip(cells).collect();
    let get = |name: &str| values.get(name).copied().flatten();
    let ra = number(get("Lon (Equatorial J2000)"));
    let dec = number(get("Lat (Equatorial J2000)"));
    let cross_ids = get("Cross-identifications").unwrap_or("");
    let names_match = cross_ids
        .split(';')
        .map(normalize_name)
        .any(|id| id == normalize_name(&object.id) || id == normalize_name(&object.queried_alias));
    let separation = ra
        .zip(dec)
        .map(|(r, d)| separation_arcsec(r, d, object.ra_deg, object.dec_deg));
    let mut result = empty(
        "identity_unverified",
        url,
        "NED and SIMBAD identity could not be confirmed",
    );
    result.source_cross_ids = Some(cross_ids.into());
    result.separation_arcsec = separation;
    // A matching catalog name tolerates different centers for extended galaxies.
    // Without a shared name, only a near-exact position is considered safe.
    if !separation.is_some_and(|s| s <= 60.0 && (names_match || s <= 2.0)) {
        return Ok(result);
    }
    result.redshift = number(get("Redshift"));
    let mean = number(get("Mean Distance")).filter(|d| *d > 0.0);
    let hubble = number(get("D (3K CMB)")).filter(|d| *d > 0.0);
    let (distance, uncertainty, method, note) = if let Some(d) =
        result.redshift.and_then(comoving_distance_mpc)
    {
        (
            d,
            None,
            "comoving_redshift_model_h0_67_8_omegam_0_308",
            "Model-derived line-of-sight comoving distance from heliocentric redshift; not light-travel time",
        )
    } else if let Some(d) =
        mean.filter(|d| *d <= 200.0 || result.redshift.is_none_or(|z| z <= 0.01))
    {
        (
            d,
            number(get("SEM Distance")),
            "redshift_independent_mean",
            "NED mean of published redshift-independent distances; methods can differ",
        )
    } else if let Some(d) = hubble {
        // A nearby galaxy's peculiar motion can dominate its redshift. Leave those blank.
        if result.redshift.is_none_or(|z| z <= 0.01) {
            result.status = "no_reliable_distance".into();
            result.note = "Only a low-redshift Hubble-flow estimate is available".into();
            return Ok(result);
        }
        (
            d,
            number(get("D Unc (3K CMB)")),
            "hubble_flow_cmb_h0_67_8",
            "Approximate CMB-frame Hubble-flow distance; not a direct measurement or light-travel distance",
        )
    } else {
        result.status = "no_distance".into();
        result.note = "NED overview has no usable distance".into();
        return Ok(result);
    };
    result.status = "available".into();
    result.distance_mpc = Some(distance);
    result.distance_million_ly = Some(distance * MPC_TO_MLY);
    result.uncertainty_mpc = uncertainty.filter(|u| *u >= 0.0);
    result.method = Some(method.into());
    result.note = note.into();
    Ok(result)
}

fn save(path: &Path, snapshot: &Snapshot) -> Result<()> {
    let part = path.with_extension("json.part");
    fs::write(&part, serde_json::to_vec_pretty(snapshot)?)?;
    fs::rename(part, path)?;
    Ok(())
}

pub fn run(data: &Path, output: &Path, limit: Option<usize>, offline: bool) -> Result<()> {
    let bytes = fs::read(data.join("candidates.json"))?;
    let hash = sha256(&bytes);
    let records: Vec<Record> = serde_json::from_slice(&bytes)?;
    let mut objects = BTreeMap::new();
    for record in records {
        for object in record.objects {
            objects.entry(object.id.clone()).or_insert(object);
        }
    }
    let parent = output
        .parent()
        .context("distance output needs parent directory")?;
    fs::create_dir_all(parent)?;
    let mut snapshot = if output.exists() {
        let old: Snapshot = serde_json::from_slice(&fs::read(output)?)?;
        ensure!(
            old.candidates_sha256 == hash,
            "catalog changed; use a new distance output path"
        );
        ensure!(
            old.policy == POLICY,
            "distance policy changed; use a new output path"
        );
        old
    } else {
        Snapshot {
            source: SOURCE.into(),
            policy: POLICY.into(),
            candidates_sha256: hash,
            objects: BTreeMap::new(),
        }
    };
    let fetch = Fetcher::new(parent.join("cache"), offline)?;
    let mut processed = 0;
    let mut failures = 0;
    for (id, object) in &objects {
        if snapshot.objects.contains_key(id) {
            continue;
        }
        if processed >= limit.unwrap_or(usize::MAX) {
            break;
        }
        processed += 1;
        let mut url =
            reqwest::Url::parse("https://ned.ipac.caltech.edu/NED::API/OverviewOfObject")?;
        url.query_pairs_mut()
            .append_pair("TARGET", id)
            .append_pair("HCONST", "67.8")
            .append_pair("OMEGAM", "0.308")
            .append_pair("OMEGAV", "0.692");
        match fetch
            .get(url.as_str())
            .and_then(|raw| parse_overview(std::str::from_utf8(&raw)?, object, url.as_str()))
        {
            Ok(estimate) => {
                eprintln!(
                    "[{}/{}] {id}: {}",
                    snapshot.objects.len() + 1,
                    objects.len(),
                    estimate.status
                );
                snapshot.objects.insert(id.clone(), estimate);
                save(output, &snapshot)?;
            }
            Err(error) => {
                failures += 1;
                eprintln!("{id}: {error:#}");
                if fetch.is_rate_limited() {
                    break;
                }
            }
        }
    }
    println!(
        "{} of {} objects checked; {} have distances; {} failed in this run. {}",
        snapshot.objects.len(),
        objects.len(),
        snapshot
            .objects
            .values()
            .filter(|e| e.status == "available")
            .count(),
        failures,
        output.display()
    );
    if failures > 0 {
        bail!("distance fetch incomplete; rerun to resume");
    }
    Ok(())
}

pub fn apply(data: &Path, distances: &Path) -> Result<()> {
    let bytes = fs::read(distances)?;
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    ensure!(snapshot.policy == POLICY, "unsupported distance policy");
    ensure!(
        snapshot.candidates_sha256 == sha256(&fs::read(data.join("candidates.json"))?),
        "distance snapshot belongs to a different candidate catalog"
    );
    let mut db = Connection::open(data.join("catalog.sqlite"))?;
    let object_ids = {
        let mut statement = db.prepare("SELECT id FROM objects ORDER BY id")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    ensure!(
        object_ids.len() == snapshot.objects.len()
            && object_ids
                .iter()
                .all(|id| snapshot.objects.contains_key(id)),
        "distance snapshot is incomplete for this catalog"
    );
    db.execute_batch(include_str!("schema.sql"))?;
    let tx = db.transaction()?;
    tx.execute("DELETE FROM object_distances", [])?;
    for (id, estimate) in &snapshot.objects {
        tx.execute(
            "INSERT INTO object_distances VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                id,
                estimate.status,
                estimate.distance_mpc,
                estimate.distance_million_ly,
                estimate.uncertainty_mpc,
                estimate.method,
                estimate.redshift,
                estimate.source_url,
                serde_json::to_string(estimate)?
            ],
        )?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO run_metadata VALUES ('distance_policy',?1)",
        [POLICY],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO run_metadata VALUES ('distance_snapshot_sha256',?1)",
        [sha256(&bytes)],
    )?;
    tx.commit()?;
    println!(
        "Joined {} distance statuses into {}",
        snapshot.objects.len(),
        data.join("catalog.sqlite").display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn m74_overview_prefers_independent_distance() {
        let o = Object {
            id: "M 74".into(),
            queried_alias: "Messier 74".into(),
            ra_deg: 24.173938,
            dec_deg: 15.783641,
            coordinate_frame: "ICRS".into(),
            coordinate_epoch: "J2000".into(),
            object_type: "G".into(),
            morphology: None,
            bibliography_count: 0,
        };
        let xml = r#"<VOTABLE><RESOURCE><TABLE ID="allbyname"><FIELD name="Cross-identifications"/><FIELD name="Lon (Equatorial J2000)"/><FIELD name="Lat (Equatorial J2000)"/><FIELD name="Redshift"/><FIELD name="Mean Distance"/><FIELD name="SEM Distance"/><FIELD name="D (3K CMB)"/><DATA><TABLEDATA><TR><TD>Messier 074; MESSIER 074; NGC 0628</TD><TD>24.1741387</TD><TD>15.7836868</TD><TD>0.002192</TD><TD>7.495</TD><TD>0.53</TD><TD>5.29</TD></TR></TABLEDATA></DATA></TABLE></RESOURCE></VOTABLE>"#;
        let value = parse_overview(xml, &o, "test").unwrap();
        assert_eq!(value.method.as_deref(), Some("redshift_independent_mean"));
        assert_eq!(value.distance_mpc, Some(7.495));
        assert!(value.distance_million_ly.unwrap() > 24.0);
    }
    #[test]
    fn distant_redshift_uses_cosmology_and_local_redshift_does_not() {
        assert!(comoving_distance_mpc(0.001).is_none());
        let d = comoving_distance_mpc(1.0).unwrap();
        assert!((3000.0..4000.0).contains(&d));
    }

    #[test]
    fn unresolved_ned_name_is_a_missing_result() {
        let object = Object {
            id: "6C 073759+311929".into(),
            queried_alias: "6C 073759+311929".into(),
            ra_deg: 0.0,
            dec_deg: 0.0,
            coordinate_frame: "ICRS".into(),
            coordinate_epoch: "J2000".into(),
            object_type: "G".into(),
            morphology: None,
            bibliography_count: 0,
        };
        let xml = r#"<VOTABLE><RESOURCE><PARAM name="QUERY_STATUS" value="ERROR"><DESCRIPTION>GeneralFault: Service could not complete request; Failed to resolve input object name (6)</DESCRIPTION></PARAM></RESOURCE></VOTABLE>"#;
        assert_eq!(
            parse_overview(xml, &object, "test").unwrap().status,
            "not_found"
        );
    }

    #[test]
    fn catalog_zero_padding_does_not_break_identity() {
        assert_eq!(normalize_name("NGC 0660"), normalize_name("NGC 660"));
        assert_eq!(normalize_name("Messier 074"), normalize_name("Messier 74"));
        assert_ne!(normalize_name("2MASS J0012"), normalize_name("2MASS J012"));
    }
}
