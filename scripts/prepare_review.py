#!/usr/bin/env python3
"""Create a stable, deletable photo review set from a merged catalog."""

import argparse
import collections
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import sys


def safe_name(value):
    value = re.sub(r"[^\w .+-]", "_", value, flags=re.UNICODE).strip(" .")
    return value[:75] or "untitled"


def write_page(path, title, body):
    path.write_text(
        '<!doctype html><html lang="en"><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width,initial-scale=1">'
        f'<title>{html.escape(title)}</title><style>'
        'body{font:16px system-ui,sans-serif;background:#101620;color:#e9edf4;'
        'max-width:1500px;margin:0 auto;padding:24px}a{color:#9ad1ff}'
        'nav{display:flex;gap:16px;flex-wrap:wrap;margin:22px 0}'
        '.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(275px,1fr));gap:18px}'
        'article{background:#1c2635;padding:12px;border-radius:8px;min-width:0}'
        'img{width:100%;height:240px;object-fit:contain;background:#080b10}'
        'p{line-height:1.4}small,code{color:#c2cbd8;overflow-wrap:anywhere}'
        '</style><h1>' + html.escape(title) + '</h1>' + body + '</html>', encoding="utf-8"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, default=Path("crates/galaxy-scraper/data-wikipedia"))
    parser.add_argument("--output", type=Path, default=Path("crates/galaxy-scraper/review"))
    args = parser.parse_args()
    data = args.data.resolve()
    output = args.output.resolve()
    if output.exists():
        parser.error(f"review directory already exists: {output}; refusing to restore deleted pictures")
    if data == output or data in output.parents:
        parser.error("put the review directory outside the generated dataset")
    raw = (data / "candidates.json").read_bytes()
    records = json.loads(raw)
    selected = {r["image_id"] for r in json.loads((data / "selected.json").read_text())}
    by_photo = collections.defaultdict(list)
    for record in records:
        source = data / record["local_path"]
        if source.parent != data / "images" or source.suffix.lower() not in {".jpg", ".png"}:
            raise ValueError(f"unexpected image path: {record['local_path']}")
        if not source.is_file():
            raise FileNotFoundError(source)
        by_photo[record["sha256"]].append(record)

    # Copy rather than hard-link: removing or editing a review file cannot alter the source.
    output.mkdir(parents=True)
    entries = []
    try:
        for digest, group in sorted(by_photo.items(), key=lambda item: (
            item[1][0]["source_constellation"], item[1][0]["title"].casefold(), item[0]
        )):
            constellations = {r["source_constellation"] for r in group}
            if len(constellations) != 1:
                raise ValueError(f"shared photo spans constellations: {digest}")
            constellation = next(iter(constellations))
            source = data / group[0]["local_path"]
            filename = f"{safe_name(group[0]['title'])}__{digest[:12]}{source.suffix.lower()}"
            review_path = Path("images") / constellation / filename
            target = output / review_path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            entries.append({
                "review_path": review_path.as_posix(),
                "source_path": group[0]["local_path"],
                "sha256": digest,
                "constellation": constellation,
                "records": [{"image_id": r["image_id"], "title": r["title"],
                             "selected": r["image_id"] in selected} for r in group],
            })

        manifest = {"source_data": str(data),
                    "candidates_sha256": hashlib.sha256(raw).hexdigest(),
                    "photos": entries}
        (output / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        pages = collections.defaultdict(list)
        for entry in entries:
            pages[entry["constellation"]].append(entry)
        nav = "<nav>" + " ".join(
            f'<a href="{html.escape(name)}.html">{html.escape(name)} ({len(items)})</a>'
            for name, items in sorted(pages.items())) + "</nav>"
        write_page(output / "index.html", "Galaxy photo review",
                   f"<p>{len(entries)} distinct photos, {len(records)} catalog records. "
                   "Open a constellation to browse; open a photo for full resolution. "
                   "Delete unsuitable files from the <code>images/</code> folders. "
                   "Keep <code>manifest.json</code> so missing files identify rejected records. "
                   "The source catalog remains available for later reconciliation.</p>" + nav)
        for name, items in sorted(pages.items()):
            cards = []
            for entry in items:
                group = by_photo[entry["sha256"]]
                photo = html.escape(entry["review_path"], quote=True)
                names = ", ".join(html.escape(r["title"]) for r in group)
                primary = group[0]
                badge = "selected" if any(r["image_id"] in selected for r in group) else "candidate"
                coords = "; ".join(
                    f"{html.escape(str(o['id']))}: RA {o['ra_deg']:.4f}°, Dec {o['dec_deg']:.4f}°"
                    for r in group for o in r["objects"]
                )
                cards.append(
                    f'<article><a href="{photo}"><img loading="lazy" src="{photo}" alt="{names}"></a>'
                    f'<h2>{names}</h2><p>{badge} · {primary["width"]}×{primary["height"]} · '
                    f'{"Wikipedia" if primary.get("wikipedia") else "ESA/Hubble"}</p>'
                    f'<p><small>{coords}</small></p><p><a href="{html.escape(primary["source_page"], quote=True)}">Source page</a></p>'
                    f'<p><code>{html.escape(Path(photo).name)}</code></p></article>'
                )
            write_page(output / f"{name}.html", f"{name}: {len(items)} photos",
                       '<p><a href="index.html">All constellations</a></p><div class="grid">'
                       + "".join(cards) + "</div>")
    except Exception:
        shutil.rmtree(output)
        raise
    print(f"{len(entries)} distinct review photos for {len(records)} records: {output}")


if __name__ == "__main__":
    main()
