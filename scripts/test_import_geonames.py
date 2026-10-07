from contextlib import closing
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
import zipfile

from import_geonames import import_archive


def record(identifier=3448439, name="São Paulo", aliases="Sampa,Сан-Паулу"):
    return "\t".join(map(str, [
        identifier, name, "Sao Paulo", aliases, -23.5475, -46.63611,
        "P", "PPLA", "BR", "", "27", "3550308", "", "", 12325232,
        "", 769, "America/Sao_Paulo", "2026-10-01",
    ])) + "\n"


class ImportTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.database = Path(self.directory.name) / "catalog.sqlite"
        self.archive = Path(self.directory.name) / "cities500.zip"
        with closing(sqlite3.connect(self.database)) as db:
            db.execute("CREATE TABLE run_metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            db.execute("CREATE TABLE objects(id TEXT PRIMARY KEY)")
            db.execute("INSERT INTO objects VALUES ('preserved')")
            db.commit()

    def write_archive(self, text):
        with zipfile.ZipFile(self.archive, "w") as archive:
            archive.writestr("cities500.txt", text)

    def test_search_and_reimport(self):
        self.write_archive(record())
        first = import_archive(self.archive, self.database)
        self.assertEqual(first, import_archive(self.archive, self.database))
        with closing(sqlite3.connect(self.database)) as db:
            for query in ('"sao" "pau"*', '"São" "Paulo"', '"samp"*', '"Сан"*'):
                rows = db.execute(
                    "SELECT rowid, latitude, longitude, timezone FROM birthplaces "
                    "WHERE birthplaces MATCH ?", (query,),
                ).fetchall()
                self.assertEqual(rows, [(3448439, -23.5475, -46.63611, "America/Sao_Paulo")])
            self.assertEqual(db.execute("SELECT count(*) FROM birthplaces").fetchone()[0], 1)
            self.assertEqual(db.execute("SELECT * FROM objects").fetchall(), [("preserved",)])
            self.assertEqual(json.loads(db.execute(
                "SELECT value FROM run_metadata WHERE key='geonames_cities500'"
            ).fetchone()[0]), first)
        self.write_archive(record(123, "Replacement", ""))
        import_archive(self.archive, self.database)
        with closing(sqlite3.connect(self.database)) as db:
            self.assertEqual(db.execute("SELECT rowid FROM birthplaces").fetchall(), [(123,)])
            self.assertEqual(db.execute("SELECT rowid FROM birthplaces WHERE birthplaces "
                                        "MATCH 'sampa'").fetchall(), [])

    def test_failed_import_rolls_back_data_and_metadata(self):
        self.write_archive(record())
        original = import_archive(self.archive, self.database)
        for invalid in ("", record(123) + "broken\n", record(123) * 2,
                        record(123).replace("-23.5475", "nan")):
            with self.subTest(invalid=invalid):
                self.write_archive(invalid)
                with self.assertRaises(ValueError):
                    import_archive(self.archive, self.database)
                with closing(sqlite3.connect(self.database)) as db:
                    self.assertEqual(db.execute("SELECT rowid FROM birthplaces").fetchall(), [(3448439,)])
                    self.assertEqual(json.loads(db.execute(
                        "SELECT value FROM run_metadata WHERE key='geonames_cities500'"
                    ).fetchone()[0]), original)

    def test_missing_database_is_not_created(self):
        self.write_archive(record())
        missing = self.database.with_name("missing.sqlite")
        with self.assertRaises(sqlite3.OperationalError):
            import_archive(self.archive, missing)
        self.assertFalse(missing.exists())


if __name__ == "__main__":
    unittest.main()
