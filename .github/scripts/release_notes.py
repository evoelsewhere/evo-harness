#!/usr/bin/env python3
"""Write the release notes of a desktop release from its commits.

Commits on `main` follow Conventional Commits (`feat(scope): …`,
`fix(scope)!: …`; AGENTS.md, "Change rules"). This script lists the commits
between the previous release tag and `--to`, groups them for people who use
the app, and writes Markdown for the GitHub release:

    ### Breaking changes   `!` after the type, or a `BREAKING CHANGE:` footer
    ### Features           feat
    ### Fixes              fix
    ### Performance        perf
    ### Other changes      refactor, revert, and subjects without a type

`docs`, `style`, `test`, `ci`, `build` and `chore` commits are left out
unless they are breaking. Merge commits are skipped; the commits they bring
in are listed.

    release_notes.py --to v4.3.0 --repo OWNER/NAME --out notes.md
    release_notes.py --to HEAD   --repo OWNER/NAME --out CHANGELOG.md

`--from` defaults to the newest `v*` tag before `--to`. With no earlier tag
every commit up to `--to` is listed. The checkout needs full history and
tags (`actions/checkout` with `fetch-depth: 0`).
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

SECTIONS = ("Breaking changes", "Features", "Fixes", "Performance", "Other changes")
BY_KIND = {"feat": "Features", "fix": "Fixes", "perf": "Performance", "refactor": "Other changes", "revert": "Other changes"}
LEFT_OUT = {"docs", "style", "test", "tests", "ci", "build", "chore"}

SUBJECT = re.compile(r"^(?P<kind>[A-Za-z]+)(?:\((?P<scope>[^)]*)\))?(?P<bang>!)?:\s*(?P<description>.+)$")
BREAKING_FOOTER = re.compile(r"^BREAKING[ -]CHANGE:", re.MULTILINE)

# Fields and records of `git log --format`, which never occur in a message.
FIELD, RECORD = "\x1f", "\x1e"


@dataclass(frozen=True)
class Commit:
    sha: str
    subject: str
    kind: str | None
    scope: str | None
    description: str
    breaking: bool


def parse(sha: str, subject: str, body: str = "") -> Commit:
    subject = subject.strip()
    match = SUBJECT.match(subject)
    if not match:
        return Commit(sha, subject, None, None, subject, False)
    scope = (match["scope"] or "").strip() or None
    breaking = bool(match["bang"]) or bool(BREAKING_FOOTER.search(body))
    return Commit(sha, subject, match["kind"].lower(), scope, match["description"].strip(), breaking)


def section(commit: Commit) -> str | None:
    """The heading a commit is listed under; `None` leaves it out."""
    if commit.breaking:
        return "Breaking changes"
    if commit.kind is None:
        return "Other changes"
    if commit.kind in LEFT_OUT:
        return None
    return BY_KIND.get(commit.kind, "Other changes")


def entry(commit: Commit, repo: str) -> str:
    text = commit.description[:1].upper() + commit.description[1:]
    scope = f"**{commit.scope}:** " if commit.scope else ""
    link = f"[{commit.sha[:7]}](https://github.com/{repo}/commit/{commit.sha})"
    return f"- {scope}{text} ({link})"


def render(commits: list[Commit], repo: str, previous: str | None, to: str) -> str:
    """Markdown notes; `commits` newest first, as `git log` lists them."""
    grouped: dict[str, list[str]] = {name: [] for name in SECTIONS}
    seen: set[str] = set()
    for commit in commits:
        heading = section(commit)
        if heading is None or commit.subject in seen:
            continue
        seen.add(commit.subject)
        grouped[heading].append(entry(commit, repo))

    parts = ["## What's changed", ""]
    listed = [(name, lines) for name, lines in grouped.items() if lines]
    if not listed:
        parts += ["No user-facing changes in this release.", ""]
    for name, lines in listed:
        parts += [f"### {name}", "", *lines, ""]
    if previous:
        parts.append(f"**Full Changelog**: https://github.com/{repo}/compare/{previous}...{to}")
    return "\n".join(parts).rstrip() + "\n"


def git(*args: str, cwd: Path | None = None) -> str:
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout


def previous_tag(to: str, cwd: Path | None = None) -> str | None:
    """The newest `v*` tag reachable from `to`'s parent, if any."""
    try:
        return git("describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", f"{to}^", cwd=cwd).strip() or None
    except subprocess.CalledProcessError:
        return None


def commits_between(previous: str | None, to: str, cwd: Path | None = None) -> list[Commit]:
    span = f"{previous}..{to}" if previous else to
    out = git("log", "--no-merges", f"--format=%H{FIELD}%s{FIELD}%b{RECORD}", span, cwd=cwd)
    commits = []
    for record in out.split(RECORD):
        record = record.strip("\n")
        if not record:
            continue
        sha, subject, body = (record.split(FIELD) + ["", ""])[:3]
        commits.append(parse(sha.strip(), subject, body))
    return commits


def main(argv: list[str] | None = None, cwd: Path | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--to", required=True, help="the release tag, or HEAD for a build that is not released")
    parser.add_argument("--from", dest="previous", help="the previous release tag (default: found from --to)")
    parser.add_argument("--repo", required=True, help="OWNER/NAME on github.com, for links")
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args(argv)

    try:
        previous = args.previous or previous_tag(args.to, cwd=cwd)
        commits = commits_between(previous, args.to, cwd=cwd)
        # A moving ref is pinned to its commit, so the compare link stays right.
        to = args.to if args.to.startswith("v") else git("rev-parse", args.to, cwd=cwd).strip()
    except subprocess.CalledProcessError as error:
        print(f"release_notes: git failed: {error.stderr.strip()}", file=sys.stderr)
        return 1
    args.out.write_text(render(commits, args.repo, previous, to), encoding="utf-8")
    print(f"release_notes: {len(commits)} commits since {previous or 'the first commit'}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
