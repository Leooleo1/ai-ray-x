#!/usr/bin/env python3
"""Writes latest.json, the file the desktop app's updater reads from the latest release.

    scripts/update-manifest.py 0.1.1 "What changed" \
        --mac target/release/bundle/macos/raiox.app.tar.gz \
        --windows target/x86_64-pc-windows-msvc/release/bundle/nsis/raiox_0.1.1_x64-setup.exe \
        > latest.json

Each file needs its .sig next to it (the build makes it when TAURI_SIGNING_PRIVATE_KEY is set).
Upload the files, their .sig and latest.json to the release.
"""
import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = "https://github.com/Leooleo1/ai-ray-x/releases/download"


def entry(path: str, base: str) -> dict:
    file = Path(path)
    signature = Path(f"{path}.sig")
    if not file.is_file() or not signature.is_file():
        sys.exit(f"missing {file} or its .sig")
    return {"signature": signature.read_text().strip(), "url": f"{base}/{file.name}"}


def main() -> None:
    parser = argparse.ArgumentParser(description="latest.json for the raiox app's updater")
    parser.add_argument("version")
    parser.add_argument("notes")
    parser.add_argument("--mac", help="the .app.tar.gz (Apple Silicon)")
    parser.add_argument("--windows", help="the NSIS -setup.exe (x64)")
    parser.add_argument("--base", help="where the files are served (default: the release on GitHub)")
    args = parser.parse_args()
    base = args.base or f"{REPO}/v{args.version}"
    platforms = {}
    if args.mac:
        platforms["darwin-aarch64"] = entry(args.mac, base)
    if args.windows:
        platforms["windows-x86_64"] = entry(args.windows, base)
    if not platforms:
        sys.exit("give --mac, --windows or both")
    manifest = {"version": args.version, "notes": args.notes, "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), "platforms": platforms}
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
