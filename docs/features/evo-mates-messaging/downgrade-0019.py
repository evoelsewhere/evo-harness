"""Take an Evo database from schema 19 back to 18 (docs/features/evo-mates-messaging/spec.md, NFR-1).

Migration 0019 only adds the table `channel_reactions` (reactions to channel
lines). Going back drops that table and its index, removes version 19 from
`schema_migrations` and sets `user_version` to 18, so an Evo built before
messaging basics 1b opens the database again. Every other table is left as it
is. The reactions themselves are lost.

Close Evo first. Dry run (default) says what it would do; ``--apply`` does it
inside one transaction and checks the database's integrity after.

    python downgrade-0019.py <path to evo.db>
    python downgrade-0019.py <path to evo.db> --apply

Never run it on a database you have not backed up.
"""

from __future__ import annotations

import argparse
import sqlite3
import sys


def state(conn: sqlite3.Connection) -> tuple[int, bool, int]:
    version = conn.execute("PRAGMA user_version").fetchone()[0]
    has_table = conn.execute("SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'channel_reactions'").fetchone() is not None
    rows = conn.execute("SELECT COUNT(*) FROM channel_reactions").fetchone()[0] if has_table else 0
    return version, has_table, rows


def downgrade(path: str, apply: bool) -> int:
    conn = sqlite3.connect(path)
    try:
        version, has_table, rows = state(conn)
        print(f"{path}: schema {version}, channel_reactions {'present' if has_table else 'absent'} ({rows} rows)")
        if version < 19:
            print("nothing to do: the schema is older than 19")
            return 0
        if version > 19:
            print(f"refused: schema {version} is newer than 19; downgrade the later migrations first")
            return 1
        if not apply:
            print("dry run: would drop channel_reactions, remove version 19 and set user_version 18 (add --apply)")
            return 0
        conn.execute("BEGIN IMMEDIATE")
        conn.execute("DROP INDEX IF EXISTS channel_reactions_untold")
        conn.execute("DROP TABLE IF EXISTS channel_reactions")
        conn.execute("DELETE FROM schema_migrations WHERE version = 19")
        conn.execute("PRAGMA user_version = 18")
        conn.commit()
        ok = conn.execute("PRAGMA integrity_check").fetchone()[0]
        version, has_table, _ = state(conn)
        print(f"done: schema {version}, channel_reactions {'present' if has_table else 'absent'}, integrity {ok}")
        return 0 if ok == "ok" and version == 18 and not has_table else 1
    finally:
        conn.close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("db")
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    sys.exit(downgrade(args.db, args.apply))


if __name__ == "__main__":
    main()
