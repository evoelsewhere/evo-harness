"""Tests for release_notes.py.

    python3 -m unittest discover -s .github/scripts -p "test_*.py"
"""

import subprocess
import tempfile
import unittest
from pathlib import Path

import release_notes as r

REPO = "evoelsewhere/evo-harness"


def commit(subject, body="", sha="0123456789abcdef0123456789abcdef01234567"):
    return r.parse(sha, subject, body)


class ParseTests(unittest.TestCase):
    def test_reads_type_scope_and_description(self):
        c = commit("feat(desktop): computer app control on macOS (T41, T42)")
        self.assertEqual((c.kind, c.scope, c.description, c.breaking), ("feat", "desktop", "computer app control on macOS (T41, T42)", False))

    def test_a_bang_or_a_footer_marks_a_breaking_change(self):
        self.assertTrue(commit("refactor(protocol)!: rename the session events").breaking)
        self.assertTrue(commit("feat: new store", body="BREAKING CHANGE: old databases are not read").breaking)
        self.assertTrue(commit("feat: new store", body="BREAKING-CHANGE: x").breaking)

    def test_a_subject_without_a_type_is_kept_as_other(self):
        c = commit("Update README.md")
        self.assertEqual((c.kind, c.scope, c.description), (None, None, "Update README.md"))
        self.assertEqual(r.section(c), "Other changes")

    def test_type_case_is_ignored(self):
        self.assertEqual(commit("Fix(engine): x").kind, "fix")


class SectionTests(unittest.TestCase):
    def test_sections_by_type(self):
        self.assertEqual(r.section(commit("feat(desktop): a")), "Features")
        self.assertEqual(r.section(commit("fix(engine): a")), "Fixes")
        self.assertEqual(r.section(commit("perf(store): a")), "Performance")
        self.assertEqual(r.section(commit("refactor(engine): a")), "Other changes")
        self.assertEqual(r.section(commit("feat(desktop)!: a")), "Breaking changes")

    def test_housekeeping_is_left_out(self):
        for subject in ("docs(readme): a", "style(engine): a", "test: a", "ci: a", "build: a", "chore(deps): a", "chore(release): prepare v4.2.1"):
            self.assertIsNone(r.section(commit(subject)), subject)

    def test_breaking_housekeeping_is_still_listed(self):
        self.assertEqual(r.section(commit("build!: raise the MSRV to 1.89")), "Breaking changes")


class RenderTests(unittest.TestCase):
    def test_groups_in_order_with_links_and_a_compare_link(self):
        commits = [
            commit("fix(engine): survive oversized RPC frames", sha="a" * 40),
            commit("feat(desktop): update the app from signed GitHub releases", sha="b" * 40),
            commit("docs(readme): describe the core features", sha="c" * 40),
            commit("feat!: drop the CLI", sha="d" * 40),
        ]
        text = r.render(commits, REPO, "v4.2.1", "v4.3.0")
        order = [text.index(heading) for heading in ("### Breaking changes", "### Features", "### Fixes")]
        self.assertEqual(order, sorted(order))
        self.assertIn(f"- **desktop:** Update the app from signed GitHub releases ([bbbbbbb](https://github.com/{REPO}/commit/{'b' * 40}))", text)
        self.assertIn("- Drop the CLI (", text)
        self.assertNotIn("describe the core features", text)
        self.assertTrue(text.rstrip().endswith(f"**Full Changelog**: https://github.com/{REPO}/compare/v4.2.1...v4.3.0"))

    def test_the_same_subject_twice_is_listed_once(self):
        commits = [commit("fix(desktop): a", sha="a" * 40), commit("fix(desktop): a", sha="b" * 40)]
        self.assertEqual(r.render(commits, REPO, "v1", "v2").count("- **desktop:** A"), 1)

    def test_nothing_to_list_says_so(self):
        text = r.render([commit("docs: a")], REPO, "v1", "v2")
        self.assertIn("No user-facing changes", text)

    def test_a_first_release_has_no_compare_link(self):
        text = r.render([commit("feat: a")], REPO, None, "v1.0.0")
        self.assertNotIn("compare", text)

    def test_an_unreleased_head_compares_against_the_commit(self):
        text = r.render([commit("feat: a")], REPO, "v1.0.0", "1234567890abcdef")
        self.assertIn("compare/v1.0.0...1234567890abcdef", text)


def git(cwd, *args):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


class GitTests(unittest.TestCase):
    """Against a throw-away repository: previous tag, range, merges."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self.tmp.name)
        git(self.repo, "init", "-q", "-b", "main")
        git(self.repo, "config", "user.email", "test@example.invalid")
        git(self.repo, "config", "user.name", "test")
        git(self.repo, "config", "commit.gpgsign", "false")
        git(self.repo, "config", "tag.gpgsign", "false")

    def tearDown(self):
        self.tmp.cleanup()

    def commit(self, subject, body=""):
        message = subject + ("\n\n" + body if body else "")
        git(self.repo, "commit", "-q", "--allow-empty", "-m", message)

    def test_lists_the_commits_since_the_previous_tag(self):
        self.commit("feat: first")
        git(self.repo, "tag", "v1.0.0")
        self.commit("fix(engine): second")
        git(self.repo, "tag", "not-a-release")
        self.commit("feat(desktop): third", body="Longer text.\n\nBREAKING CHANGE: gone")
        git(self.repo, "tag", "v1.1.0")
        self.commit("feat: after the tag")

        self.assertEqual(r.previous_tag("v1.1.0", cwd=self.repo), "v1.0.0")
        self.assertIsNone(r.previous_tag("v1.0.0", cwd=self.repo))
        commits = r.commits_between("v1.0.0", "v1.1.0", cwd=self.repo)
        self.assertEqual([c.description for c in commits], ["third", "second"])
        self.assertTrue(commits[0].breaking)

    def test_merge_commits_are_skipped_but_what_they_bring_is_kept(self):
        self.commit("feat: base")
        git(self.repo, "tag", "v1.0.0")
        git(self.repo, "switch", "-q", "-c", "topic")
        self.commit("fix(desktop): on a branch")
        git(self.repo, "switch", "-q", "main")
        self.commit("feat(engine): on main")
        git(self.repo, "merge", "-q", "--no-ff", "-m", "Merge pull request #1 from x/topic", "topic")
        subjects = {c.subject for c in r.commits_between("v1.0.0", "HEAD", cwd=self.repo)}
        self.assertEqual(subjects, {"fix(desktop): on a branch", "feat(engine): on main"})

    def test_main_writes_the_notes(self):
        self.commit("feat: first")
        git(self.repo, "tag", "v1.0.0")
        self.commit("fix(engine): second")
        out = self.repo / "notes.md"
        code = r.main(["--to", "HEAD", "--repo", REPO, "--out", str(out)], cwd=self.repo)
        self.assertEqual(code, 0)
        self.assertIn("- **engine:** Second", out.read_text())


if __name__ == "__main__":
    unittest.main()
