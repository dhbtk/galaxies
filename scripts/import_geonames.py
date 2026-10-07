#!/usr/bin/env python3
"""Atomically import a saved GeoNames cities500.zip into the web catalog."""

import argparse
from datetime import date
import hashlib
import io
import json
import math
from pathlib import Path
import sqlite3
import zipfile


SOURCE_URL = "https://download.geonames.org/export/dump/cities500.zip"
LICENSE_URL = "https://creativecommons.org/licenses/by/4.0/"
DEFAULT_DATABASE = Path(__file__).resolve().parents[1] / "crates/web/catalog.sqlite"
SCHEMA = """
CREATE VIRTUAL TABLE birthplaces USING fts5(
    name, ascii_name, alternate_names,
    country_code UNINDEXED, admin1_code UNINDEXED, admin2_code UNINDEXED,
    admin3_code UNINDEXED, admin4_code UNINDEXED,
    latitude UNINDEXED, longitude UNINDEXED, population UNINDEXED,
    timezone UNINDEXED, feature_code UNINDEXED, modification_date UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3'
)
"""
# FTS5's rowid is the stable GeoNames ID, not a generated sequence number.
INSERT = """
INSERT INTO birthplaces (
    rowid, name, ascii_name, alternate_names, country_code,
    admin1_code, admin2_code, admin3_code, admin4_code,
    latitude, longitude, population, timezone, feature_code, modification_date
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"""


def parse_record(line, number):
    fields = line.rstrip("\r\n").split("\t")
    try:
        if len(fields) != 19:
            raise ValueError(f"expected 19 fields, got {len(fields)}")
        (identifier, name, ascii_name, aliases, latitude, longitude, feature_class,
         feature_code, country, _cc2, admin1, admin2, admin3, admin4, population,
         _elevation, _dem, timezone, modified) = fields
        identifier, population = int(identifier), int(population)
        latitude, longitude = float(latitude), float(longitude)
        if identifier <= 0 or population < 0 or not name or not timezone:
            raise ValueError("invalid ID, population, name, or timezone")
        if feature_class != "P":
            raise ValueError(f"expected populated place, got {feature_class!r}")
        if not (math.isfinite(latitude) and -90 <= latitude <= 90
                and math.isfinite(longitude) and -180 <= longitude <= 180):
            raise ValueError("invalid coordinates")
        date.fromisoformat(modified)
        return (identifier, name, ascii_name, aliases, country, admin1, admin2,
                admin3, admin4, latitude, longitude, population, timezone,
                feature_code, modified)
    except ValueError as error:
        raise ValueError(f"cities500.txt line {number}: {error}") from error


def import_archive(archive, database):
    archive, database = Path(archive), Path(database).resolve()
    # Read one immutable snapshot for both hashing and importing; no ZIP extraction.
    payload = archive.read_bytes()
    checksum = hashlib.sha256(payload).hexdigest()
    with zipfile.ZipFile(io.BytesIO(payload)) as source:
        if source.namelist().count("cities500.txt") != 1:
            raise ValueError("archive must contain exactly one cities500.txt")
        # mode=rw prevents a typo from silently creating a new empty catalog.
        connection = sqlite3.connect(database.as_uri() + "?mode=rw", uri=True)
        try:
            connection.execute("BEGIN IMMEDIATE")
            connection.execute("DROP TABLE IF EXISTS birthplaces")
            connection.execute(SCHEMA)
            seen = set()
            with source.open("cities500.txt") as raw:
                for number, line in enumerate(io.TextIOWrapper(raw, encoding="utf-8"), 1):
                    row = parse_record(line, number)
                    if row[0] in seen:
                        raise ValueError(f"duplicate GeoNames ID on line {number}: {row[0]}")
                    seen.add(row[0])
                    connection.execute(INSERT, row)
            if not seen:
                raise ValueError("refusing to import an empty cities500.txt")
            connection.execute("INSERT INTO birthplaces(birthplaces) VALUES ('optimize')")
            connection.execute("INSERT INTO birthplaces(birthplaces) VALUES ('integrity-check')")
            metadata = {
                "schema_version": 1, "source_url": SOURCE_URL,
                "archive_sha256": checksum, "rows": len(seen),
                "attribution": "GeoNames", "license": "CC BY 4.0",
                "license_url": LICENSE_URL,
            }
            connection.execute(
                "INSERT OR REPLACE INTO run_metadata(key,value) VALUES (?,?)",
                ("geonames_cities500", json.dumps(metadata, sort_keys=True)),
            )
            connection.commit()
            return metadata
        except BaseException:
            connection.rollback()
            raise
        finally:
            connection.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True,
                        help="saved cities500.zip (retained for offline replay)")
    parser.add_argument("--database", type=Path, default=DEFAULT_DATABASE)
    args = parser.parse_args()
    print(json.dumps(import_archive(args.archive, args.database), indent=2))


if __name__ == "__main__":
    main()
