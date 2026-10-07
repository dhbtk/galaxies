# Initial prototype verification

These are bounded smoke runs, not coverage estimates for the complete archive.

## Live results

| Input | Image | Resolved entities | Source constellation | Decoded pixels |
| --- | --- | --- | --- | --- |
| Curated smoke manifest | potw1108a | NGC 6384 | Ophiuchus (opt-in) | 3871×1836 |
| Curated smoke manifest | potw1945a | NGC 772 | Aries | 2858×2841 |
| Curated smoke manifest | potw2538a | NGC 2775 | Cancer | 4150×3879 |
| First ranked archive page | heic0719a | M 74 | Pisces | 4014×3865 |
| First ranked archive page | heic1503a | APG 284, NGC 7714 | Pisces | 3269×2240 |
| First ranked archive page | opo0328a | M 104 | Virgo | 11472×6429 |
| First ranked archive page | potw1921a | M 59 | Virgo | 3762×3000 |
| First ranked archive page | potw2114a | M 61 | Virgo | 4077×3978 |

The smoke manifest's fourth image, HCG 87 (`opo9931a`), was excluded at 1500×1470 pixels. The 50-image discovered manifest yielded five candidates, 44 out-of-scope exclusions, and one unresolved entry (`potw1822a`, SDSS J1156+1911, with no linked SIMBAD identifier). That run correctly exited nonzero while retaining its partial catalog and error report.

The eight candidate images above are not eight reviewed, approved collage assets. All remain provisional. NGC 772 was visually inspected: it is a detailed color close-up, not a full-galaxy silhouette. This confirms why cropping/subject coverage must be evaluated separately from pixel dimensions. M 59 also demonstrates that non-spiral galaxies remain in the candidate pool at lower priority when there is spare capacity.

## Checks completed

- Six Rust tests passed: image thresholds/orientation, exact constellation matching, stable selection and deduplication/caps, gallery discovery, metadata parsing, malformed-page rejection.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` passed.
- `cargo fmt --all --check` passed.
- Live discovery returned 50 distinct image IDs; ingestion exercised real SIMBAD responses and actual downloaded JPEGs.
- Offline replay of the smoke manifest produced byte-identical `selected.json`.
- Both SQLite databases passed integrity and foreign-key checks.
- Both ZIP bundles passed CRC checks; every bundled candidate image matched its stored SHA-256.

Generated files are ignored by Git and currently live in `data/demo/`, `data/archive-sample/`, `data/galaxies.zip` and `data/archive-sample.zip`. JSON contains full source URLs and credits. Keep the cache directories for replay; they are deliberately not included in the distribution ZIPs.
