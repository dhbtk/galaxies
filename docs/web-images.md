# Web catalog images

`crates/web/catalog-images` contains the site's content-addressed JPEGs. The `images.local_path` values in `crates/web/catalog.sqlite` retain the `images/<sha256>.jpg` convention; the site can serve the file with that basename from `catalog-images`.

The one-time preparation command is `python3 scripts/optimize_web_catalog.py`. It validates every source image against the database, converts PNGs to JPEG, and downsizes images only when either dimension exceeds 1920 pixels. It uses ImageMagick with quality 88 and 4:4:4 chroma sampling. Opaque PNGs are composited on black. Originals already small enough and in JPEG format retain their original bytes. The command prepares a separate image directory and SQLite copy, verifies them, then replaces the web pair. It refuses to overwrite an existing processing manifest.

Transformed images get new SHA-256 filenames. The `images` table and its `metadata_json` projection receive the new path, hash and decoded dimensions. `site_image_processing` inside transformed records stores the original path, hash, dimensions and encoding settings. Original source URL, license, credit and source dimensions remain in the metadata. `catalog-images-processing.json` maps every original file to its web file. The original curated catalog under `crates/galaxy-scraper/data-curated` remains unchanged.

The prepared set contains 346 JPEGs for 349 image records (some records share a photo). Of these, 111 large JPEGs were resized and 35 PNGs were converted. The resulting folder is about 159 MB, down from 652 MB. File hashes, decoded dimensions, metadata and database integrity were checked after installation.
