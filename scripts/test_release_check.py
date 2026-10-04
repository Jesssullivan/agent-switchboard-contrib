"""OCI integrity checks with finite owned fixtures; no build or network."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("release_check", Path(__file__).with_name("release-check.py"))
check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check)


class LayoutTests(unittest.TestCase):
    def fixture(self, directory):
        layout = Path(directory)
        blobs = layout / "blobs/sha256"
        blobs.mkdir(parents=True)
        def write(raw):
            value = check.digest(raw)
            (blobs / value[7:]).write_bytes(raw)
            return {"digest": value, "size": len(raw)}
        platform = {"os": "linux", "architecture": "amd64", "user": "65532", "entrypoint": ["/usr/local/bin/swb"], "cmd": ["serve"]}
        config = write(json.dumps({"os": "linux", "architecture": "amd64", "config": {"User": "65532", "Entrypoint": platform["entrypoint"], "Cmd": ["serve"]}}).encode())
        layer = write(b"fixture-layer")
        raw = json.dumps({"schemaVersion": 2, "config": config, "layers": [layer]}).encode()
        descriptor = write(raw)
        (layout / "oci-layout").write_text('{"imageLayoutVersion":"1.0.0"}')
        (layout / "index.json").write_text(json.dumps({"manifests": [descriptor]}))
        release = {"image": "ghcr.io/xoxd-ai/agent-switchboard@" + descriptor["digest"], "manifest_size": len(raw), "platform": platform}
        return layout, release, descriptor, config, layer

    def test_all_blob_bytes_and_runtime_match(self):
        with tempfile.TemporaryDirectory() as directory:
            layout, release, *_ = self.fixture(directory)
            self.assertEqual(check.check_layout(layout, release)["verified_blob_count"], 3)

    def test_corrupt_layer_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            layout, release, _, _, layer = self.fixture(directory)
            (layout / "blobs/sha256" / layer["digest"][7:]).write_bytes(b"corrupt-layer")
            with self.assertRaisesRegex(ValueError, "hash or size"):
                check.check_layout(layout, release)

    def test_other_immutable_manifest_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            layout, release, descriptor, *_ = self.fixture(directory)
            raw = (layout / "blobs/sha256" / descriptor["digest"][7:]).read_bytes()
            with self.assertRaisesRegex(ValueError, "immutable digest"):
                check.check_manifest(raw + b" ", release)

    def test_duplicate_manifest_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            layout, release, descriptor, *_ = self.fixture(directory)
            (layout / "index.json").write_text(json.dumps({"manifests": [descriptor, descriptor]}))
            with self.assertRaisesRegex(ValueError, "exactly one"):
                check.check_layout(layout, release)

    def test_wrong_platform_or_user_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            layout, release, *_ = self.fixture(directory)
            release["platform"]["architecture"] = "arm64"
            with self.assertRaisesRegex(ValueError, "platform"):
                check.check_layout(layout, release)
            release["platform"]["architecture"] = "amd64"
            release["platform"]["user"] = "0"
            with self.assertRaisesRegex(ValueError, "user"):
                check.check_layout(layout, release)

    def test_path_escape_digest_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "invalid OCI blob digest"):
                check.read_blob(Path(directory), {"digest": "sha256:../../secret", "size": 1})


class SourceTests(unittest.TestCase):
    def exercise(self, file_bytes=b"approved", listed="Cargo.toml", extras="", mapped_bytes=b"approved"):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / "Cargo.toml").write_bytes(file_bytes)
            release = {"schema": "swb.approved-release.v1", "source": "a" * 40, "upstream_main": "b" * 40, "pr_heads": {}, "source_inputs": check.SOURCE_INPUTS_V1, "file_sha256": {"Cargo.toml": check.digest(mapped_bytes)[7:]}}
            def git(_repo, *args):
                if args[0] == "ls-tree":
                    return "Cargo.toml"
                if args == ("ls-files",):
                    return listed
                if args[0] == "ls-files":
                    return extras
                return ""
            with patch.object(check, "git", side_effect=git), patch.object(check.subprocess, "check_output", return_value=b"approved"):
                return check.check_source(repo, release)

    def test_same_inputs_pass(self):
        self.assertEqual(self.exercise()["checked_input_count"], 1)

    def test_runtime_drift_refuses(self):
        with self.assertRaisesRegex(ValueError, "current build/runtime"):
            self.exercise(file_bytes=b"changed")

    def test_missing_or_untracked_input_refuses(self):
        with self.assertRaisesRegex(ValueError, "file set"):
            self.exercise(listed="")
        with self.assertRaisesRegex(ValueError, "untracked"):
            self.exercise(extras="crates/swb/build.rs")

    def test_forged_source_map_refuses(self):
        with self.assertRaisesRegex(ValueError, "signed source"):
            self.exercise(mapped_bytes=b"forged")

    def test_empty_or_weakened_roots_refuse(self):
        release = {"schema": "swb.approved-release.v1", "source_inputs": [], "file_sha256": {}}
        with self.assertRaisesRegex(ValueError, "protected input roots"):
            check.check_source(Path("."), release)
        release["source_inputs"] = check.SOURCE_INPUTS_V1[:-1]
        with self.assertRaisesRegex(ValueError, "protected input roots"):
            check.check_source(Path("."), release)
        release["source_inputs"] = check.SOURCE_INPUTS_V1
        with self.assertRaisesRegex(ValueError, "file map is empty"):
            check.check_source(Path("."), release)


class TagTests(unittest.TestCase):
    HEAD = "c" * 40

    def exercise(self, tag="v1.0.0", kind="tag", commit=HEAD, verified=True, on_main=True, main_ref="main"):
        def git(_repo, *args):
            if args[0] == "cat-file":
                return kind
            if args[0] == "verify-tag" and not verified:
                raise check.subprocess.CalledProcessError(1, "git verify-tag")
            if args[0] == "rev-parse":
                return self.HEAD if args[1] == "HEAD" else commit
            if args[0] == "merge-base" and not on_main:
                raise check.subprocess.CalledProcessError(1, "git merge-base")
            return ""
        with patch.object(check, "git", side_effect=git):
            return check.check_tag(Path("."), tag, main_ref)

    def test_signed_annotated_tag_on_main_passes(self):
        report = self.exercise()
        self.assertEqual((report["release_tag_commit"], report["release_tag_on_main"]), (self.HEAD, "main"))

    def test_bad_name_or_lightweight_tag_refuses(self):
        with self.assertRaisesRegex(ValueError, "tag name"):
            self.exercise(tag="latest")
        with self.assertRaisesRegex(ValueError, "annotated"):
            self.exercise(kind="commit")

    def test_unsigned_tag_refuses(self):
        with self.assertRaisesRegex(ValueError, "signature"):
            self.exercise(verified=False)

    def test_tag_elsewhere_or_off_main_refuses(self):
        with self.assertRaisesRegex(ValueError, "checked-out HEAD"):
            self.exercise(commit="d" * 40)
        with self.assertRaisesRegex(ValueError, "protected main"):
            self.exercise(on_main=False)


class BuiltDigestTests(unittest.TestCase):
    APPROVED = "sha256:" + "a" * 64

    def compare(self, text):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "image.json.sha256"
            path.write_text(text)
            return check.check_built_digest(path, {"image": "ghcr.io/xoxd-ai/agent-switchboard@" + self.APPROVED})

    def test_approved_digest_passes(self):
        self.assertEqual(self.compare(self.APPROVED + "\n"), self.APPROVED)

    def test_other_or_malformed_digest_refuses(self):
        with self.assertRaisesRegex(ValueError, "own ruling"):
            self.compare("sha256:" + "b" * 64)
        with self.assertRaisesRegex(ValueError, "malformed"):
            self.compare("sha256:../../x")


if __name__ == "__main__":
    unittest.main()
