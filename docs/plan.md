# Galaxy catalog and birth-chart collage plan

Historical initial plan: the resolution and ranking policy below is superseded by the [Wikipedia supplement](wikipedia.md), which uses a 720-pixel long-edge minimum and prioritizes spirals.

## Recommendation

Build an image-led, reproducible catalog, with an explicit review layer. Start from editorially selected observatory imagery; resolve objects against an astronomical catalog; rank within real constellations; retain up to 50 eligible visual subjects per constellation. A subject may be one galaxy or an interesting pair/system. Do not promise 600 qualifying subjects before a coverage audit.

“Notable” has no universal catalog field. The deterministic part is a published rule applied to frozen inputs, not an assertion that taste is objective. Keep candidate discovery, scientific identity, visual suitability, ranking, and collage mapping as separate decisions.

## Sources, checked during this prototype

| Source | Role | Approach |
| --- | --- | --- |
| [ESA/Hubble galaxy archive](https://esahubble.org/images/archive/category/galaxies/) | Primary editorial discovery and color images | Crawl bounded archive pages and structured image metadata. No public image API was confirmed; do not invent an endpoint. The current gallery embeds image URLs in JavaScript. |
| [SIMBAD TAP](https://simbad.cds.unistra.fr/simbad/sim-tap) | Canonical object identity, RA/Dec, morphology, bibliography count | ADQL over HTTP. The prototype successfully queried `basic` joined to `ident`. Preserve responses and query URLs. [Query documentation](https://astroquery.readthedocs.io/en/stable/simbad/simbad.html). |
| [ESA/Webb](https://esawebb.org/images/) | Later source for visually distinctive infrared composites | Separate adapter and policy review; record instrument and wavelength provenance. |
| [ESO](https://www.eso.org/public/images/) | Later wider-field optical imagery | Useful when Hubble crops away the outer galaxy or companion; separate source adapter. |
| [NASA Image and Video Library](https://api.nasa.gov/) | Later documented API fallback | Search and asset-manifest endpoints exist; identity and per-image rights still need validation. |
| Wikimedia Commons | Optional later gap filling | Resolve specific objects, then inspect file metadata and licenses. Do not infer rights from a search thumbnail. |

Prefer primary observatory files over search-engine thumbnails. [ESA/Hubble formats](https://esahubble.org/press/image_formats/) distinguish screensize from full-size downloads. [Its reuse policy](https://esahubble.org/copyright/) generally uses CC BY 4.0 and requires the complete credit; retain both the policy snapshot and the image's credit. This should travel with any exported collage.

## Discovery and reproducibility

1. Snapshot source archive listings and freeze discovered image IDs. Archive feature selection is the initial notability proxy; a ranked archive page is not a formal popularity survey.
2. Parse observation type, object names, explicit constellation label, morphology, dimensions, downloads, filters and attribution. Exclude illustrations, charts, annotated variants and non-galaxy objects. Fail visibly if the source layout changes.
3. Resolve names to canonical astronomical entities, preserving all image-to-object associations. Do not deduplicate galaxies by display name or merge a pair into one component. Foreground/background coincidences are not automatically physical pairs.
4. Store galaxy coordinates independently of image-center coordinates. Preserve frame, source and epoch metadata; do not call ICRS and FK5/J2000 interchangeable. RA is degrees in storage, not hours. Missing measurements are null, never fabricated zeroes.
5. Verify constellation membership from coordinates against official IAU boundaries. The established method precesses to B1875 and applies the Roman/Delporte boundary table ([Astropy reference](https://docs.astropy.org/en/stable/api/astropy.coordinates.get_constellation.html)). Implement or adopt a validated Rust transform and boundary lookup, compare against Astropy fixtures, and test poles, wraparound and boundaries. Until then the archive label is explicitly provisional.
6. Decode downloads and validate dimensions and color content. Keep native pixels: never upscale a tiny survey cutout to pass. Preserve originals and generate derivatives separately.
7. Rank all eligible subjects in each constellation, select at most 50, and produce a coverage report with exclusions and unresolved cases. Review visual suitability before publishing a catalog release.

Reproducible identity: input-manifest hash + cached source response hashes + selection-policy version + review-overrides hash + locked code/dependencies. Live catalog values and source pages may change. Dataset releases must be immutable; a collage stores the dataset version and mapping version. Offline logical output should match; raw SQLite bytes need not match after rebuilding because page allocation is an implementation detail.

## Image acceptance and ranking

Proposed 1080p interpretation: **short side at least 1080 pixels and long side at least 1920**, in either orientation. This accepts square and portrait photography without forcing a 16:9 crop. It deliberately rejects a 1500×1470 image despite its respectable appearance. If square tiles are the final design, a separate 1080×1080 tier may be more useful; do not quietly mix thresholds.

The prototype checks decoded dimensions and a small fraction of genuinely colored pixels. Composite colors, including infrared assigned to visible channels, are acceptable but must not be described as naked-eye “true color.” Colorfulness is a heuristic, not a final quality judgment.

The publication stage also needs a reviewed subject bounding box/crop, adequate subject pixel coverage, sharpness, no unwanted labels, and no misleading image/object match. A 4000-pixel frame with a 30-pixel galaxy is unsuitable. Ring galaxies and gravitational Einstein rings should have distinct tags.

Initial transparent score, implemented as `prototype-v1`:

```text
20,000 if structured metadata says ring / interacting / merger / pair / peculiar
+ 10,000 if structured metadata says spiral
+ min(max scientific reference count across associated objects, 9,999)
tie-break: source image ID, ascending
```

All eligible galaxy candidates can enter the provisional pool; morphology affects priority. A final strict morphology filter should follow reviewed tags, rather than throwing out interesting galaxies because metadata is sparse. Add Messier/Caldwell/Arp/Hickson membership and public-attention metrics only as explicit, separately recorded evidence. Do not use image count as popularity: prolific photography would dominate.

Future final selection should reserve some capacity for rare visual classes, avoid near-identical crops, and preserve variety. The prototype only prevents repeated canonical objects and exact image hashes. Independent images of different components of the same system will need a shared `visual_subject_id` to avoid repetition. Store reviewer overrides in a version-controlled file with reason and provenance; no silent database edits.

## Coverage constraints

The initial 12 groups are Aries, Taurus, Gemini, Cancer, Leo, Virgo, Libra, Scorpius, Sagittarius, Capricornus, Aquarius and Pisces. Ophiuchus is optional and separate. Galaxy abundance, foreground dust, source coverage and image quality vary; expect uneven counts, but measure before giving estimates.

The smoke sample includes NGC 772 in Aries, NGC 2775 in Cancer, HCG 87 in Capricornus and NGC 6384 in Ophiuchus. HCG 87's checked source image is only 1500×1470 and should be excluded by this prototype's rule. This illustrates why a famous name does not guarantee an eligible image.

An empty group is a product decision, not a scraper failure. Options: show an explicit unavailable tile; relax to a documented lower-resolution tier; expand to other observed galaxy morphologies; or offer a separate all-sky artistic pool. Do not silently assign a galaxy from another constellation while labeling it as local. Even iconic ring/pair objects may lie outside the 12 desired constellations.

## Mapping a birth chart to galaxies

In tropical astrology, signs are twelve equal 30-degree ecliptic-longitude sectors, anchored at the March equinox. They are not local horizon angles or the irregular physical constellations. The horizon matters for Ascendant and house calculations. Precession is part of the discrepancy, but the unequal boundaries mean no single fixed angular offset can align all twelve. [Royal Observatory of Belgium explanation](https://robinfo.oma.be/en/astro-info/sun/passing-of-the-sun-through-the-zodiac-signs-in-2026/).

Keep three product modes conceptually separate:

| Mode | How a pool is chosen | Meaning |
| --- | --- | --- |
| Sign-inspired (recommended initial mode) | Tropical Aries selects the physical Aries constellation's catalog, and so on | An explicitly artistic association that preserves familiar signs |
| Actual sky | Transform each planet's full sky direction into the catalog frame and compute its IAU constellation | Physical sky membership; allow Ophiuchus and potentially other constellations because planets have ecliptic latitude |
| All-sky longitude | Compare common-frame ecliptic longitude/latitude of planets and galaxies | Better pool coverage, but no longer the requested constellation grouping |

For the initial mode, use planet longitude to derive sign and within-sign fraction. Choose a galaxy using weighted rendezvous hashing over `(mapping_version, dataset_version, canonical_chart, planet_id, subject_id)`. One formulation is to minimize `-ln(U)/weight`, with `U` derived from a fixed cryptographic hash and mapped strictly inside (0,1). Use bounded weights so the most referenced galaxy does not appear in every collage. Use a specified digest/byte order and fixed canonical encoding, not Rust's randomized `DefaultHasher`.

Define chart canonicalization before implementation: UTC instant and precision, timezone resolution, observer coordinates and precision, ephemeris/version, geocentric vs topocentric choice, zodiac/ayanamsha if applicable, house system if used, and normalized planet longitudes. Same input and version yields the same result; a tiny input change may yield a different collage. If smooth visual changes are desired, interpolate a spatial/rank mapping instead of hashing the whole chart.

For more geometry in the artwork, let within-sign longitude control position/rotation/scale or a bounded preference over galaxies' ecliptic-longitude ranks, with hashing for variation. Keep galaxy declination/latitude as an independent visual dimension. For actual angular proximity, transform both objects into the same frame/equinox first and use spherical separation; never subtract an ICRS RA directly from an astrological longitude.

Define deterministic collision handling when multiple planets select one subject: process planets in a fixed order and take the next ranked unused subject; allow disclosed reuse only after exhausting the pool. This is an artistic mapping, not a claim that a planet was physically aligned with the chosen galaxy.

## Storage and next milestones

SQLite plus image files is the appropriate local build artifact. Keep bytes outside SQLite, addressed by SHA-256, with a many-to-many object/image relation. ZIP is a distribution format, not the editable working store. JSON remains useful for debugging and a future static site. Add schema migrations when the prototype schema stabilizes.

1. **This prototype:** bounded discovery, cached ingest, SIMBAD resolution, automatic image gates, provisional ranking, SQLite/JSON and ZIP; exercise both successes and exclusions.
2. **Coverage audit:** crawl the complete source archive and report unique systems, qualifying files, exclusions and failures for every constellation. Add ESO/Webb only where worthwhile; confirm each adapter against live samples and saved fixtures.
3. **Release-quality catalog:** verified constellation boundaries, review gallery/override import, subject crops and visual tags, duplicate-system grouping, license exceptions, stable IDs, immutable manifest and source provenance bundle. Publish no unreviewed entry as approved.
4. **Mapping experiment:** a pure Rust mapping crate with supplied chart coordinates first; compare reproducibility, variation and collision behavior. Choose an ephemeris after checking accuracy, licensing and native/WASM feasibility.
5. **Site:** select static/client or server architecture from actual image, privacy and ephemeris needs. Keep birth information out of ingestion and avoid saving birth data by default.

Success for the next phase means knowing the real coverage and image quality for all pools, not merely having a scraper that runs.
