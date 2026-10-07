# Galaxy collage

A Rust workspace for building a reproducible image catalog for a future birth-chart galaxy collage.

- `galaxy-catalog`: shared data model, image-size policy, deterministic ranking and selection.
- `galaxy-scraper`: ESA/Hubble and Wikipedia discovery, SIMBAD coordinate resolution, image downloads, SQLite/JSON output and ZIP packaging.

See [Wikipedia ingestion and visual review](docs/wikipedia.md) for the supplementary catalog and curated snapshot, and [distance enrichment](docs/distances.md) for object distances.

Read [the research and product plan](docs/plan.md) before expanding the dataset. This is a working ingestion prototype, not a complete catalog or birth-chart calculator.

See [initial live results and verification](docs/prototype-results.md) for the bounded sample, exclusions and known unresolved entry.

## Run

Rust 2024 edition; developed and checked with Rust 1.96. Keep `Cargo.lock` with the project.

```sh
# Small curated smoke sample; Ophiuchus is opt-in.
cargo run --locked -p galaxy-scraper -- run --include-ophiuchus

# Discover IDs automatically from one ranked archive page (~50 images).
cargo run --locked -p galaxy-scraper -- discover --pages 1
cargo run --locked -p galaxy-scraper -- run \
  --manifest data/discovered.json --data data/archive-sample

# Rebuild from the same snapshot, with no network access.
cargo run --locked -p galaxy-scraper -- run --include-ophiuchus --offline

# SQLite + JSON + originals + attribution, in a portable ZIP.
cargo run --locked -p galaxy-scraper -- pack

cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

`run` defaults to `config/demo.json`, `data/demo`, and a ceiling of 50 collage tiles per constellation. It counts one pair/system image as one tile, even if several catalog objects are associated with it. It is not the discovery algorithm.

Start with a bounded crawl. More `--pages` expands discovery; the input list is sorted and frozen. A partial archive crawl is not a complete constellation census. Network requests are sequential, cached, paced and retried; downloads are limited to 80 MiB. A failed record is reported and the run exits nonzero after saving partial results. Use the report to distinguish absence, exclusion, and failure.

## Outputs

Under the chosen data directory:

| File | Purpose |
| --- | --- |
| `catalog.sqlite` | Objects, images, many-to-many associations, provisional selections, run metadata |
| `input.json` | Frozen discovery/seed manifest |
| `candidates.json` | Full metadata for images passing automatic gates |
| `selected.json` | Deterministic provisional subset, capped per constellation |
| `report.json` | Counts (including zeros), exclusions/errors, input hash and policy version |
| `images/<sha256>.jpg` or `.png` | Downloaded image bytes, named by content hash |
| `CREDITS.txt` | Source attribution for all downloaded candidates |
| `cache/` | Source responses, request URLs, retrieval times and checksums |

SQLite contains queryable columns and complete per-image JSON. Image metadata includes dimensions measured by decoding, source dimensions, credit and original credit HTML, license/policy links, filters, SHA-256 and local path. Galaxy RA/Dec are decimal degrees in ICRS from SIMBAD; image-center coordinates remain separate source strings. `objects` includes systems such as galaxy pairs, not only individual galaxies.

Cached responses are reused without automatic refresh. Use a **new data directory** for a fresh upstream snapshot; retain the old directory for reproduction. `run` rebuilds the database and JSON projections from its manifest. Do not manually edit generated output. ZIP export is for distribution; keep the cache separately for offline reproduction. Birth data never enters this scraper.

## Prototype limits

- Constellations currently come from explicit archive labels or matching Wikipedia article/navbox labels, **not a computed IAU boundary lookup**. Records say so. Boundary verification is the next astronomy milestone.
- Every selection remains pending visual, identity and license review. Pixel dimensions and the color heuristic do not prove sharpness, good framing, or that the entire subject is visible.
- General ESA/Hubble CC BY 4.0 policy is recorded as a default; image-specific exceptions still need review. Preserve credit links when publishing; do not insert raw source HTML into a future site without sanitization.
- The rank prioritizes spirals, then distinctive subjects, then SIMBAD reference counts. Images must have at least 720 pixels on either axis; Wikipedia images also pass a color heuristic. Reference counts measure scientific attention, not public popularity. Caption-only features and catalog membership are not yet scored.
- SIMBAD names come from archive links or Wikipedia titles and infobox aliases. Ambiguous/missing names and unhandled object types become reported errors; pairs are not silently reduced to one resolved component.
- No all-archive crawl, curation overrides, perceptual duplicate detection, constellation transforms, crop-quality estimator or birth-chart mapping yet.
- The web service lives in `crates/web`; catalog ingestion remains independent of the site.
