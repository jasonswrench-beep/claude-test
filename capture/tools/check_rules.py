#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Checks the OpenStage Capture rules on every change (requirements story G4, rules R7 and R8).

It fails if:
  * a dependency has a license that is not on the allowed list (no GPL, no unknown licenses);
  * a dependency looks like FFmpeg (FFmpeg is never bundled);
  * any build file or script asks for an FFmpeg build with --enable-gpl or --enable-nonfree
    (--enable-version3 is only flagged for review);
  * a source file is missing its Apache-2.0 license header;
  * project prose or comments contain an em dash (OpenStage's writing rules avoid them).

Run it from anywhere:   python3 tools/check_rules.py
Write the third-party list:   python3 tools/check_rules.py --write-third-party
Exit code: 0 if every check passes, 1 if any fails.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent  # the capture/ folder
REPO = ROOT.parent

# Licenses that can be shipped inside an Apache-2.0 project (brief, section 7).
ALLOWED = {
    "Apache-2.0", "MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unlicense", "CC0-1.0",
    "0BSD", "BSL-1.0", "Unicode-3.0", "Unicode-DFS-2016",
}

SKIP_DIRS = {"target", ".git", "node_modules"}
HEADER_SUFFIXES = {".rs", ".py"}
EDIT = REPO / "capture-edit"  # the TypeScript edit model
EDIT_HEADER_SUFFIXES = {".ts"}
FLAG_SUFFIXES = {".toml", ".rs", ".bat", ".cmd", ".sh", ".yml", ".yaml", ".py", ".lock"}
STYLE_SUFFIXES = {".md", ".rs", ".bat", ".py", ".yml", ".yaml", ".toml"}
EM_DASH = chr(0x2014)  # built from its code so this file does not trip its own check

results = []


def record(status, name, detail=""):
    results.append((status, name, detail))


def files(base, suffixes):
    for path in sorted(base.rglob("*")):
        if path.is_file() and path.suffix in suffixes and not (set(path.parts) & SKIP_DIRS):
            yield path


# ---- license expressions: "MIT OR Apache-2.0", "(A OR B) AND C", "Apache-2.0 WITH LLVM-exception"

def tokenize(expr):
    return re.findall(r"\(|\)|[A-Za-z0-9.+\-]+", expr.replace("/", " OR "))


def license_ok(expr):
    tokens = tokenize(expr)
    pos = 0

    def parse_or():
        nonlocal pos
        value = parse_and()
        while pos < len(tokens) and tokens[pos] == "OR":
            pos += 1
            other = parse_and()
            value = value or other
        return value

    def parse_and():
        nonlocal pos
        value = parse_atom()
        while pos < len(tokens) and tokens[pos] == "AND":
            pos += 1
            other = parse_atom()
            value = value and other
        return value

    def parse_atom():
        nonlocal pos
        if pos >= len(tokens):
            return False
        token = tokens[pos]
        pos += 1
        if token == "(":
            value = parse_or()
            if pos < len(tokens) and tokens[pos] == ")":
                pos += 1
            return value
        ok = token.rstrip("+") in ALLOWED
        if pos < len(tokens) and tokens[pos] == "WITH":
            pos += 2  # an exception such as LLVM-exception only loosens a license
        return ok

    return parse_or()


def cargo_packages():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(ROOT / "Cargo.toml")],
        capture_output=True, text=True,
    )
    if out.returncode != 0:
        record("FAIL", "read the dependency list", out.stderr.strip()[-400:])
        return []
    return json.loads(out.stdout)["packages"]


def check_dependencies(packages):
    bad = []
    for pkg in packages:
        expr = pkg.get("license")
        if not expr:
            bad.append(f"{pkg['name']} {pkg['version']}: no license listed")
        elif not license_ok(expr):
            bad.append(f"{pkg['name']} {pkg['version']}: {expr}")
    if bad:
        record("FAIL", "dependency licenses", "not on the allowed list: " + "; ".join(bad))
    else:
        record("PASS", "dependency licenses", f"{len(packages)} packages, all on the allowed list")

    ffmpeg = [p["name"] for p in packages if "ffmpeg" in p["name"].lower()]
    if ffmpeg:
        record("FAIL", "no FFmpeg dependency", "FFmpeg is never bundled: " + ", ".join(ffmpeg))
    else:
        record("PASS", "no FFmpeg dependency")


def check_headers():
    missing = []
    for path in files(ROOT, HEADER_SUFFIXES):
        head = "\n".join(path.read_text(encoding="utf-8").splitlines()[:5])
        if "SPDX-License-Identifier: Apache-2.0" not in head:
            missing.append(str(path.relative_to(REPO)))
    if EDIT.is_dir():
        for path in files(EDIT, EDIT_HEADER_SUFFIXES):
            head = "\n".join(path.read_text(encoding="utf-8").splitlines()[:5])
            if "SPDX-License-Identifier: Apache-2.0" not in head:
                missing.append(str(path.relative_to(REPO)))
    if missing:
        record("FAIL", "license headers", "missing SPDX header: " + ", ".join(missing))
    else:
        record("PASS", "license headers")


def check_ffmpeg_flags():
    this = Path(__file__).resolve()
    hard, review = [], []
    for path in files(ROOT, FLAG_SUFFIXES):
        if path.resolve() == this:
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for flag in ("--enable-gpl", "--enable-nonfree"):
            if flag in text:
                hard.append(f"{path.relative_to(REPO)} uses {flag}")
        if "--enable-version3" in text:
            review.append(str(path.relative_to(REPO)))
    if hard:
        record("FAIL", "FFmpeg build flags", "; ".join(hard))
    else:
        record("PASS", "FFmpeg build flags", "no --enable-gpl or --enable-nonfree")
    if review:
        record("WARN", "FFmpeg --enable-version3", "needs review: " + ", ".join(review))


def check_style():
    found = []
    for base in (ROOT, REPO / "docs", EDIT):
        if not base.is_dir():
            continue
        for path in files(base, STYLE_SUFFIXES | {".ts", ".json"}):
            if EM_DASH in path.read_text(encoding="utf-8", errors="replace"):
                found.append(str(path.relative_to(REPO)))
    readme = REPO / "README.md"
    if readme.exists() and EM_DASH in readme.read_text(encoding="utf-8"):
        found.append("README.md")
    if found:
        record("FAIL", "writing style (no em dashes)", ", ".join(sorted(set(found))))
    else:
        record("PASS", "writing style (no em dashes)")


def batch_block_problems(text):
    """Lines of a Windows batch file that break it: an `echo` inside a ( ... ) block that contains a
    bracket. In a block, the first `)` ends it, so the rest of the line is run as a command and the
    whole script stops with a syntax error (and its window closes, which looks like a crash). Returns
    (line number, line) pairs. Escaping with ^ is allowed."""
    problems = []
    depth = 0
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip()
        if depth > 0 and line.lower().startswith("echo"):
            unescaped = re.sub(r"\^[()]", "", line)
            if "(" in unescaped or ")" in unescaped:
                problems.append((number, line))
        # A line ending in ( opens a block; a line starting with ) closes one.
        if line.endswith("("):
            depth += 1
        if line.startswith(")") and depth > 0:
            depth -= 1
    return problems


def check_edit_package():
    """The TypeScript edit model ships with no runtime dependencies at all, so there is nothing to license-check."""
    manifest = EDIT / "package.json"
    if not manifest.exists():
        return
    package = json.loads(manifest.read_text(encoding="utf-8"))
    runtime = package.get("dependencies") or {}
    if runtime:
        record("FAIL", "edit model has no runtime dependencies", "found: " + ", ".join(sorted(runtime)))
    else:
        record("PASS", "edit model has no runtime dependencies")


def batch_path_problems(text):
    """Lines that pass "%~dp0" (the script's folder, which ends in a backslash) as a quoted argument. A backslash
    before the closing quote escapes it, so the program receives the path with a stray quote and glued to the
    next argument. `cd /d "%~dp0"` is fine; for a program, write "%~dp0." instead. Returns (line number, line) pairs."""
    problems = []
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip()
        if '"%~dp0"' in line and not line.lower().startswith("cd "):
            problems.append((number, line))
    return problems


def check_batch_files():
    bad = []
    for path in sorted(ROOT.glob("*.bat")):
        text = path.read_text(encoding="ascii", errors="replace")
        for number, line in batch_block_problems(text):
            bad.append(f"{path.name} line {number}")
        for number, line in batch_path_problems(text):
            bad.append(f"{path.name} line {number} (a quoted folder ending in a backslash)")
    if bad:
        record("FAIL", "batch files (no brackets in echo lines inside blocks)", ", ".join(bad))
    else:
        record("PASS", "batch files (no brackets in echo lines inside blocks)")


def write_third_party(packages):
    members = {p["name"] for p in packages if p.get("source") is None}
    rows = sorted((p["name"], p["version"], p.get("license") or "unknown") for p in packages if p["name"] not in members)
    lines = [
        "# Third-party components",
        "",
        "Generated by `tools/check_rules.py --write-third-party`. Do not edit by hand.",
        "OpenStage Capture is Apache-2.0. These Rust crates are built into it, each under the license shown.",
        "",
        "| Crate | Version | License |",
        "|---|---|---|",
    ]
    lines += [f"| {name} | {version} | {lic} |" for name, version, lic in rows]
    (ROOT / "THIRD-PARTY.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote {ROOT / 'THIRD-PARTY.md'} ({len(rows)} crates)")


def main():
    packages = cargo_packages()
    if packages:
        check_dependencies(packages)
    check_headers()
    check_ffmpeg_flags()
    check_style()
    check_batch_files()
    check_edit_package()
    if "--write-third-party" in sys.argv and packages:
        write_third_party(packages)

    failed = 0
    for status, name, detail in results:
        print(f"{status:<5} {name}" + (f": {detail}" if detail else ""))
        failed += status == "FAIL"
    print()
    print("All rule checks passed." if not failed else f"{failed} rule check(s) FAILED.")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
