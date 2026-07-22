#!/usr/bin/env python3
"""Generate complete ja.js and ko.js i18n catalogs from en.js template + embedded translations."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

DIR = Path(__file__).resolve().parent

# ── Embedded translations (dotted keys → localized string) ───────────────────
# Natural JA/KO UI copy; placeholders preserved; product terms kept per project rules.

JA: dict[str, str] = {}
KO: dict[str, str] = {}


def _load_translations() -> None:
    """Populate JA and KO from the translations module."""
    from _ja_ko_translation_data import JA as JA_DATA, KO as KO_DATA  # noqa: WPS433

    JA.update(JA_DATA)
    KO.update(KO_DATA)


def load_js_object(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    text = re.sub(r"^export default\s*", "", text.strip())
    text = re.sub(r";\s*$", "", text)
    return json.loads(text)


def flatten(d: dict, prefix: str = "") -> dict[str, str]:
    out: dict[str, str] = {}
    for k, v in d.items():
        key = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            out.update(flatten(v, key))
        else:
            out[key] = v
    return out


def unflatten(flat: dict[str, str]) -> dict:
    root: dict = {}
    for key, value in sorted(flat.items()):
        parts = key.split(".")
        node = root
        for part in parts[:-1]:
            node = node.setdefault(part, {})
        node[parts[-1]] = value
    return root


def to_js_module(obj: dict) -> str:
    return f"export default {json.dumps(obj, ensure_ascii=False, indent=2)};\n"


def validate(
    en_flat: dict[str, str],
    zh_flat: dict[str, str],
    loc_flat: dict[str, str],
    loc: str,
) -> tuple[int, list[str]]:
    """Return (identical_to_en_count, list of unexpected identical keys)."""
    en_keys = set(en_flat)
    loc_keys = set(loc_flat)
    if en_keys != loc_keys:
        missing = sorted(en_keys - loc_keys)
        extra = sorted(loc_keys - en_keys)
        raise SystemExit(
            f"{loc}: key mismatch — missing {len(missing)} extra {len(extra)}\n"
            f"  missing sample: {missing[:5]}\n  extra sample: {extra[:5]}"
        )

    identical: list[str] = []
    for key in sorted(en_keys):
        en_val = en_flat[key]
        loc_val = loc_flat[key]
        zh_val = zh_flat.get(key, "")
        if loc_val == en_val and loc_val != zh_val:
            identical.append(key)
    return len(identical), identical


def main() -> int:
    _load_translations()

    en = load_js_object(DIR / "en.js")
    zh = load_js_object(DIR / "zh.js")
    en_flat = flatten(en)
    zh_flat = flatten(zh)

    expected = len(en_flat)
    if len(JA) != expected or len(KO) != expected:
        print(
            f"Translation count mismatch: en={expected} ja={len(JA)} ko={len(KO)}",
            file=sys.stderr,
        )
        if len(JA) != expected:
            missing_ja = sorted(set(en_flat) - set(JA))
            extra_ja = sorted(set(JA) - set(en_flat))
            if missing_ja:
                print(f"  JA missing ({len(missing_ja)}): {missing_ja[:8]}", file=sys.stderr)
            if extra_ja:
                print(f"  JA extra ({len(extra_ja)}): {extra_ja[:8]}", file=sys.stderr)
        if len(KO) != expected:
            missing_ko = sorted(set(en_flat) - set(KO))
            extra_ko = sorted(set(KO) - set(en_flat))
            if missing_ko:
                print(f"  KO missing ({len(missing_ko)}): {missing_ko[:8]}", file=sys.stderr)
            if extra_ko:
                print(f"  KO extra ({len(extra_ko)}): {extra_ko[:8]}", file=sys.stderr)
        return 1

    results = {}
    for loc, trans in (("ja", JA), ("ko", KO)):
        loc_flat = {k: trans[k] for k in en_flat}
        identical_count, identical_keys = validate(en_flat, zh_flat, loc_flat, loc)
        nested = unflatten(loc_flat)
        out_path = DIR / f"{loc}.js"
        out_path.write_text(to_js_module(nested), encoding="utf-8")
        results[loc] = {
            "keys": len(loc_flat),
            "identical_to_en": identical_count,
            "identical_keys": identical_keys,
        }

    print(f"Generated ja.js and ko.js — {expected} keys each")
    for loc, r in results.items():
        print(f"\n{loc.upper()}:")
        print(f"  key count: {r['keys']}")
        print(f"  identical to en (should be low): {r['identical_to_en']}")
        if r["identical_keys"]:
            print(f"  identical keys: {', '.join(r['identical_keys'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
