"""Take an Evo database from schema 20 back to 19 (docs/features/evo-mates-threads-board/spec.md, NFR-1).

Migration 0020 only adds the board tables (`boards`, `board_columns`,
`board_cards`, `board_events`). Going back drops them and their indexes,
removes version 20 from `schema_migrations` and sets `user_version` to 19, so
an Evo built before the board opens the database again. Every other table is
left as it is. The boards and their cards are lost.

Close Evo first. Dry run (default) says what it would do; ``--apply`` copies
the database to ``<path>.before-0020-downgrade`` (unless ``--no-backup``),
then does it inside one transaction and checks the database's integrity.

    python downgrade-0020.py <path to evo.db>
    python downgrade-0020.py <path to evo.db> --apply

Never run it on a database you have not backed up.
"""

from __future__ import annotations

import argparse
import shutil
import sqlite3
import sys

TABLES = ["board_events", "board_cards", "board_columns", "boards"]
INDEXES = ["board_events_card", "board_cards_assignee", "board_cards_column", "board_columns_channel"]


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
        rows = ", ".join(f"{name} {count}" for name, count in present.items()) or "no board tables"
        print(f"{path}: schema {version} (newest migration row {newest}); {rows}")
        if version != newest:
            print(f"refused: user_version {version} and schema_migrations ({newest}) disagree; check the database first")
            return 1
        if version < 20:
            print("nothing to do: the schema is older than 20")
            return 0
        if version > 20:
            print(f"refused: schema {version} is newer than 20; downgrade the later migrations first")
            return 1
        if not apply:
            print("dry run: would drop the board tables, remove version 20 and set user_version 19 (add --apply)")
            return 0
        if backup:
            conn.close()
            copy = f"{path}.before-0020-downgrade"
            shutil.copyfile(path, copy)
            print(f"backup: {copy}")
            conn = sqlite3.connect(path)
        conn.execute("BEGIN IMMEDIATE")
        for index in INDEXES:
            conn.execute(f'DROP INDEX IF EXISTS "{index}"')
        for table in TABLES:
            conn.execute(f'DROP TABLE IF EXISTS "{table}"')
        conn.execute("DELETE FROM schema_migrations WHERE version = 20")
        conn.execute("PRAGMA user_version = 19")
        conn.commit()
        ok = conn.execute("PRAGMA integrity_check").fetchone()[0]
        version, newest, present = state(conn)
        print(f"done: schema {version}, board tables {'left: ' + ', '.join(present) if present else 'gone'}, integrity {ok}")
        return 0 if ok == "ok" and version == 19 and newest == 19 and not present else 1
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
