PRAGMA foreign_keys = ON;
PRAGMA user_version = 2;
CREATE TABLE IF NOT EXISTS objects (
    id TEXT PRIMARY KEY,
    ra_deg REAL NOT NULL CHECK(ra_deg >= 0 AND ra_deg < 360),
    dec_deg REAL NOT NULL CHECK(dec_deg >= -90 AND dec_deg <= 90),
    coordinate_frame TEXT NOT NULL,
    object_type TEXT NOT NULL,
    morphology TEXT,
    bibliography_count INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS images (
    id TEXT PRIMARY KEY,
    source_constellation TEXT NOT NULL,
    source_page TEXT NOT NULL,
    local_path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    credit TEXT NOT NULL,
    score INTEGER NOT NULL,
    metadata_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS image_objects (
    image_id TEXT REFERENCES images(id),
    object_id TEXT REFERENCES objects(id),
    PRIMARY KEY(image_id, object_id)
);
CREATE TABLE IF NOT EXISTS object_distances (
    object_id TEXT PRIMARY KEY REFERENCES objects(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    distance_mpc REAL CHECK(distance_mpc > 0),
    distance_million_ly REAL CHECK(distance_million_ly > 0),
    uncertainty_mpc REAL CHECK(uncertainty_mpc >= 0),
    method TEXT,
    redshift REAL,
    source_url TEXT NOT NULL,
    metadata_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS selections (
    image_id TEXT PRIMARY KEY REFERENCES images(id),
    rank INTEGER NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS run_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS images_constellation ON images(source_constellation);
