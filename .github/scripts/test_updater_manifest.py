"""Tests for updater_manifest.py (docs/features/app-updates/tasks.md, T16).

    python3 -m unittest discover -s .github/scripts -p "test_*.py"
"""

import json
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

import updater_manifest as m

NOW = datetime(2026, 11, 20, 8, 0, tzinfo=timezone.utc)
REPO, TAG, VERSION = "evoelsewhere/evo-harness", "v4.3.0", "4.3.0"


def platforms(**override):
    base = {
        "windows-x86_64": {"platform": "windows-x86_64", "eligible": True, "asset": "Evo Harness_4.3.0_x64-setup.exe", "signature": "c2lnLXdpbg==\n"},
        "darwin-aarch64": {"platform": "darwin-aarch64", "eligible": True, "asset": "evo-harness_4.3.0_aarch64.app.tar.gz", "signature": "c2lnLWFybQ=="},
        "darwin-x86_64": {"platform": "darwin-x86_64", "eligible": True, "asset": "evo-harness_4.3.0_x64.app.tar.gz", "signature": "c2lnLXg2NA=="},
        "linux-x86_64": {"platform": "linux-x86_64", "eligible": False},
    }
    for name, value in override.items():
        key = name.replace("_", "-", 1)
        if value is None:
            base.pop(key)
        else:
            base[key] = value
    return base


ASSETS = [
    {"name": "Evo.Harness_4.3.0_x64-setup.exe"},
    {"name": "evo-harness_4.3.0_aarch64.app.tar.gz"},
    {"name": "evo-harness_4.3.0_x64.app.tar.gz"},
    {"name": "Evo.Harness_4.3.0_amd64.deb"},
]


class BuildTests(unittest.TestCase):
    def test_a_complete_release_lists_windows_and_both_macs(self):
        manifest = m.build(platforms(), ASSETS, REPO, TAG, VERSION, NOW)
        self.assertEqual(manifest["version"], "4.3.0")
        self.assertEqual(manifest["pub_date"], "2026-11-20T08:00:00Z")
        self.assertEqual(manifest["notes"], "https://github.com/evoelsewhere/evo-harness/releases/tag/v4.3.0")
        self.assertEqual(sorted(manifest["platforms"]), ["darwin-aarch64", "darwin-x86_64", "windows-x86_64"])
        windows = manifest["platforms"]["windows-x86_64"]
        # GitHub stored the space as a dot; the URL uses the tag, not a draft's.
        self.assertEqual(windows["url"], "https://github.com/evoelsewhere/evo-harness/releases/download/v4.3.0/Evo.Harness_4.3.0_x64-setup.exe")
        self.assertEqual(windows["signature"], "c2lnLXdpbg==")

    def test_an_unsigned_mac_build_is_left_out(self):
        manifest = m.build(platforms(darwin_x86_64={"platform": "darwin-x86_64", "eligible": False}), ASSETS, REPO, TAG, VERSION, NOW)
        self.assertNotIn("darwin-x86_64", manifest["platforms"])
        self.assertIn("darwin-aarch64", manifest["platforms"])

    def test_a_missing_platform_file_fails(self):
        with self.assertRaisesRegex(m.ManifestError, "windows-x86_64"):
            m.build(platforms(windows_x86_64=None), ASSETS, REPO, TAG, VERSION, NOW)
        with self.assertRaisesRegex(m.ManifestError, "darwin-aarch64"):
            m.build(platforms(darwin_aarch64=None), ASSETS, REPO, TAG, VERSION, NOW)

    def test_windows_without_an_update_file_fails(self):
        with self.assertRaisesRegex(m.ManifestError, "installable"):
            m.build(platforms(windows_x86_64={"platform": "windows-x86_64", "eligible": False}), ASSETS, REPO, TAG, VERSION, NOW)

    def test_an_asset_not_in_the_release_fails(self):
        with self.assertRaisesRegex(m.ManifestError, "not an asset"):
            m.build(platforms(), ASSETS[1:], REPO, TAG, VERSION, NOW)

    def test_an_eligible_entry_needs_its_signature(self):
        broken = {"platform": "darwin-aarch64", "eligible": True, "asset": "evo-harness_4.3.0_aarch64.app.tar.gz"}
        with self.assertRaisesRegex(m.ManifestError, "signature"):
            m.build(platforms(darwin_aarch64=broken), ASSETS, REPO, TAG, VERSION, NOW)

    def test_linux_never_gets_an_entry(self):
        linux = {"platform": "linux-x86_64", "eligible": True, "asset": "Evo.Harness_4.3.0_amd64.deb", "signature": "eA=="}
        with self.assertRaisesRegex(m.ManifestError, "announced only"):
            m.build(platforms(linux_x86_64=linux), ASSETS, REPO, TAG, VERSION, NOW)


class MainTests(unittest.TestCase):
    def run_main(self, tag=TAG, entries=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            folder = root / "release-assets" / "evo-windows-x64"
            folder.mkdir(parents=True)
            for name, entry in (entries or platforms()).items():
                (folder / f"{name}-updater.json").write_text(json.dumps(entry), encoding="utf-8")
            (root / "assets.json").write_text(json.dumps({"assets": ASSETS}), encoding="utf-8")
            out = root / "latest.json"
            code = m.main([
                "--platforms", str(root / "release-assets"), "--assets", str(root / "assets.json"),
                "--repo", REPO, "--tag", tag, "--version", VERSION, "--out", str(out),
            ])
            return code, (json.loads(out.read_text(encoding="utf-8")) if out.exists() else None)

    def test_writes_the_manifest(self):
        code, manifest = self.run_main()
        self.assertEqual(code, 0)
        self.assertEqual(len(manifest["platforms"]), 3)

    def test_fails_without_writing_on_a_bad_release(self):
        code, manifest = self.run_main(entries=platforms(windows_x86_64=None))
        self.assertEqual(code, 1)
        self.assertIsNone(manifest)

    def test_the_tag_must_match_the_version(self):
        code, manifest = self.run_main(tag="v4.3.1")
        self.assertEqual(code, 1)
        self.assertIsNone(manifest)


if __name__ == "__main__":
    unittest.main()
