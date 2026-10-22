"""Release publication tests use a fake GitHub CLI; no external writes."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("publish_release", Path(__file__).parents[1] / "scripts/publish-release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = Path(self.temp.name)
        self.dist = self.directory / "dist"
        self.dist.mkdir()
        (self.directory / "Cargo.toml").write_text('[package]\nversion = "0.9.3"\n')
        for name in release.ASSETS:
            if not name.startswith("SHA256SUMS-"):
                (self.dist / name).write_bytes(name.encode())
        for platform in ["linux-x86_64", "windows-x86_64"]:
            sums = []
            for name in release.ASSETS:
                if platform in name and not name.startswith("SHA256SUMS-"):
                    sums.append(f"{hashlib.sha256((self.dist / name).read_bytes()).hexdigest()}  {name}\n")
            (self.dist / f"SHA256SUMS-{platform}.txt").write_text("".join(sums))
        self.previous = Path.cwd()
        os.chdir(self.directory)
        self.environment = patch.dict(os.environ, {
            "GITHUB_REF": "refs/heads/main", "GITHUB_SHA": "a" * 40,
            "GITHUB_RUN_NUMBER": "42", "GITHUB_REPOSITORY": "example/lennette",
        })
        self.environment.start()

    def tearDown(self):
        self.environment.stop()
        os.chdir(self.previous)
        self.temp.cleanup()

    def test_banner_is_first_and_links_exact_assets(self):
        body = release.release_body("example/lennette", "v-test", "abc", "Changes")
        self.assertTrue(body.startswith(f"![Lennette]({release.BANNER})"))
        self.assertIn("https://swr2.itssingularity.com/lennette/github_release_banner_lennette.png", body)
        self.assertIn("/blob/abc/.github/README.md", body)
        for name in release.ASSETS[:3]:
            self.assertIn(f"/releases/download/v-test/{name}", body)

    def test_checksum_failure_blocks_all_github_calls(self):
        (self.dist / "lennette-windows-x86_64.exe").write_bytes(b"damaged")
        with patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "Checksum mismatch"):
                release.main()
            run.assert_not_called()

    def test_missing_platform_blocks_publication(self):
        (self.dist / "lennette-linux-x86_64.tar.gz").unlink()
        with self.assertRaisesRegex(RuntimeError, "missing="):
            release.validate_assets(self.dist)

    def test_only_main_may_publish(self):
        with patch.dict(os.environ, {"GITHUB_REF": "refs/heads/feature"}):
            with self.assertRaisesRegex(RuntimeError, "only be published from main"):
                release.main()

    def test_publish_happens_after_verified_upload_and_banner_notes(self):
        calls = []
        def fake_gh(*args):
            calls.append(args)
            if args[0] == "api": return json.dumps({"body": "Generated changes"})
            if args[:2] in [("release", "create"), ("release", "edit")] and "--notes-file" in args:
                body = Path(args[args.index("--notes-file") + 1]).read_text()
                self.assertTrue(body.startswith(f"![Lennette]({release.BANNER})"))
            if args[:2] == ("release", "view"):
                return json.dumps({"assets": [{"name": n} for n in release.ASSETS]})
            return ""
        missing = subprocess.CompletedProcess([], 1, "", "not found")
        with patch.object(release.subprocess, "run", return_value=missing), patch.object(release, "gh", side_effect=fake_gh):
            release.main()
        self.assertIn("--draft", calls[1])
        self.assertEqual(calls[1][2], "v0.9.3-build.42.aaaaaaa")
        self.assertEqual(calls[-1][-2:], ("--draft=false", "--latest"))
        self.assertEqual(calls[-2][:2], ("release", "view"))
        self.assertEqual(calls[-3][:2], ("release", "upload"))

    def test_published_rerun_preserves_existing_assets(self):
        existing = subprocess.CompletedProcess([], 0, json.dumps({
            "isDraft": False, "targetCommitish": "a" * 40,
            "assets": [{"name": n} for n in release.ASSETS],
        }), "")
        with patch.object(release.subprocess, "run", return_value=existing), patch.object(release, "gh") as gh:
            release.main()
            gh.assert_not_called()

    def test_incomplete_upload_stays_draft(self):
        def fake_gh(*args):
            if args[0] == "api": return json.dumps({"body": "Changes"})
            if args[:2] == ("release", "view"): return json.dumps({"assets": []})
            return ""
        missing = subprocess.CompletedProcess([], 1, "", "not found")
        with patch.object(release.subprocess, "run", return_value=missing), patch.object(release, "gh", side_effect=fake_gh) as gh:
            with self.assertRaisesRegex(RuntimeError, "leaving release as a draft"):
                release.main()
            self.assertFalse(any("--draft=false" in call.args for call in gh.call_args_list))


if __name__ == "__main__":
    unittest.main()
