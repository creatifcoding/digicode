#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import argparse
import fcntl
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("rust_cache_gc.py")
SPEC = importlib.util.spec_from_file_location("rust_cache_gc", MODULE_PATH)
assert SPEC and SPEC.loader
rust_cache_gc = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = rust_cache_gc
SPEC.loader.exec_module(rust_cache_gc)


class RustCacheGcTests(unittest.TestCase):
    def candidate(
        self,
        path: str,
        size: int,
        modified: float,
        *,
        active: bool = False,
        recent: bool = False,
        priority: int = 2,
    ):
        return rust_cache_gc.Candidate(path, size, modified, active, recent, priority)

    def test_selects_scratch_then_oldest_until_both_limits_are_met(self):
        candidates = [
            self.candidate("/repo/target", 30, 30, priority=2),
            self.candidate("/home/u/.jcode/scratch/a/target", 25, 20, priority=0),
            self.candidate("/home/u/.jcode/scratch/b/target", 20, 10, priority=0),
        ]
        selected = rust_cache_gc.select_deletions(
            candidates, free_bytes=40, reserve_bytes=70, max_target_bytes=35
        )
        self.assertEqual([item.path for item in selected], [candidates[2].path, candidates[1].path])

    def test_active_and_recent_targets_are_never_selected(self):
        candidates = [
            self.candidate("/active/target", 100, 1, active=True),
            self.candidate("/recent/target", 100, 2, recent=True),
            self.candidate("/safe/target", 100, 3),
        ]
        selected = rust_cache_gc.select_deletions(
            candidates, free_bytes=0, reserve_bytes=50, max_target_bytes=0
        )
        self.assertEqual([item.path for item in selected], ["/safe/target"])

    def test_discovery_requires_cargo_target_markers_and_skips_builds(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            valid = root / "project" / "target"
            valid.mkdir(parents=True)
            (valid / "CACHEDIR.TAG").write_text(
                "Signature: 8a477f597d28d172789f06886806bc55\n"
                "# This file is a cache directory tag created by cargo.\n",
                encoding="utf-8",
            )
            invalid = root / "notes" / "target"
            invalid.mkdir(parents=True)
            (invalid / "CACHEDIR.TAG").write_text("generic cache", encoding="utf-8")
            published = root / "builds" / "target"
            published.mkdir(parents=True)
            (published / ".rustc_info.json").write_text("{}", encoding="utf-8")
            explicit = root / "scratch" / "compile-cache-123"
            (explicit / "debug" / "deps").mkdir(parents=True)
            (explicit / "debug" / ".cargo-lock").write_text("", encoding="utf-8")
            self.assertEqual(
                rust_cache_gc.discover_targets([root]),
                [valid.resolve(), explicit.resolve()],
            )

    def test_explicit_root_does_not_expand_to_the_default_roots_file(self):
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            explicit = base / "explicit"
            implicit = base / "implicit"
            explicit.mkdir()
            implicit.mkdir()
            roots_file = base / "roots"
            roots_file.write_text(f"{implicit}\n", encoding="utf-8")
            args = argparse.Namespace(root=[str(explicit)], roots_file=str(roots_file))
            self.assertEqual(rust_cache_gc.configured_roots(args), [explicit.resolve()])

    def test_directory_stats_reports_bytes_and_target_mtime(self):
        with tempfile.TemporaryDirectory() as raw:
            target = Path(raw) / "target"
            target.mkdir()
            first = target / "a"
            second = target / "b"
            first.write_bytes(b"123")
            time.sleep(0.01)
            second.write_bytes(b"4567")
            size, modified = rust_cache_gc.directory_stats(target)
            self.assertEqual(size, 7)
            self.assertEqual(modified, target.stat().st_mtime)
            self.assertTrue(rust_cache_gc.path_recently_modified(target, time.time() - 60))
            self.assertFalse(rust_cache_gc.path_recently_modified(target, time.time() + 60))

    def test_active_process_matches_target_or_project_working_directory(self):
        target = Path("/repo/project/target")
        self.assertTrue(rust_cache_gc.path_is_active(target, [("rustc --out-dir /repo/project/target/debug", "/other")]))
        self.assertTrue(rust_cache_gc.path_is_active(target, [("cargo build", "/repo/project")]))
        self.assertFalse(rust_cache_gc.path_is_active(target, [("cargo build", "/repo/other")]))

    def test_second_collector_exits_cleanly_when_lock_is_held(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            lock_path = root / "collector.lock"
            with lock_path.open("a+", encoding="utf-8") as lock:
                fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                result = subprocess.run(
                    [
                        sys.executable,
                        str(MODULE_PATH),
                        "--root",
                        str(root),
                        "--lock-file",
                        str(lock_path),
                    ],
                    check=True,
                    capture_output=True,
                    text=True,
                )
            self.assertIn("another collector owns", result.stdout)


if __name__ == "__main__":
    unittest.main()
