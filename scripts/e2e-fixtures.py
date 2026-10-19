#!/usr/bin/env python3
"""Run the owned switchboard fixtures and write a receipt; never contact a live broker.

Carried from PR #32 (TIN-4655) onto the v0.3.0 tree. Receipt authority:
R-C949 (TIN-6688), TIN-4655, SWB-R49, R-N13. Linux build hosts only (sting
or honey), never neo; on sting run it under `tinyland-heavy`.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

TARGETS = [
    "//crates/swb:swb_test",
    "//crates/swb:hook_blackhole_test",
    "//crates/swb-store:swb_store_test",
    "//crates/swb-broker:swb_broker_test",
    "//crates/swb:swb",
]


def rust_case(name):
    return f"test tests::{name} ... ok"


# target -> {case name: the line its test.log must contain}
EXPECTED = {
    "crates/swb-broker:swb_broker_test": {
        name: rust_case(name)
        for name in [
            "rmcp_streamable_http_initialize_and_tools",
            "conflict_is_opaque_over_rest",
            "end_over_rest_needs_the_registered_token",
            "owned_http_round_trip_survives_graceful_service_reopen",
        ]
    },
    "crates/swb:swb_test": {
        name: rust_case(name)
        for name in [
            "explicit_broker_round_trip_and_session_identity",
            "stalled_resolution_worker_cannot_hold_caller",
            "stalled_http_read_cannot_hold_caller",
        ]
    },
    "crates/swb:hook_blackhole_test": {
        "hook_blackholed_broker_exit_0_within_2s": "all hook cases returned exit 0 within 2 s against a blackholed broker",
    },
    "crates/swb-store:swb_store_test": {
        name: rust_case(name)
        for name in [
            "round_trip_and_opaque_conflict",
            "wal_survives_reopen_and_sequences_thread",
            "end_requires_the_latest_registration_token",
        ]
    },
}
RULINGS = ["R-C949", "TIN-6688", "TIN-4655", "SWB-R49", "R-N13"]

def own_cgroup():
    try:
        lines = Path("/proc/self/cgroup").read_text().splitlines()
    except OSError:
        return None
    # cgroup v2 has the single line "0::<path>".
    paths = [line.split(":", 2)[2] for line in lines if line.startswith("0::")]
    return paths[0] if paths else None

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path)
    p.add_argument("--plan", action="store_true")
    p.add_argument("--cpus", type=int, default=4, help="CPUs to pin the run to (default 4)")
    a = p.parse_args()
    if a.cpus < 1:
        p.error("--cpus must be at least 1")
    repo = Path(__file__).resolve().parents[1]
    sha = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    output_base = str(a.output.resolve() / "bazel-output-base") if a.output else "<run-output>/bazel-output-base"
    startup = ["bazelisk", "--batch", "--ignore_all_rc_files", "--host_jvm_args=-Xmx512m", f"--host_jvm_args=-XX:ActiveProcessorCount={a.cpus}", "--host_jvm_args=-Xss512k", "--output_base=" + output_base]
    command = [*startup, "test", "--lockfile_mode=error", f"--jobs={a.cpus}", f"--local_resources=cpu={a.cpus}", "--legacy_globbing_threads=1", "--loading_phase_threads=1", "--local_test_jobs=1", "--test_output=all", "--test_arg=--test-threads=1", "--action_env=MALLOC_ARENA_MAX=2", "--action_env=GLIBC_TUNABLES=", "--host_action_env=MALLOC_ARENA_MAX=2", "--host_action_env=GLIBC_TUNABLES=", "--test_env=MALLOC_ARENA_MAX=2", "--test_env=GLIBC_TUNABLES=", *TARGETS]
    plan = {"schema": "swb.owned-fixture-receipt.v1", "source_sha": sha, "command": command, "expected_cases": EXPECTED, "qualification": "owned-fixtures-only", "live_acceptance": False, "process_actions": False, "rulings": RULINGS}
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
    try:
        allowed_cpus = os.sched_getaffinity(0)
        if len(allowed_cpus) < a.cpus:
            raise RuntimeError(f"{len(allowed_cpus)} allowed CPUs, {a.cpus} requested")
        selected = set(sorted(allowed_cpus)[: a.cpus])
        os.sched_setaffinity(0, selected)
        admitted_cpus = os.sched_getaffinity(0)
        if admitted_cpus != selected or not admitted_cpus.issubset(allowed_cpus):
            raise RuntimeError("CPU affinity readback mismatch")
    except (OSError, RuntimeError) as exc:
        p.error(f"owned CPU affinity admission failed: {exc}")
    plan["cpu_affinity"] = sorted(admitted_cpus)
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    # Memory is bounded by the caller's cgroup (on sting, tinyland-heavy's
    # slice), not by RLIMIT_AS: an address-space limit counts reservations,
    # and the Nix evaluator behind rules_nixpkgs fails a 6 GiB one with
    # "allocating arena using mmap: Cannot allocate memory" (sting, 2026-10-19).
    plan["memory_bound"] = {"kind": "cgroup", "cgroup": own_cgroup()}
    plan["jvm_heap_limit_mib"] = 512
    plan["glibc_malloc_arena_max"] = 2
    env = dict(os.environ)
    # Fixed internal Bazel pools can reserve many glibc arenas despite jobs=1.
    # Scope this bound to task children; ambient tunables must not override it.
    env.pop("GLIBC_TUNABLES", None)
    env["MALLOC_ARENA_MAX"] = "2"
    for key in list(env):
        if key.startswith("SWB_") or key in {"CARGO_BAZEL_DEBUG", "CARGO_BAZEL_GENERATOR_SHA256", "CARGO_BAZEL_GENERATOR_URL", "CARGO_BAZEL_ISOLATED", "CARGO_BAZEL_REPIN", "CARGO_BAZEL_REPIN_ONLY", "CARGO_BAZEL_TIMEOUT", "REPIN"}:
            env.pop(key)
    plan["started_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    plan.update(exit_code=None, cases={}, source_unchanged=False, source_clean=False, verdict="failed")
    try:
        with (output / "bazel.log").open("x") as log:
            result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT, check=False)
        plan["exit_code"] = result.returncode
        if result.returncode != 0:
            raise subprocess.CalledProcessError(result.returncode, command)
        # Batch info uses no persistent server and no process lifecycle action.
        testlogs = Path(subprocess.check_output([*startup, "info", "bazel-testlogs"], cwd=repo, env=env, text=True).strip())
        bindir = Path(subprocess.check_output([*startup, "info", "bazel-bin"], cwd=repo, env=env, text=True).strip())
        binary = bindir / "crates/swb/swb"
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise OSError("required compiled swb executable is missing")
        with binary.open("rb") as compiled:
            if compiled.read(4) != b"\x7fELF":
                raise OSError("required compiled swb executable is not ELF")
        artifacts = output / "artifacts"
        artifacts.mkdir(mode=0o700)
        admitted_binary = artifacts / "swb"
        shutil.copyfile(binary, admitted_binary)
        admitted_binary.chmod(0o500)
        artifacts.chmod(0o500)
        plan["compiled_binary"] = {
            "path": str(admitted_binary),
            "sha256": hashlib.sha256(admitted_binary.read_bytes()).hexdigest(),
            "source_sha": sha,
            "target": "//crates/swb:swb",
            "format": "ELF",
            "mode": "0500",
            "original_bazel_path": str(binary.resolve()),
        }
        for target, cases in EXPECTED.items():
            logpath = testlogs / target.replace(":", "/") / "test.log"
            text = logpath.read_text() if logpath.is_file() else ""
            plan["cases"][target] = {name: marker in text for name, marker in cases.items()}
        plan["source_unchanged"] = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip() == sha
        plan["source_clean"] = not subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain", "--untracked-files=all"], text=True).strip()
        if result.returncode == 0 and plan["source_unchanged"] and plan["source_clean"] and all(all(items.values()) for items in plan["cases"].values()):
            plan["verdict"] = "passed"
    except (OSError, subprocess.CalledProcessError) as exc:
        plan["error"] = str(exc)
    plan["finished_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    plan["bazel_log_sha256"] = hashlib.sha256((output / "bazel.log").read_bytes()).hexdigest()
    (output / "receipt.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
    print(json.dumps(plan, sort_keys=True))
    return 0 if plan["verdict"] == "passed" else 1

if __name__ == "__main__":
    sys.exit(main())
