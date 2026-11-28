"""Take an Evo database from schema 21 back to 20 (docs/features/evo-mates-threads-board/spec.md, NFR-1).

Migration 0021 only adds `mate_shares` (linked conversations) and
`thread_follows` (followed and resolved threads). Going back drops them and
their indexes, removes version 21 from `schema_migrations` and sets
`user_version` to 20, so an Evo built before linking opens the database
again. Every other table is left as it is. Shares and follows are lost (the
shared messages themselves stay in their conversations).

Close Evo first. Dry run (default) says what it would do; ``--apply`` copies
the database to ``<path>.before-0021-downgrade`` (unless ``--no-backup``),
then does it inside one transaction and checks the database's integrity.

    python downgrade-0021.py <path to evo.db>
    python downgrade-0021.py <path to evo.db> --apply

Never run it on a database you have not backed up.
"""

from __future__ import annotations

import argparse
import shutil
import sqlite3
import sys

TABLES = ["mate_shares", "thread_follows"]
INDEXES = ["mate_shares_from", "mate_shares_to", "thread_follows_following"]


def state(conn: sqlite3.Connection) -> tuple[int, int, dict[str, int]]:
    version = conn.execute("PRAGMA user_version").fetchone()[0]
    newest = conn.execute("SELECT COALESCE(MAX(version), 0) FROM schema_migrations").fetchone()[0]
    present = {
        name: conn.execute(f'SELECT COUNT(*) FROM "{name}"').fetchone()[0]
        for name in TABLES
        if conn.execute("SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?", (name,)).fetchone()
    }
    return version, newest, present


def downgrade(path: str, apply: bool, backup: bool) -> int:
    conn = sqlite3.connect(path)
    try:
        version, newest, present = state(conn)
        rows = ", ".join(f"{name} {count}" for name, count in present.items()) or "no link tables"
        print(f"{path}: schema {version} (newest migration row {newest}); {rows}")
        if version != newest:
            print(f"refused: user_version {version} and schema_migrations ({newest}) disagree; check the database first")
            return 1
        if version < 21:
            print("nothing to do: the schema is older than 21")
            return 0
        if version > 21:
            print(f"refused: schema {version} is newer than 21; downgrade the later migrations first")
            return 1
        if not apply:
            print("dry run: would drop mate_shares and thread_follows, remove version 21 and set user_version 20 (add --apply)")
            return 0
        if backup:
            conn.close()
            copy = f"{path}.before-0021-downgrade"
            shutil.copyfile(path, copy)
            print(f"backup: {copy}")
            conn = sqlite3.connect(path)
        conn.execute("BEGIN IMMEDIATE")
        for index in INDEXES:
            conn.execute(f'DROP INDEX IF EXISTS "{index}"')
        for table in TABLES:
            conn.execute(f'DROP TABLE IF EXISTS "{table}"')
        conn.execute("DELETE FROM schema_migrations WHERE version = 21")
        conn.execute("PRAGMA user_version = 20")
        conn.commit()
        ok = conn.execute("PRAGMA integrity_check").fetchone()[0]
        version, newest, present = state(conn)
        print(f"done: schema {version}, link tables {'left: ' + ', '.join(present) if present else 'gone'}, integrity {ok}")
        return 0 if ok == "ok" and version == 20 and newest == 20 and not present else 1
    finally:
        conn.close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("db")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--no-backup", action="store_true")
    args = parser.parse_args()
    sys.exit(downgrade(args.db, args.apply, not args.no_backup))


if __name__ == "__main__":
    main()
