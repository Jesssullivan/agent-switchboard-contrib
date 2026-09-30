#!/usr/bin/env python3
"""Run existing owned switchboard fixtures; never contact a live broker.

Receipt authority: LAB-TAKEOVER-20260930, TIN-4655, SWB-R49, R-N13.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

TARGETS = ["//crates/swb:swb_test", "//crates/swb-store:swb_store_test"]
EXPECTED = {
    "crates/swb:swb_test": ["explicit_broker_round_trip_and_session_identity", "stalled_resolution_worker_cannot_hold_caller", "stalled_http_read_cannot_hold_caller"],
    "crates/swb-store:swb_store_test": ["round_trip_and_opaque_conflict", "wal_survives_reopen_and_sequences_thread", "end_requires_matching_process_start"],
}

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path)
    p.add_argument("--plan", action="store_true")
    a = p.parse_args()
    repo = Path(__file__).resolve().parents[1]
    sha = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    command = ["bazelisk", "--ignore_all_rc_files", "test", "--lockfile_mode=error", "--jobs=1", "--local_cpu_resources=1", "--local_test_jobs=1", "--test_output=all", "--test_arg=--test-threads=1", *TARGETS]
    plan = {"schema": "swb.owned-fixture-receipt.v1", "source_sha": sha, "command": command, "expected_cases": EXPECTED, "qualification": "owned-fixtures-only", "live_acceptance": False, "process_actions": False, "rulings": ["LAB-TAKEOVER-20260930", "TIN-4655", "SWB-R49", "R-N13"]}
    if a.plan:
        print(json.dumps(plan, indent=2, sort_keys=True))
        return 0
    if platform.system() != "Linux":
        p.error("fixture compilation runs on Honey or Sting, never Neo")
    if not a.output:
        p.error("output is required for execution")
    if subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain", "--untracked-files=all"], text=True).strip():
        p.error("exact clean committed source required")
    output = a.output.resolve()
    if output == repo or repo in output.parents:
        p.error("receipt output must be outside source")
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    env = dict(os.environ)
    for key in list(env):
        if key.startswith("SWB_") or key in {"CARGO_BAZEL_DEBUG", "CARGO_BAZEL_GENERATOR_SHA256", "CARGO_BAZEL_GENERATOR_URL", "CARGO_BAZEL_ISOLATED", "CARGO_BAZEL_REPIN", "CARGO_BAZEL_REPIN_ONLY", "CARGO_BAZEL_TIMEOUT", "REPIN"}:
            env.pop(key)
    plan["started_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (output / "bazel.log").open("x") as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT, check=False)
    # Fresh case readback prevents a cached/empty target from masquerading as
    # the requested owned-fixture proof. These are Rust harness test logs.
    testlogs = Path(subprocess.check_output(["bazelisk", "--ignore_all_rc_files", "info", "bazel-testlogs"], cwd=repo, env=env, text=True).strip())
    cases = {}
    for target, names in EXPECTED.items():
        logpath = testlogs / target.replace(":", "/") / "test.log"
        text = logpath.read_text() if logpath.is_file() else ""
        cases[target] = {name: f"test tests::{name} ... ok" in text for name in names}
    plan.update(exit_code=result.returncode, cases=cases, finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
    plan["source_unchanged"] = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip() == sha
    plan["verdict"] = "passed" if result.returncode == 0 and plan["source_unchanged"] and all(all(items.values()) for items in cases.values()) else "failed"
    plan["bazel_log_sha256"] = hashlib.sha256((output / "bazel.log").read_bytes()).hexdigest()
    (output / "receipt.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
    print(json.dumps(plan, sort_keys=True))
    return 0 if plan["verdict"] == "passed" else 1

if __name__ == "__main__":
    sys.exit(main())
