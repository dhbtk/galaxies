#!/usr/bin/env python3
"""Prepare content-addressed JPEGs for the web catalog and update SQLite."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile


MAX_EDGE = 1920
QUALITY = 88


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def identify(path):
    result = subprocess.run(
        ["magick", "identify", "-ping", "-format", "%m %w %h", str(path)],
        check=True, capture_output=True, text=True,
    )
    image_format, width, height = result.stdout.split()
    return image_format, int(width), int(height)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--web", type=Path, default=Path("crates/web"))
    args = parser.parse_args()
    web = args.web.resolve()
    originals = web / "catalog-images"
    database = web / "catalog.sqlite"
    staged_images = web / ".catalog-images-staging"
    staged_database = web / ".catalog.sqlite-staging"
    backup_images = web / ".catalog-images-before-optimize"
    backup_database = web / ".catalog.sqlite-before-optimize"
    manifest_path = web / "catalog-images-processing.json"
    for path in (staged_images, staged_database, backup_images, backup_database, manifest_path):
        if path.exists():
            parser.error(f"refusing to overwrite existing path: {path}")
    if not originals.is_dir() or not database.is_file():
        parser.error("catalog-images and catalog.sqlite must both exist")

    source = sqlite3.connect(f"file:{database}?mode=ro", uri=True)
    source.row_factory = sqlite3.Row
    rows = source.execute("SELECT id,local_path,sha256,width,height,metadata_json FROM images").fetchall()
    by_name = {}
    for row in rows:
        name = Path(row["local_path"]).name
        if row["local_path"] != f"images/{name}" or name != f'{row["sha256"]}{Path(name).suffix}':
            raise ValueError(f"unexpected content-addressed path: {row['local_path']}")
        if Path(name).suffix.lower() not in {".jpg", ".png"}:
            raise ValueError(f"unsupported file type: {name}")
        if name in by_name and by_name[name]["sha256"] != row["sha256"]:
            raise ValueError(f"conflicting rows for {name}")
        by_name[name] = row
    actual_names = {p.name for p in originals.iterdir() if p.is_file()}
    if actual_names != set(by_name):
        raise ValueError(f"image/DB mismatch: missing={set(by_name)-actual_names}, extra={actual_names-set(by_name)}")

    staged_images.mkdir()
    changes = {}
    try:
        for index, (name, row) in enumerate(sorted(by_name.items()), 1):
            original = originals / name
            if digest(original) != row["sha256"]:
                raise ValueError(f"original SHA-256 mismatch: {name}")
            image_format, width, height = identify(original)
            if image_format != ("PNG" if name.endswith(".png") else "JPEG"):
                raise ValueError(f"file extension/format mismatch: {name}")
            if (width, height) != (row["width"], row["height"]):
                raise ValueError(f"image/DB dimension mismatch: {name}")
            transform = image_format == "PNG" or max(width, height) > MAX_EDGE
            if transform:
                with tempfile.NamedTemporaryFile(suffix=".jpg", dir=staged_images, delete=False) as temp:
                    temporary = Path(temp.name)
                try:
                    subprocess.run([
                        "magick", str(original), "-background", "black", "-alpha", "remove",
                        "-alpha", "off", "-resize", f"{MAX_EDGE}x{MAX_EDGE}>",
                        "-sampling-factor", "4:4:4", "-quality", str(QUALITY), str(temporary),
                    ], check=True, capture_output=True, text=True)
                    new_hash = digest(temporary)
                    new_name = f"{new_hash}.jpg"
                    new_file = staged_images / new_name
                    if new_file.exists():
                        if digest(new_file) != new_hash:
                            raise ValueError(f"output hash collision: {new_name}")
                        temporary.unlink()
                    else:
                        temporary.rename(new_file)
                finally:
                    temporary.unlink(missing_ok=True)
            else:
                new_hash = row["sha256"]
                new_name = name
                new_file = staged_images / new_name
                shutil.copy2(original, new_file)
            new_format, new_width, new_height = identify(new_file)
            if new_format != "JPEG" or max(new_width, new_height) > MAX_EDGE or min(new_width, new_height) < 1:
                raise ValueError(f"invalid output dimensions/format: {new_name}")
            if digest(new_file) != new_hash:
                raise ValueError(f"output SHA-256 mismatch: {new_name}")
            changes[name] = {
                "original_sha256": row["sha256"], "original_width": width,
                "original_height": height, "sha256": new_hash,
                "width": new_width, "height": new_height,
                "local_path": f"images/{new_name}", "transformed": transform,
            }
            if index % 25 == 0 or index == len(by_name):
                print(f"Prepared {index}/{len(by_name)} images", flush=True)

        staged = sqlite3.connect(staged_database)
        source.backup(staged)
        staged.execute("BEGIN IMMEDIATE")
        for row in rows:
            change = changes[Path(row["local_path"]).name]
            if not change["transformed"]:
                continue
            metadata = json.loads(row["metadata_json"])
            metadata.update({key: change[key] for key in ("local_path", "sha256", "width", "height")})
            metadata["site_image_processing"] = {
                "original_local_path": row["local_path"],
                "original_sha256": row["sha256"],
                "original_width": row["width"],
                "original_height": row["height"],
                "max_edge": MAX_EDGE,
                "format": "JPEG",
                "quality": QUALITY,
                "chroma_subsampling": "4:4:4",
            }
            staged.execute(
                "UPDATE images SET local_path=?,sha256=?,width=?,height=?,metadata_json=? WHERE id=?",
                (change["local_path"], change["sha256"], change["width"], change["height"],
                 json.dumps(metadata, ensure_ascii=False, separators=(",", ":")), row["id"]),
            )
        staged.execute("INSERT OR REPLACE INTO run_metadata(key,value) VALUES (?,?)", (
            "web_image_processing",
            json.dumps({"max_edge": MAX_EDGE, "format": "JPEG", "quality": QUALITY,
                        "chroma_subsampling": "4:4:4", "transformed_images": sum(c["transformed"] for c in changes.values())}),
        ))
        staged.commit()
        if staged.execute("PRAGMA integrity_check").fetchone()[0] != "ok" or staged.execute("PRAGMA foreign_key_check").fetchall():
            raise ValueError("staged SQLite integrity check failed")
        staged.close()
        source.close()

        # Move the prepared pair into place, retaining the originals until validation succeeds.
        os.rename(originals, backup_images)
        os.rename(database, backup_database)
        try:
            os.rename(staged_images, originals)
            os.rename(staged_database, database)
        except Exception:
            if originals.exists():
                os.rename(originals, staged_images)
            if database.exists():
                os.rename(database, staged_database)
            os.rename(backup_images, originals)
            os.rename(backup_database, database)
            raise
        with sqlite3.connect(database) as check:
            for row in check.execute("SELECT local_path,sha256,width,height,metadata_json FROM images"):
                name = Path(row[0]).name
                file = originals / name
                if not file.is_file() or digest(file) != row[1]:
                    raise ValueError(f"installed image/hash mismatch: {name}")
                metadata = json.loads(row[4])
                if any(metadata[key] != value for key, value in zip(
                    ("local_path", "sha256", "width", "height"), row[:4])):
                    raise ValueError(f"installed embedded metadata mismatch: {name}")
            if check.execute("PRAGMA integrity_check").fetchone()[0] != "ok" or check.execute("PRAGMA foreign_key_check").fetchall():
                raise ValueError("installed SQLite integrity check failed")
        manifest_path.write_text(json.dumps({
            "max_edge": MAX_EDGE, "format": "JPEG", "quality": QUALITY,
            "chroma_subsampling": "4:4:4", "images": changes,
        }, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        shutil.rmtree(backup_images)
        backup_database.unlink()
        print(f"Optimized {len(changes)} images; transformed {sum(c['transformed'] for c in changes.values())}.")
    except Exception:
        # Keep any staging or original backup after failure for safe inspection/recovery.
        raise


if __name__ == "__main__":
    main()
