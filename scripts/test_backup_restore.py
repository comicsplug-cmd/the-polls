"""Exercise the same SQLite online-backup and restore path used in production."""

import sqlite3
import tempfile
from pathlib import Path


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="the-polls-backup-") as directory:
        root = Path(directory)
        live = root / "live.sqlite"
        snapshot = root / "snapshot.sqlite"
        restored = root / "restored.sqlite"

        source = sqlite3.connect(live)
        source.execute("PRAGMA journal_mode=WAL")
        source.execute("CREATE TABLE receipt (receipt_id TEXT PRIMARY KEY, status TEXT NOT NULL)")
        source.execute("CREATE TABLE tally (party TEXT PRIMARY KEY, total INTEGER NOT NULL)")
        source.execute("INSERT INTO receipt VALUES ('pr_example', 'counted')")
        source.execute("INSERT INTO tally VALUES ('AA', 7)")
        source.commit()

        target = sqlite3.connect(snapshot)
        source.backup(target)
        assert target.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        target.close()
        source.close()

        saved = sqlite3.connect(snapshot)
        target = sqlite3.connect(restored)
        saved.backup(target)
        assert target.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert target.execute("SELECT status FROM receipt WHERE receipt_id='pr_example'").fetchone() == ("counted",)
        assert target.execute("SELECT total FROM tally WHERE party='AA'").fetchone() == (7,)
        target.close()
        saved.close()

    print("SQLite backup and restore passed integrity and data checks.")


if __name__ == "__main__":
    main()
