#!/usr/bin/env python3
"""specbook.py — source specification manager.

Specs live in .specs/<hash>.md where <hash> = sha256(source)[:16].
Code changes -> hash changes -> spec becomes stale -> must be refreshed.
`sync` merges all valid specs into SPECBOOK.md for consumers.

Commands:
  status                  check all spec freshness
  sync                    build SPECBOOK.md from valid specs
  init <file> [file...]   create spec templates for source files
  refresh [file|--all]    migrate stale specs to current hashes
"""

import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPECS_DIR = ROOT / ".specs"
SPECBOOK = ROOT / "SPECBOOK.md"

SUBSYSTEMS = ["core", "app", "admin-ui", "adapters", "ops"]


def content_hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()[:16]


def parse_spec(path: Path) -> dict | None:
    text = path.read_text(encoding="utf-8")
    m = re.match(r"^---\s*\nfile:\s*(.+?)\s*\n---\s*\n", text)
    if not m:
        return None
    return {
        "file": m.group(1),
        "hash": path.stem,
        "body": text[m.end() :],
        "path": path,
    }


def all_specs() -> list[dict]:
    if not SPECS_DIR.exists():
        return []
    return [s for p in sorted(SPECS_DIR.glob("*.md")) if (s := parse_spec(p))]


def classify(specs: list[dict]):
    valid, stale, gone = [], [], []
    for s in specs:
        src = ROOT / s["file"]
        if not src.exists():
            gone.append(s)
        elif content_hash(src) == s["hash"]:
            valid.append(s)
        else:
            s["current_hash"] = content_hash(src)
            stale.append(s)
    return valid, stale, gone


def find_spec_for(source: str, specs: list[dict]) -> dict | None:
    for s in specs:
        if s["file"] == source:
            return s
    return None


def subsystem_of(filepath: str) -> str:
    top = filepath.split("/")[0]
    return top if top in SUBSYSTEMS else "other"


def cmd_status():
    specs = all_specs()
    if not specs:
        print("No specs in .specs/")
        return 1
    valid, stale, gone = classify(specs)
    for s in sorted(valid, key=lambda x: x["file"]):
        print(f"  OK    {s['file']}")
    for s in sorted(stale, key=lambda x: x["file"]):
        print(f"  STALE {s['file']}  (spec {s['hash']}, now {s['current_hash']})")
    for s in sorted(gone, key=lambda x: x["file"]):
        print(f"  GONE  {s['file']}")
    total = len(valid) + len(stale) + len(gone)
    print(f"\n{len(valid)}/{total} valid, {len(stale)} stale, {len(gone)} gone")
    return 1 if stale or gone else 0


def group_by_dir(specs: list[dict]) -> dict[str, list[dict]]:
    groups: dict[str, list[dict]] = {}
    for s in specs:
        dirpath = "/".join(s["file"].split("/")[:-1])
        groups.setdefault(dirpath, []).append(s)
    return groups


def module_of(filepath: str) -> str:
    parts = filepath.split("/")
    top = parts[0]
    if top == "core" and len(parts) >= 3:
        return f"core/{parts[2]}"
    if top == "app" and len(parts) >= 2:
        return f"app/{parts[1]}"
    if top == "admin-ui":
        return "admin-ui"
    if top == "adapters" and len(parts) >= 2:
        return f"adapters/{parts[1]}"
    if top == "ops":
        return "ops"
    return top


def cmd_sync():
    specs = all_specs()
    valid, stale, gone = classify(specs)

    if stale:
        for s in stale:
            print(f"  stale: {s['file']}")
    if gone:
        for s in gone:
            print(f"  gone:  {s['file']}")

    by_module: dict[str, list[dict]] = {}
    for s in valid:
        by_module.setdefault(module_of(s["file"]), []).append(s)

    outdir = ROOT / "docs" / "specbook"
    outdir.mkdir(parents=True, exist_ok=True)

    index_lines = [f"# Specbook\n\n{len(valid)} specs across {len(by_module)} modules.\n"]

    for mod in sorted(by_module.keys()):
        group = by_module[mod]
        groups = group_by_dir(group)
        slug = mod.replace("/", "-")
        filename = f"{slug}.md"

        parts = [f"# {mod}\n\n{len(group)} specs.\n"]
        for dirpath in sorted(groups.keys()):
            parts.append(f"\n{'=' * 60}")
            parts.append(f"{dirpath}/")
            parts.append(f"{'=' * 60}\n")
            for s in sorted(groups[dirpath], key=lambda x: x["file"]):
                fname = s["file"].split("/")[-1]
                parts.append(f"\n--- {fname} ---\n")
                body = s["body"].strip()
                if body:
                    parts.append(f"{body}\n")

        (outdir / filename).write_text("\n".join(parts), encoding="utf-8")
        index_lines.append(f"- [{mod}](docs/specbook/{filename}) — {len(group)} specs")

    index_path = ROOT / "SPECBOOK.md"
    index_path.write_text("\n".join(index_lines) + "\n", encoding="utf-8")
    print(f"Wrote {len(by_module)} module files to docs/specbook/ + SPECBOOK.md index ({len(valid)} specs)")
    return 0


def cmd_init(sources: list[str]):
    SPECS_DIR.mkdir(exist_ok=True)
    specs = all_specs()

    for source in sources:
        src = ROOT / source
        if not src.exists():
            print(f"  skip  {source} (not found)")
            continue

        existing = find_spec_for(source, specs)
        if existing:
            if content_hash(src) == existing["hash"]:
                print(f"  skip  {source} (spec exists and valid)")
            else:
                print(f"  skip  {source} (spec exists but stale — use refresh)")
            continue

        h = content_hash(src)
        spec_path = SPECS_DIR / f"{h}.md"
        spec_path.write_text(
            f"---\nfile: {source}\n---\n\n",
            encoding="utf-8",
        )
        print(f"  init  {source} -> .specs/{h}.md")
    return 0


def cmd_refresh(targets: list[str]):
    specs = all_specs()
    _, stale, _ = classify(specs)

    if not targets or targets == ["--all"]:
        to_refresh = stale
    else:
        to_refresh = [s for s in stale if s["file"] in targets]

    if not to_refresh:
        print("Nothing to refresh.")
        return 0

    for s in to_refresh:
        src = ROOT / s["file"]
        new_hash = content_hash(src)
        new_path = SPECS_DIR / f"{new_hash}.md"

        new_path.write_text(
            f"---\nfile: {s['file']}\n---\n\n{s['body']}",
            encoding="utf-8",
        )
        s["path"].unlink()
        print(f"  refresh  {s['file']}  {s['hash']} -> {new_hash}")

    print(f"\n{len(to_refresh)} specs migrated. Review each — code changed, spec may need updating.")
    return 0


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 1

    cmd = sys.argv[1]

    if cmd == "status":
        return cmd_status()
    elif cmd == "sync":
        return cmd_sync()
    elif cmd == "init":
        if len(sys.argv) < 3:
            print("Usage: specbook.py init <file> [file...]")
            return 1
        return cmd_init(sys.argv[2:])
    elif cmd == "refresh":
        return cmd_refresh(sys.argv[2:])
    else:
        print(f"Unknown: {cmd}\n")
        print(__doc__)
        return 1


if __name__ == "__main__":
    sys.exit(main() or 0)
