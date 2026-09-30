"""Fixture-driver failure receipts, using mocks only; no Bazel or signals."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("e2e", Path(__file__).with_name("e2e-fixtures.py"))
e2e = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(e2e)

class ReceiptTests(unittest.TestCase):
    def exercise(self, dirty=False, info_error=False):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            logs = base / "testlogs"
            for target, names in e2e.EXPECTED.items():
                path = logs / target.replace(":", "/") / "test.log"
                path.parent.mkdir(parents=True)
                path.write_text("\n".join(f"test tests::{name} ... ok" for name in names))
            status_calls = [0]
            def check_output(command, **kwargs):
                if "rev-parse" in command:
                    return "a" * 40 + "\n"
                if "status" in command:
                    status_calls[0] += 1
                    return " M tracked.rs\n" if dirty and status_calls[0] > 1 else ""
                if "bazel-testlogs" in command:
                    if info_error:
                        raise subprocess.CalledProcessError(7, command)
                    return str(logs) + "\n"
                raise AssertionError(command)
            output = base / "receipt"
            with patch.object(sys, "argv", ["e2e", "--output", str(output)]), patch.object(e2e.platform, "system", return_value="Linux"), patch.object(e2e.resource, "getrlimit", return_value=(-1, -1)), patch.object(e2e.resource, "setrlimit"), patch.object(e2e.subprocess, "check_output", side_effect=check_output), patch.object(e2e.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), contextlib.redirect_stdout(io.StringIO()):
                result = e2e.main()
            return result, json.loads((output / "receipt.json").read_text())

    def test_complete_clean_source_passes(self):
        code, receipt = self.exercise()
        self.assertEqual(code, 0)
        self.assertTrue(receipt["source_clean"])
        self.assertEqual(receipt["memory_limit_bytes_per_process"], 6 * 1024**3)
        self.assertFalse(receipt["live_acceptance"])

    def test_same_head_dirty_source_refuses(self):
        code, receipt = self.exercise(dirty=True)
        self.assertEqual(code, 1)
        self.assertTrue(receipt["source_unchanged"])
        self.assertFalse(receipt["source_clean"])
        self.assertEqual(receipt["verdict"], "failed")

    def test_log_discovery_failure_preserves_failed_receipt(self):
        code, receipt = self.exercise(info_error=True)
        self.assertEqual(code, 1)
        self.assertEqual(receipt["exit_code"], 0)
        self.assertEqual(receipt["verdict"], "failed")
        self.assertIn("error", receipt)
        self.assertIn("bazel_log_sha256", receipt)

if __name__ == "__main__":
    unittest.main()
