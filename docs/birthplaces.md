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
curl --fail --location --retry 3 \
  --output data/geonames/admin1CodesASCII.txt \
  https://download.geonames.org/export/dump/admin1CodesASCII.txt
python3 scripts/import_geonames.py --archive data/geonames/cities500.zip \
  --admin1 data/geonames/admin1CodesASCII.txt
```

Keep both downloaded files for offline reproduction. For a fresh snapshot, download
to new filenames and pass those paths to `--archive` and `--admin1`. Downloads and local backups
under `data/` are ignored by Git; the populated web database is tracked.
The importer defaults to the web catalog regardless of the working directory;
`--database PATH` selects another **existing** catalog with a `run_metadata` table.

Each import replaces only `birthplaces` and its `geonames_cities500` metadata entry
in one transaction. Malformed records, duplicate IDs, invalid coordinates, empty
archives, and failed FTS integrity checks roll back the import. Other catalog data
is preserved. The input SHA-256 checksums, source URLs, row count, schema version, and
license are recorded in `run_metadata`. Replaying the same input files produces the
same records and metadata. A fresh galaxy catalog build must be followed by this
import again.

## Columns and queries

- `rowid`: stable integer GeoNames ID; select it as `geoname_id`.
- Searchable: `name`, `ascii_name`, comma-separated `alternate_names`,
  `admin1_name`, and `admin1_ascii_name`.
- Unindexed: `latitude`, `longitude` (WGS84 decimal degrees), `population`,
  `timezone` (IANA identifier), `country_code`, `admin1_code` through `admin4_code`,
  `feature_code`, and `modification_date`.

Names use Unicode tokenization, case folding, accent removal, and prefix indexes
of lengths 2 and 3. Other prefix lengths work too. FTS5 provides token-prefix
matching, not typo correction or arbitrary substring matching.

Example autocomplete lookup:

```sql
SELECT rowid AS geoname_id, name, admin1_name, country_code, admin1_code,
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
SELECT rowid AS geoname_id, name, admin1_name, country_code, latitude, longitude, timezone
FROM birthplaces
WHERE rowid = ?;
```

First-level administrative names come from `admin1CodesASCII.txt`, joined by
`country_code || '.' || admin1_code`. In Brazil these are states: `BR.18` is
Paraná and `BR.06` is Ceará. In other countries they may be provinces or regions.
The names are copied into the FTS table, so no join is needed at query time.
The source provides English names and ASCII variants, not a full localization dataset.

For example:

```sql
SELECT rowid AS geoname_id, name, admin1_name, country_code,
       latitude, longitude, timezone
FROM birthplaces
WHERE birthplaces MATCH '"cascavel"*'
ORDER BY population DESC, rowid ASC
LIMIT 10;
```

The two cities named Cascavel in Brazil return `Paraná` and `Ceará`. Searching
`'"cascavel" "paran"*'` narrows the results by state, without requiring accents.
For matching only city names, use an FTS column filter such as
`'{name ascii_name alternate_names} : "cascavel"*'`.

Missing division mappings produce SQL NULL names; the city and original codes
remain available. `run_metadata.geonames_cities500` records their count as
`rows_without_admin1_name`. The UI should omit missing state labels rather than
showing `NULL`. Country remains an ISO code (`BR`); full country names would
require `countryInfo.txt` or an application country-name mapping. Admin levels
2–4 remain codes. Use historical timezone rules when converting local birth time
to UTC, rather than today's offset.

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
