# Wikipedia supplement

The scraper discovers article links only from the galaxy sections of constellation navigation templates, freezes their template revision IDs, then reads each article's infobox and subject galleries. `config/wikipedia.json` contains 951 sorted article seeds across the 12 zodiac constellations. A navbox entry is a discovery lead, not a guarantee of eligibility.

## Resume the current catalog

Run from the workspace root:

```sh
cargo run --release --locked -p galaxy-scraper -- wikipedia-run \
  --manifest config/wikipedia.json \
  --data crates/galaxy-scraper/data-wikipedia \
  --base crates/galaxy-scraper/data
```

The original ESA directory is read-only input. The separate output contains both sources, SQLite, JSON, images and credits. Successful Wikipedia candidates are checkpointed after each acceptance. Repeating this command reuses validated downloads and cached responses. Default concurrency is one worker; requests are paced. The User-Agent is `GalaxyCollageBot/0.1 (mailto:galaxy-bot@dianahorbatiuk.com)`. HTTP 429 and server errors pause all workers and retry automatically, honoring numeric or HTTP-date `Retry-After` values with a minimum exponential backoff of 5 seconds. After five retries, or a requested delay above 300 seconds, further uncached requests stop for that run. Remaining inputs are deferred, not rejected as unsuitable. The command exits nonzero after saving partial results when there are failures or deferrals. Cooldowns apply within the running process; an exhausted run is not automatically scheduled again.

Append `--offline` to rebuild without network access. Add `--limit 6` with `config/wikipedia-smoke.json` and a separate output directory for a bounded smoke test.

To freeze a new discovery snapshot:

```sh
cargo run --release --locked -p galaxy-scraper -- wikipedia-discover \
  --output config/wikipedia.json \
  --cache crates/galaxy-scraper/data-wikipedia/discovery-cache
```

Use a new manifest/output location if preserving an existing snapshot matters. Export a portable bundle with:

```sh
cargo run --release --locked -p galaxy-scraper -- pack \
  --data crates/galaxy-scraper/data-wikipedia --output data/galaxies-wikipedia.zip
```

## Acceptance and provenance

- At least 720 pixels on either axis, checked against both source metadata and decoded bytes; no upscaling. JPEG and PNG are supported.
- Color is estimated from channel differences in a small decoded preview. This does not establish visual quality; every candidate still needs visual review.
- Prefer spirals, then distinctive subjects, then SIMBAD reference counts, using deterministic tie-breaking and up to 50 selections per constellation. Nonspirals remain eligible.
- Wikipedia constellation labels must match the navbox. SIMBAD supplies galaxy identity and ICRS RA/Dec; unresolved or ambiguous identities are reported. IAU boundary verification is still pending.
- Accept explicit compatible CC BY, CC BY-SA, CC0 or public-domain metadata; reject nonfree, NC, ND, missing/conflicting licenses and files with restrictions. Keep creator credit, license URL, file description, raw license metadata and source revisions. Share-alike and attribution obligations remain attached to the image.
- Ask MediaWiki for a standard 1280-pixel-width derivative; reuse already cached original downloads. Preserve actual decoded dimensions, downloaded-byte SHA-256, original URL and original-file SHA-1. Images may be smaller than the requested derivative size.

References: [MediaWiki image metadata API](https://www.mediawiki.org/wiki/API:Imageinfo), [standard thumbnail sizes](https://www.mediawiki.org/wiki/Common_thumbnail_sizes), and [Aquarius navigation template](https://en.wikipedia.org/wiki/Template:Aquarius_(constellation)).

## Current merged snapshot — 2026-10-06

The merged output currently contains 786 candidate records: 152 ESA and 634 Wikipedia. It contains 780 distinct photos and 292 provisional selections. All original ESA candidates remain in the merged catalog. The latest run predates the review set, and the review set freezes this candidate snapshot. Counts by constellation and unresolved crawl inputs are recorded in `data-wikipedia/report.json`.

## Visual review set

The existing merged output is at `crates/galaxy-scraper/data-wikipedia`. Generate a separate browseable review set once, using `python3 scripts/prepare_review.py`. Open `crates/galaxy-scraper/review/index.html` in a browser, then inspect the folders under `review/images/` in Finder. Each distinct photo has one file named with its subject and a short content hash. A photo shared by two or more catalog records appears once; `review/manifest.json` lists all linked record IDs.

Delete review image files that are unsuitable. Do not delete the manifest or the source `data-wikipedia/images` files. The review photos are independent copies; deleting them records your decision without losing the downloaded original. The script refuses to run again into an existing review directory so later runs cannot silently restore rejected photos. After review, run `cargo run --locked -p galaxy-scraper -- review-apply` to make `data-curated`. This verifies the manifest and retained image hashes, removes every record linked only to a deleted photo, removes orphan objects from SQLite, and recalculates selections. The current review produced 349 retained records, 285 objects, and 213 provisional selections; 437 records were removed. The original merged output and review manifest remain available. Use a new output path for a later review revision.
