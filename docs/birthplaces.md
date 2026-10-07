# Birthplace search

`crates/web/catalog.sqlite` contains a standalone FTS5 virtual table named
`birthplaces`, imported from [GeoNames cities500.zip](https://download.geonames.org/export/dump/cities500.zip).
The dataset covers populated places with more than 500 inhabitants **or** administrative
seats down to PPLA4; it is not a complete list of every settlement.

## Import or refresh

Run from the workspace root, using Python 3 with SQLite FTS5 support:

```sh
mkdir -p data/geonames
curl --fail --location --retry 3 \
  --output data/geonames/cities500.zip \
  https://download.geonames.org/export/dump/cities500.zip
python3 scripts/import_geonames.py --archive data/geonames/cities500.zip
```

Keep the downloaded ZIP for offline reproduction. For a fresh snapshot, download
to a new filename and pass that path to `--archive`. Downloads and local backups
under `data/` are ignored by Git; the populated web database is tracked.
The importer defaults to the web catalog regardless of the working directory;
`--database PATH` selects another **existing** catalog with a `run_metadata` table.

Each import replaces only `birthplaces` and its `geonames_cities500` metadata entry
in one transaction. Malformed records, duplicate IDs, invalid coordinates, empty
archives, and failed FTS integrity checks roll back the import. Other catalog data
is preserved. The archive SHA-256, source URL, row count, schema version, and
license are recorded in `run_metadata`. Replaying the same archive produces the
same records and metadata. A fresh galaxy catalog build must be followed by this
import again.

## Columns and queries

- `rowid`: stable integer GeoNames ID; select it as `geoname_id`.
- Searchable: `name`, `ascii_name`, and comma-separated `alternate_names`.
- Unindexed: `latitude`, `longitude` (WGS84 decimal degrees), `population`,
  `timezone` (IANA identifier), `country_code`, `admin1_code` through `admin4_code`,
  `feature_code`, and `modification_date`.

Names use Unicode tokenization, case folding, accent removal, and prefix indexes
of lengths 2 and 3. Other prefix lengths work too. FTS5 provides token-prefix
matching, not typo correction or arbitrary substring matching.

Example autocomplete lookup:

```sql
SELECT rowid AS geoname_id, name, country_code, admin1_code,
       latitude, longitude, timezone, population
FROM birthplaces
WHERE birthplaces MATCH '"sao" "pau"*'
ORDER BY population DESC, rowid ASC
LIMIT 10;
```

In application code, bind the MATCH expression as a parameter. Do not pass raw
user text as FTS syntax: split into words, escape embedded `"` as `""`, wrap each
word in double quotes, and append `*` to the final quoted word. Join the words with
spaces (implicit AND); return no suggestions for empty input. For example,
`sao pau` becomes `"sao" "pau"*`. Use a minimum input length and debounce requests.
Population ordering is a simple starting point for autocomplete ranking.

Resolve a selection directly by its ID:

```sql
SELECT rowid AS geoname_id, name, latitude, longitude, timezone
FROM birthplaces
WHERE rowid = ?;
```

Country and administrative values are **codes**, not display names. Human-readable
country/region labels would require GeoNames `countryInfo.txt` and
`admin1CodesASCII.txt` (not part of this import). Use historical timezone rules
when converting local birth time to UTC, rather than today's offset.

## Attribution and verification

Display “Place data from [GeoNames](https://www.geonames.org/), licensed under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)” in the application's
credits. The import selects fields and builds a search index from the original
data. GeoNames provides no guarantee of completeness or accuracy.
See the [source format](https://download.geonames.org/export/dump/readme.txt).

```sh
python3 -m unittest discover -s scripts -p 'test_import_geonames.py' -v
sqlite3 crates/web/catalog.sqlite \
  "SELECT count(*) FROM birthplaces; PRAGMA integrity_check;"
```
