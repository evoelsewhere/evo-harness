#!/usr/bin/env python3
"""Build the update manifest (latest.json) for a desktop release.

Installed apps read `latest.json` from the latest GitHub release to find,
verify and install a newer version (docs/features/app-updates/spec.md,
FR-15; plan.md 3.3; ADR 0017). Each build job writes one platform file:

    {"platform": "windows-x86_64", "eligible": true,
     "asset": "<file name>", "signature": "<content of the .sig file>"}

`eligible` is false for Linux and for a macOS build made without the
project's Apple identity: those apps only get a notice, so they have no
entry. This script checks that every platform reported, matches each
eligible file to an asset of the release, and writes the manifest. It
fails (exit 1) instead of writing an incomplete manifest, so the release
stays a draft.

    updater_manifest.py --platforms DIR --assets assets.json --repo OWNER/NAME \
        --tag v4.3.0 --version 4.3.0 --out latest.json

`assets.json` is `gh release view <tag> --json assets`.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

# Every release must report these; Windows must be installable in the app.
REQUIRED = ("windows-x86_64", "darwin-x86_64", "darwin-aarch64")
MUST_BE_ELIGIBLE = ("windows-x86_64",)
# Never in the manifest: Linux packages are only told (FR-12).
NOTICE_ONLY_PREFIXES = ("linux-",)


class ManifestError(Exception):
    pass


def load_platforms(folder: Path) -> dict[str, dict]:
    platforms: dict[str, dict] = {}
    for path in sorted(folder.rglob("*-updater.json")):
        entry = json.loads(path.read_text(encoding="utf-8"))
        name = entry.get("platform")
        if not isinstance(name, str) or not name:
            raise ManifestError(f"{path.name}: no platform")
        if name in platforms:
            raise ManifestError(f"{name} is reported twice")
        platforms[name] = entry
    return platforms


def github_name(local: str) -> str:
    """GitHub stores an uploaded file's spaces as dots."""
    return local.replace(" ", ".")


def build(platforms: dict[str, dict], assets: list[dict], repo: str, tag: str, version: str, now: datetime) -> dict:
    missing = [name for name in REQUIRED if name not in platforms]
    if missing:
        raise ManifestError(f"no platform file for {', '.join(missing)}")
    names = {asset["name"] for asset in assets}
    entries: dict[str, dict] = {}
    for name, entry in sorted(platforms.items()):
        eligible = entry.get("eligible") is True
        if name in MUST_BE_ELIGIBLE and not eligible:
            raise ManifestError(f"{name} must be installable in the app, but its build has no update file")
        if not eligible:
            continue
        if name.startswith(NOTICE_ONLY_PREFIXES):
            raise ManifestError(f"{name} is announced only and cannot have an update entry")
        asset, signature = entry.get("asset"), entry.get("signature")
        if not asset or not signature:
            raise ManifestError(f"{name}: an eligible build needs its asset and signature")
        published = asset if asset in names else github_name(asset)
        if published not in names:
            raise ManifestError(f"{name}: {asset} is not an asset of the release")
        entries[name] = {
            "url": f"https://github.com/{repo}/releases/download/{tag}/{published}",
            "signature": signature.strip(),
        }
    return {
        "version": version,
        "notes": f"https://github.com/{repo}/releases/tag/{tag}",
        "pub_date": now.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": entries,
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--platforms", type=Path, required=True)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.tag != f"v{args.version}":
            raise ManifestError(f"tag {args.tag} does not match version {args.version}")
        assets = json.loads(args.assets.read_text(encoding="utf-8")).get("assets", [])
        manifest = build(load_platforms(args.platforms), assets, args.repo, args.tag, args.version, datetime.now(timezone.utc))
    except (ManifestError, OSError, json.JSONDecodeError) as error:
        print(f"update manifest: {error}", file=sys.stderr)
        return 1
    args.out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
