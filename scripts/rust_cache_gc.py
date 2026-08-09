#!/usr/bin/env python3
"""Bound Cargo target storage across Jcode-managed Rust workspaces.

The collector is conservative by default: it reports candidates, skips active or
recently-written targets, and only deletes with --apply. In automatic mode it
removes the oldest safe targets until both the filesystem reserve and aggregate
target budget are satisfied.
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable

GIB = 1024**3
TARGET_NAMES = {"target"}
TARGET_PREFIXES = ("target-", "compile-cache-")
SKIP_PARTS = {".cargo", ".git", ".rustup", "builds", "node_modules"}


@dataclass(frozen=True)
class Candidate:
    path: str
    bytes: int
    modified_at: float
    active: bool
    recent: bool
    priority: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="delete selected safe targets")
    parser.add_argument("--root", action="append", default=[], help="root to scan; repeatable")
    parser.add_argument(
        "--roots-file",
        default=None,
        help="newline-delimited scan roots",
    )
    parser.add_argument("--min-free-gib", type=float, default=100.0)
    parser.add_argument("--min-free-percent", type=float, default=15.0)
    parser.add_argument("--max-target-gib", type=float, default=80.0)
    parser.add_argument("--recent-minutes", type=int, default=30)
    parser.add_argument(
        "--lock-file",
        default=os.environ.get(
            "JCODE_RUST_CACHE_LOCK_FILE", str(Path.home() / ".jcode/locks/rust-cache-gc.lock")
        ),
    )
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def configured_roots(args: argparse.Namespace) -> list[Path]:
    raw = list(args.root)
    env_roots = os.environ.get("JCODE_RUST_CACHE_ROOTS", "")
    raw.extend(value for value in env_roots.split(os.pathsep) if value)
    roots_file = (
        Path(args.roots_file).expanduser()
        if args.roots_file
        else Path.home() / ".config/jcode/rust-cache-roots"
    )
    if not raw and roots_file.is_file():
        raw.extend(
            line.strip()
            for line in roots_file.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        )
    if not raw:
        raw = [str(Path.home() / ".jcode")]
    roots: list[Path] = []
    seen: set[Path] = set()
    for value in raw:
        path = Path(value).expanduser().resolve()
        if path.exists() and path not in seen:
            roots.append(path)
            seen.add(path)
    return roots


def is_target_name(name: str) -> bool:
    return name in TARGET_NAMES or name.startswith(TARGET_PREFIXES)


def is_cargo_target(path: Path) -> bool:
    if (path / ".rustc_info.json").is_file():
        return True
    cache_tag = path / "CACHEDIR.TAG"
    if cache_tag.is_file():
        try:
            if "created by cargo" in cache_tag.read_text(encoding="utf-8").lower():
                return True
        except OSError:
            pass
    return any(
        (path / profile / "deps").is_dir()
        and (
            (path / profile / ".cargo-lock").is_file()
            or (path / profile / ".cargo-artifact-lock").is_file()
        )
        for profile in ("debug", "release", "selfdev")
    )


def discover_targets_with_walk(roots: Iterable[Path]) -> list[Path]:
    found: list[Path] = []
    seen: set[Path] = set()
    for root in roots:
        for base, dirs, _files in os.walk(root, topdown=True):
            current = Path(base)
            dirs[:] = [
                name
                for name in dirs
                if name not in SKIP_PARTS and not (current.name == "target" and name != "package")
            ]
            for name in list(dirs):
                candidate = (current / name).resolve()
                if not is_target_name(name) and not is_cargo_target(candidate):
                    continue
                dirs.remove(name)
                if candidate in seen or not is_cargo_target(candidate):
                    continue
                found.append(candidate)
                seen.add(candidate)
    return found


def discover_targets(roots: Iterable[Path]) -> list[Path]:
    roots = list(roots)
    expression = ["-xdev", "(", "-type", "d", "("]
    for index, name in enumerate(sorted(SKIP_PARTS)):
        if index:
            expression.append("-o")
        expression.extend(["-name", name])
    expression.extend(
        [
            ")",
            ")",
            "-prune",
            "-o",
            "(",
            "-type",
            "d",
            "(",
            "-name",
            "target",
            "-o",
            "-name",
            "target-*",
            "-o",
            "-name",
            "compile-cache-*",
            ")",
            "-print0",
            "-prune",
            ")",
        ]
    )
    found: list[Path] = []
    seen: set[Path] = set()
    for root in roots:
        try:
            result = subprocess.run(
                ["find", str(root), *expression],
                check=False,
                capture_output=True,
                timeout=120,
            )
        except FileNotFoundError:
            return discover_targets_with_walk(roots)
        except subprocess.TimeoutExpired:
            print(f"rust_cache_gc: target discovery exceeded 120 seconds for {root}", file=sys.stderr)
            continue
        if result.returncode not in {0, 1}:
            detail = os.fsdecode(result.stderr).strip()
            print(
                f"rust_cache_gc: target discovery failed for {root}: {detail or result.returncode}",
                file=sys.stderr,
            )
            continue
        for raw in result.stdout.split(b"\0"):
            if not raw:
                continue
            candidate = Path(os.fsdecode(raw)).resolve()
            if candidate in seen or not is_cargo_target(candidate):
                continue
            found.append(candidate)
            seen.add(candidate)
    return found


def directory_stats(path: Path) -> tuple[int, float]:
    try:
        result = subprocess.run(
            ["du", "-sb", "--apparent-size", str(path)],
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() if isinstance(error.stderr, str) else ""
        message = detail or f"du exited with status {error.returncode}"
        raise OSError(f"cannot measure {path}: {message}") from error
    total = int(result.stdout.split(maxsplit=1)[0])
    return total, path.stat().st_mtime


def path_recently_modified(path: Path, cutoff: float) -> bool:
    try:
        result = subprocess.run(
            ["find", str(path), "-type", "f", "-newermt", f"@{cutoff}", "-print", "-quit"],
            check=False,
            capture_output=True,
            text=True,
        )
        return bool(result.stdout.strip())
    except OSError:
        for base, _dirs, files in os.walk(path):
            for name in files:
                try:
                    if (Path(base) / name).stat().st_mtime >= cutoff:
                        return True
                except OSError:
                    continue
        return False


def active_process_paths() -> list[tuple[str, str]]:
    active: list[tuple[str, str]] = []
    proc = Path("/proc")
    if not proc.is_dir():
        return active
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            cmdline = (entry / "cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")
            comm = (entry / "comm").read_text(encoding="utf-8").strip()
            if comm not in {"cargo", "rustc", "sccache", "clippy-driver"} and not any(
                token in cmdline for token in (" cargo ", "/cargo ", "/rustc ", "clippy-driver")
            ):
                continue
            cwd = os.readlink(entry / "cwd")
        except (OSError, UnicodeError):
            continue
        active.append((cmdline, cwd))
    return active


def path_is_active(path: Path, active: Iterable[tuple[str, str]]) -> bool:
    text = str(path)
    project = str(path.parent)
    for cmdline, cwd in active:
        if text in cmdline:
            return True
        if cwd == project or cwd.startswith(project + os.sep) or project.startswith(cwd + os.sep):
            return True
    return False


def priority_for(path: Path) -> int:
    text = str(path)
    if f"{os.sep}.jcode{os.sep}scratch{os.sep}" in text:
        return 0
    if path.name.startswith("target-"):
        return 1
    return 2


def filesystem_reserve(path: Path, min_gib: float, min_percent: float) -> tuple[int, int, int]:
    usage = shutil.disk_usage(path)
    reserve = max(int(min_gib * GIB), int(usage.total * min_percent / 100.0))
    return usage.free, reserve, usage.total


def select_deletions(
    candidates: list[Candidate], free_bytes: int, reserve_bytes: int, max_target_bytes: int
) -> list[Candidate]:
    safe = [candidate for candidate in candidates if not candidate.active and not candidate.recent]
    safe.sort(key=lambda item: (item.priority, item.modified_at, -item.bytes, item.path))
    target_total = sum(candidate.bytes for candidate in candidates)
    selected: list[Candidate] = []
    for candidate in safe:
        if free_bytes >= reserve_bytes and target_total <= max_target_bytes:
            break
        selected.append(candidate)
        free_bytes += candidate.bytes
        target_total -= candidate.bytes
    return selected


def main() -> int:
    args = parse_args()
    lock_path = Path(args.lock_file).expanduser()
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    lock = lock_path.open("a+", encoding="utf-8")
    try:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        print(f"rust_cache_gc: another collector owns {lock_path}")
        return 0
    roots = configured_roots(args)
    if not roots:
        print("rust_cache_gc: no configured roots exist", file=sys.stderr)
        return 0
    active = active_process_paths()
    cutoff = time.time() - args.recent_minutes * 60
    candidates: list[Candidate] = []
    for path in discover_targets(roots):
        try:
            size, modified = directory_stats(path)
        except OSError as error:
            print(f"rust_cache_gc: skip unreadable {path}: {error}", file=sys.stderr)
            continue
        candidates.append(
            Candidate(
                path=str(path),
                bytes=size,
                modified_at=modified,
                active=path_is_active(path, active),
                recent=path_recently_modified(path, cutoff),
                priority=priority_for(path),
            )
        )
    free_before, reserve, filesystem_total = filesystem_reserve(
        roots[0], args.min_free_gib, args.min_free_percent
    )
    max_target = int(args.max_target_gib * GIB)
    selected = select_deletions(candidates, free_before, reserve, max_target)
    reclaimed = 0
    deleted: list[str] = []
    failures: list[dict[str, str]] = []
    if args.apply:
        for candidate in selected:
            path = Path(candidate.path)
            try:
                shutil.rmtree(path)
            except OSError as error:
                failures.append({"path": candidate.path, "error": str(error)})
                continue
            reclaimed += candidate.bytes
            deleted.append(candidate.path)
    report = {
        "apply": args.apply,
        "roots": [str(root) for root in roots],
        "filesystemBytes": filesystem_total,
        "freeBytesBefore": free_before,
        "reserveBytes": reserve,
        "targetBytes": sum(candidate.bytes for candidate in candidates),
        "maxTargetBytes": max_target,
        "candidateCount": len(candidates),
        "selectedBytes": sum(candidate.bytes for candidate in selected),
        "selected": [asdict(candidate) for candidate in selected],
        "deleted": deleted,
        "reclaimedBytes": reclaimed,
        "failures": failures,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        action = "reclaimed" if args.apply else "reclaimable"
        print(
            f"rust_cache_gc: {action}={reclaimed if args.apply else report['selectedBytes']} "
            f"targets={len(candidates)} selected={len(selected)} "
            f"free={free_before} reserve={reserve} target_total={report['targetBytes']}"
        )
        for candidate in selected:
            verb = "removed" if candidate.path in deleted else "would remove"
            print(f"rust_cache_gc: {verb} {candidate.path} [{candidate.bytes} bytes]")
        for failure in failures:
            print(f"rust_cache_gc: FAILED {failure['path']}: {failure['error']}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
