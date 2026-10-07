# Galaxy distances

The original merged image snapshot has 679 distinct SIMBAD objects. The curated snapshot after visual review has 285 objects. The `distance-enrich` command queries each object through the [current NED API](https://ned.ipac.caltech.edu/Docs::API/) and writes an independent, resumable `distances.json`. It leaves the image catalog, SQLite database and in-progress photo review unchanged.

```sh
cargo run --locked -p galaxy-scraper -- distance-enrich \
  --data crates/galaxy-scraper/data-curated \
  --output crates/galaxy-scraper/data-distances/curated-distances.json
```

After every curated object has a saved status, join the snapshot into the curated SQLite database:

```sh
cargo run --locked -p galaxy-scraper -- distance-apply \
  --data crates/galaxy-scraper/data-curated \
  --distances crates/galaxy-scraper/data-distances/curated-distances.json
```

This adds an `object_distances` table keyed to `objects.id`, including rows whose distance is unknown. The command checks that the snapshot covers exactly the database objects and that its candidate hash matches.

Every entry is keyed by the SIMBAD object ID and records its source URL, NED cross-identifications, angular separation, status, distance in Mpc and million light-years, method, and any reported uncertainty. The file also records a SHA-256 of `candidates.json` and the distance policy. A changed candidate snapshot requires a new output path. Repeating the command resumes from completed entries and cached NED replies. `--limit N` checks only N remaining objects; `--offline` replays cached responses.

The selection rule is deliberately conservative:

1. For redshift at or above 0.1, derive line-of-sight **comoving distance** from NED's redshift using a flat cosmology with H₀ = 67.8 km/s/Mpc, Ωₘ = 0.308 and ΩΛ = 0.692. This is a model estimate, not light-travel time or a directly measured separation.
2. For nearer objects, prefer NED's mean of published **redshift-independent distances** when it is at most 200 Mpc, or when redshift is below 0.01. The NED SEM is reported when present, but does not include all systematic uncertainties.
3. For intermediate redshifts with no suitable independent estimate, use NED's CMB-frame Hubble-flow distance. If redshift is at or below 0.01 and only this estimate exists, leave the distance unknown because peculiar motion can dominate.

A NED name must match the SIMBAD ID/queried alias in NED's cross-identifications with a positional agreement within 60 arcseconds, or match the position within 2 arcseconds. Failed identity matches, missing objects and unavailable distances remain explicit statuses, never invented numbers. Extended galaxies, pairs and clusters may need manual identity review. For images with multiple objects, keep their distances separate; a pair or cluster has no single automatic distance unless the catalog explicitly represents it as one object.

For UI copy, show approximate figures with a method label, for example “about 24 million light-years (published-distance mean)” or “about 2.2 billion light-years (comoving model estimate).” Avoid excess decimal precision and distinguish comoving distance from light-travel time. [NED-LVS](https://ned.ipac.caltech.edu/NED::LVS/) documents the preference for redshift-independent distances within about 200 Mpc; the [NED-D compilation](https://ned.ipac.caltech.edu/Library/Distances/) explains the range of literature methods and the meaning of its mean distances.

## Curated run result

All 285 curated objects were checked against NED. Distances are available for 267; the remaining 18 have explicit missing or unverified statuses. The complete snapshot is `crates/galaxy-scraper/data-distances/curated-distances.json`, and the same statuses are in `data-curated/catalog.sqlite` under `object_distances`. The distance table is a projection of the frozen curated candidate list; a later revision of the image review should get a new distance snapshot and join.
