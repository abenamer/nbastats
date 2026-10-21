#!/usr/bin/env python3
"""Validate the project's OKF bundle structure, optional rule IDs, and local Markdown links.

Adapted from the REAS checker (https://github.com/abenamer/reas). This check does not establish
ASD-STE100 compliance.
"""
from __future__ import annotations
import argparse
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit
import yaml

LINK = re.compile(r"(?<!!)\[[^\]\n]+\]\(([^)]+)\)")
RULE = re.compile(r"^#{1,6}\s+(REAS-[A-Z]+-\d{3})\b", re.M)
ACTOR = re.compile(r"^(?:human:[^\s]+|process:[^\s]+|[^\s/]+/[^\s/]+)$")

def split_frontmatter(text: str):
    if not text.startswith("---\n"):
        return None, text, None
    match = re.search(r"\n---\s*\n", text[4:])
    if match is None:
        return None, text, "Unclosed YAML frontmatter"
    raw = text[4:][:match.start()]
    body = text[4:][match.end():]
    try:
        metadata = yaml.safe_load(raw)
    except yaml.YAMLError as exc:
        return None, text, f"Invalid YAML: {exc}"
    if not isinstance(metadata, dict):
        return None, text, "Frontmatter must be a mapping"
    return metadata, body, None

def outside_fences(body: str) -> str:
    lines = []
    fence = None
    for line in body.splitlines():
        match = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if match:
            marker = match.group(1)
            if fence is None:
                fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence):
                fence = None
            lines.append("")
        else:
            lines.append("" if fence else line)
    return "\n".join(lines)

def validate(root: Path, bundle_dir: str = "docs", require_rules: bool = False) -> list[str]:
    root = root.resolve()
    bundle = root / bundle_dir
    if not bundle.is_dir():
        return [f"The {bundle_dir}/ OKF bundle is missing"]
    errors = []
    seen = {}
    for path in sorted(root.rglob("*.md")):
        if ".git" in path.parts or "target" in path.parts or "node_modules" in path.parts:
            continue
        rel = path.relative_to(root).as_posix()
        try:
            content = path.read_text(encoding="utf-8")
        except (UnicodeError, OSError) as exc:
            errors.append(f"{rel}: Cannot read: {exc}")
            continue
        meta, body, err = split_frontmatter(content)
        if err:
            errors.append(f"{rel}: {err}")
        if path.is_relative_to(bundle):
            if path.name == "index.md":
                if path.parent == bundle:
                    if meta is None or set(meta) != {"okf_version"} or str(meta.get("okf_version")) != "0.2":
                        errors.append(f"{rel}: Bundle index must contain only okf_version: '0.2'")
                elif meta is not None:
                    errors.append(f"{rel}: Nested index must not contain frontmatter")
            else:
                if not isinstance(meta, dict) or not isinstance(meta.get("type"), str) or not meta["type"].strip():
                    errors.append(f"{rel}: Concept requires a non-empty type")
                if meta:
                    for field in ("generated",):
                        value = meta.get(field)
                        if value is not None and (not isinstance(value, dict) or not isinstance(value.get("by"), str) or not ACTOR.fullmatch(value["by"])):
                            errors.append(f"{rel}: Invalid {field}.by")
                    verified = meta.get("verified", [])
                    if isinstance(verified, dict):
                        verified = [verified]
                    if not isinstance(verified, list) or any(not isinstance(v, dict) or not isinstance(v.get("by"), str) or not ACTOR.fullmatch(v["by"]) for v in verified):
                        errors.append(f"{rel}: Invalid verified actor")
                    sources = meta.get("sources", [])
                    if not isinstance(sources, list) or any(not isinstance(s, dict) or not isinstance(s.get("resource"), str) or not s["resource"].strip() for s in sources):
                        errors.append(f"{rel}: Sources must include a resource")
                if not body.strip():
                    errors.append(f"{rel}: Empty concept")
            for match in RULE.finditer(outside_fences(body)):
                rule = match.group(1)
                if rule in seen:
                    errors.append(f"{rel}: Duplicate {rule} also in {seen[rule]}")
                seen[rule] = rel
        for match in LINK.finditer(outside_fences(body)):
            uri = match.group(1).strip().split(' "', 1)[0].strip("<>")
            if uri.startswith(("#", "mailto:", "data:")):
                continue
            parsed = urlsplit(uri)
            if parsed.scheme or parsed.netloc or not parsed.path:
                continue
            target = unquote(parsed.path)
            actual = ((root / target.lstrip("/")) if target.startswith("/") else (path.parent / target)).resolve()
            if not actual.is_relative_to(root):
                errors.append(f"{rel}: Link leaves repository: {uri}")
            elif not actual.exists():
                errors.append(f"{rel}: Broken local link: {uri}")
    if require_rules and not seen:
        errors.append("No REAS rule IDs found")
    return errors

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--bundle", default="docs", help="bundle directory relative to the root")
    parser.add_argument("--require-rules", action="store_true", help="fail when no REAS rule IDs are present")
    args = parser.parse_args()
    errors = validate(args.root, args.bundle, args.require_rules)
    for error in errors:
        print("ERROR:", error)
    if errors:
        print(f"FAIL: {len(errors)} errors")
        return 1
    print("PASS: OKF metadata, unique rule IDs, and internal Markdown links")
    print("NOT CHECKED: full STE, external URLs, technical accuracy")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
