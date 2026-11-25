#!/usr/bin/env bash
set -euo pipefail

db_path="${DATABASE_FILE:-/var/lib/the-polls/polls.sqlite}"
temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT

python3 - "$db_path" "$temp_dir/polls.sqlite" <<'PY'
import sqlite3
import sys

source = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
target = sqlite3.connect(sys.argv[2])
source.backup(target)
result = target.execute("PRAGMA integrity_check").fetchone()[0]
if result != "ok":
    raise SystemExit(f"SQLite integrity check failed: {result}")
target.close()
source.close()
PY

# Restic encrypts the snapshot in its repository. Configure its credentials outside this script.
restic backup "$temp_dir/polls.sqlite" --tag the-polls --host "${BACKUP_HOST:-polls-vps}"
restic forget --tag the-polls --keep-daily 14 --keep-weekly 8 --keep-monthly 12 --prune
