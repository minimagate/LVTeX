#!/usr/bin/env python3
"""Generate THIRD_PARTY_LICENSES.md for the crates shipped in the mathstral app.

Run from the repository root:

    python3 scripts/third-party-licenses.py

Pass --offline (or set CARGO_NET_OFFLINE=true) to resolve dependencies from the
local cargo cache only. Uses the Python 3 standard library only.
"""

import json
import os
import re
import subprocess
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUTPUT = os.path.join(REPO_ROOT, "THIRD_PARTY_LICENSES.md")
TARGET = "aarch64-apple-darwin"
ROOT_PACKAGE = "mathstral"
LICENSE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")
NORMAL_AND_BUILD = (None, "build")
# Exception identifiers have no standalone file; their text is located by a marker.
EXCEPTION_MARKERS = {"LLVM-exception": "--- LLVM Exceptions to the Apache 2.0 License ---"}


def offline_requested(argv):
    if "--offline" in argv:
        return True
    value = os.environ.get("CARGO_NET_OFFLINE", "").strip().lower()
    return value not in ("", "0", "false", "no")


def configure_cargo(argv):
    """Use the repo-local cargo cache when present and CARGO_HOME is unset.

    Returns True when cargo should run in offline mode."""
    repo_cache = os.path.join(REPO_ROOT, ".cargo-home")
    if "CARGO_HOME" not in os.environ and os.path.isdir(os.path.join(repo_cache, "registry")):
        os.environ["CARGO_HOME"] = repo_cache
        return True
    return offline_requested(argv)


def cargo_metadata(offline):
    cmd = [
        "cargo", "metadata", "--format-version", "1", "--locked",
        "--filter-platform", TARGET,
    ]
    if offline:
        cmd.append("--offline")
    result = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        sys.exit(result.returncode)
    return json.loads(result.stdout)


def reachable_packages(metadata):
    """Return the set of package ids reachable from the root package through
    normal and build dependencies only."""
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root_ids = [
        p["id"] for p in metadata["packages"]
        if p["name"] == ROOT_PACKAGE and p["source"] is None
    ]
    if len(root_ids) != 1:
        sys.exit(f"error: expected one '{ROOT_PACKAGE}' package, found {len(root_ids)}")

    seen = set()
    stack = [root_ids[0]]
    while stack:
        pkg_id = stack.pop()
        if pkg_id in seen:
            continue
        seen.add(pkg_id)
        for dep in nodes[pkg_id]["deps"]:
            kinds = {k.get("kind") for k in dep.get("dep_kinds", [])}
            if kinds & set(NORMAL_AND_BUILD):
                stack.append(dep["pkg"])

    # Keep only third-party packages: workspace path crates have no source.
    third_party = [
        packages[i] for i in seen
        if i in packages and packages[i]["source"] is not None
    ]
    third_party.sort(key=lambda p: (p["name"], p["version"]))
    return third_party


def license_files(package):
    manifest_dir = os.path.dirname(package["manifest_path"])
    found = []
    try:
        entries = sorted(os.listdir(manifest_dir))
    except OSError:
        entries = []
    for entry in entries:
        path = os.path.join(manifest_dir, entry)
        if not os.path.isfile(path):
            continue
        if entry.upper().startswith(LICENSE_PREFIXES):
            found.append(entry)

    declared = package.get("license_file")
    if declared:
        declared = os.path.normpath(declared)
        if os.path.isfile(os.path.join(manifest_dir, declared)) and declared not in found:
            found.append(declared)
    return found


def read_bytes(package, name):
    with open(os.path.join(os.path.dirname(package["manifest_path"]), name), "rb") as fh:
        return fh.read()


def cell(value):
    if not value:
        return "-"
    return str(value).replace("|", "\\|").replace("\n", " ")


def fence_for(text):
    longest = 0
    run = 0
    for ch in text:
        run = run + 1 if ch == "`" else 0
        longest = max(longest, run)
    return "`" * max(3, longest + 1)


def spdx_ids(expression):
    """Return the license identifiers in an SPDX expression."""
    text = expression.replace("(", " ").replace(")", " ").replace("/", " ")
    return {t for t in text.split() if t not in ("OR", "AND", "WITH")}


def repo_text(name):
    with open(os.path.join(REPO_ROOT, name), "rb") as fh:
        return fh.read().decode("utf-8", errors="replace").rstrip("\n")


def appendix_text(ident, candidates):
    """Return (source description, text) for a license identifier, or None."""
    if ident == "Apache-2.0":
        return "Mathstral's LICENSE-APACHE, verbatim.", repo_text("LICENSE-APACHE")
    if ident == "MIT":
        text = repo_text("LICENSE-MIT").replace(
            "Copyright (c) 2026 Michelangelo Gubinelli",
            "Copyright (c) <copyright holders>",
        )
        return "Mathstral's LICENSE-MIT, with the copyright line left as a placeholder.", text
    marker = EXCEPTION_MARKERS.get(ident)
    if marker:
        for p in candidates:
            for name in license_files(p):
                text = read_bytes(p, name).decode("utf-8", errors="replace")
                if marker in text:
                    body = text[text.index(marker):].rstrip("\n")
                    return f"{p['name']} {p['version']}'s {name}, verbatim (exception text only).", body
    for p in candidates:
        if (p.get("license") or "").strip() == ident:
            for name in license_files(p):
                text = read_bytes(p, name).decode("utf-8", errors="replace").rstrip("\n")
                return f"{p['name']} {p['version']}'s {name}, verbatim.", text
    # Fall back to a license file named after the identifier, e.g. LICENSE-0BSD.
    key = re.sub(r"[^a-z0-9]", "", ident.lower())
    for p in candidates:
        for name in license_files(p):
            if key and key in re.sub(r"[^a-z0-9]", "", name.lower()):
                text = read_bytes(p, name).decode("utf-8", errors="replace").rstrip("\n")
                return f"{p['name']} {p['version']}'s {name}, verbatim.", text
    return None


def top_level_alternatives(expression):
    """Split an SPDX expression on its top-level OR operators."""
    expr = expression.strip()
    while expr.startswith("(") and expr.endswith(")"):
        depth, wraps = 0, True
        for i, ch in enumerate(expr):
            depth += (ch == "(") - (ch == ")")
            if depth == 0 and i < len(expr) - 1:
                wraps = False
                break
        if not wraps:
            break
        expr = expr[1:-1].strip()
    parts, depth, current, i = [], 0, "", 0
    while i < len(expr):
        ch = expr[i]
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if depth == 0 and expr.startswith(" OR ", i):
            parts.append(current.strip())
            current = ""
            i += 4
            continue
        current += ch
        i += 1
    parts.append(current.strip())
    return [p for p in parts if p]


def appendix_section(missing, candidates):
    """Return (lines, elected) where elected maps "name version" of a crate to
    the alternative Mathstral uses when an identifier has no local text."""
    ids = sorted({i for p in missing for i in spdx_ids(p.get("license") or "")})
    if not ids:
        return [], {}
    found_map = {ident: appendix_text(ident, candidates) for ident in ids}
    reproduced = {ident for ident, found in found_map.items() if found}
    elected = {}
    notes = {}
    for ident in ids:
        if found_map[ident]:
            continue
        mentioning = [p for p in missing if ident in spdx_ids(p.get("license") or "")]
        choices, alternatives, ok = {}, set(), bool(mentioning)
        for p in mentioning:
            qualifying = [
                alt for alt in top_level_alternatives(p["license"])
                if ident not in spdx_ids(alt) and spdx_ids(alt) and spdx_ids(alt) <= reproduced
            ]
            if not qualifying:
                ok = False
                break
            key = f"{p['name']} {p['version']}"
            choices[key] = qualifying[0]
            alternatives.update(qualifying)
        if ok:
            elected.update(choices)
            crates = ", ".join(sorted(choices))
            notes[ident] = (
                f"Not reproduced. Every crate offering `{ident}` also offers "
                f"{' or '.join(sorted(alternatives))}, reproduced in this appendix, "
                f"and Mathstral uses it under that license: {crates}."
            )
        else:
            notes[ident] = f"No license text for `{ident}` was found in the local cargo cache."
            print(
                f"warning: no reproduced text for {ident} and no reproduced "
                f"alternative covers every crate that uses it",
                file=sys.stderr,
            )

    lines = [
        "## Appendix: Standard license texts",
        "",
        "Each license identifier below appears in the SPDX expression of a crate "
        "listed above that has no license file. Each text is reproduced once.",
        "",
    ]
    for ident in ids:
        lines.append(f"### {ident}")
        lines.append("")
        found = found_map[ident]
        if found is None:
            lines.append(notes[ident])
            lines.append("")
            continue
        source, text = found
        lines.append(f"Source: {source}")
        lines.append("")
        fence = fence_for(text)
        lines.append(fence + "text")
        lines.append(text)
        lines.append(fence)
        lines.append("")
    return lines, elected


def vendored_sources_section():
    """Describe Typst sources copied into the repository rather than fetched as crates."""
    return [
        "## Vendored Typst sources",
        "",
        "### MiTeX Typst specification",
        "",
        "- Location: `crates/latex/vendor/mitex` (`prelude.typ` and `standard.typ`; "
        "the modifications are listed in its `README.md`).",
        "- Source: https://github.com/mitex-rs/mitex, directory `packages/mitex/specs`, "
        "commit `c5e9f9bf2` (MiTeX 0.2.4).",
        "- License: Apache-2.0. The license file is `crates/latex/vendor/mitex/LICENSE`, "
        "and the Apache-2.0 text is reproduced in Appendix: Standard license texts.",
        "",
    ]


def build_document(packages, candidates):
    missing_all = [p for p in packages if not license_files(p)]
    appendix, elected = appendix_section(missing_all, candidates)
    out = []
    out.append("# Third-party licenses")
    out.append("")
    out.append(
        "This file lists the third-party crates compiled into the Mathstral app "
        "(the `mathstral` package, target `aarch64-apple-darwin`) together with "
        "their license texts. It is generated; do not edit it by hand."
    )
    out.append("")
    out.append("To regenerate, run from the repository root:")
    out.append("")
    out.append("```sh")
    out.append("python3 scripts/third-party-licenses.py")
    out.append("```")
    out.append("")
    out.append(
        "Fonts bundled in `assets/fonts/` are not Rust crates and are covered in "
        "`assets/fonts/README.md` and `assets/fonts/UPSTREAM-NOTICE`."
    )
    out.append("")
    out.append("## Summary")
    out.append("")
    out.append(f"{len(packages)} crates.")
    out.append("")
    out.append("| Crate | Version | License | Repository |")
    out.append("| --- | --- | --- | --- |")
    for p in packages:
        out.append(
            f"| {cell(p['name'])} | {cell(p['version'])} | "
            f"{cell(p.get('license') or 'unknown')} | {cell(p.get('repository'))} |"
        )
    out.append("")
    out.append("## License texts")
    out.append("")

    first_seen = {}  # text bytes -> "name version / file"
    missing = []
    for p in packages:
        heading = f"{p['name']} {p['version']}"
        out.append(f"### {heading}")
        out.append("")
        out.append(f"- License: {p.get('license') or 'unknown'}")
        if p.get("repository"):
            out.append(f"- Repository: {p['repository']}")
        out.append("")

        files = license_files(p)
        if not files:
            missing.append(p)
            authors = "; ".join(p.get("authors") or []) or "none listed"
            out.append(
                f"No license file found in the crate. SPDX expression: "
                f"`{p.get('license') or 'unknown'}`."
            )
            out.append("")
            if heading in elected:
                out.append(f"Used under: {elected[heading]}.")
                out.append("")
            out.append(f"Copyright holders per package metadata: {authors}.")
            out.append("")
            out.append("The full license text is reproduced in Appendix: Standard license texts.")
            out.append("")
            continue

        for name in files:
            text = read_bytes(p, name)
            out.append(f"#### {name}")
            out.append("")
            if text in first_seen:
                out.append(f"Same text as {first_seen[text]}.")
                out.append("")
                continue
            first_seen[text] = f"{heading}'s {name}"
            decoded = text.decode("utf-8", errors="replace").rstrip("\n")
            fence = fence_for(decoded)
            out.append(fence + "text")
            out.append(decoded)
            out.append(fence)
            out.append("")

    out.extend(vendored_sources_section())
    if appendix:
        out.extend(appendix)
    return "\n".join(out).rstrip("\n") + "\n", missing


def main(argv):
    offline = configure_cargo(argv)
    metadata = cargo_metadata(offline)
    packages = reachable_packages(metadata)
    # Every crate in the resolved metadata, used to find verbatim license texts.
    candidates = sorted(
        (p for p in metadata["packages"] if p["source"] is not None),
        key=lambda p: (p["name"], p["version"]),
    )
    document, missing = build_document(packages, candidates)
    with open(OUTPUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(document)

    print(f"crates: {len(packages)}")
    print(f"without a license file: {len(missing)}")
    for p in missing:
        print(f"  {p['name']} {p['version']} ({p.get('license') or 'unknown'})")
    print(f"wrote {os.path.relpath(OUTPUT, REPO_ROOT)} ({os.path.getsize(OUTPUT)} bytes)")


if __name__ == "__main__":
    main(sys.argv[1:])
