#!/usr/bin/env python3
"""Build fully localized es.js, fr.js, pt.js message catalogs."""
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MSG_DIR = ROOT / "apps/web/static/i18n/messages"


def load_js_obj(path: Path) -> dict:
    content = path.read_text(encoding="utf-8")
    match = re.match(r"export default\s+(\{.*\})\s*;\s*$", content, re.DOTALL)
    if not match:
        raise ValueError(f"Cannot parse {path}")
    return json.loads(match.group(1))


def flatten(d: dict, prefix: str = "") -> dict[str, str]:
    out: dict[str, str] = {}
    for key, value in d.items():
        path = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            out.update(flatten(value, path))
        else:
            out[path] = value
    return out


def unflatten(flat: dict[str, str]) -> dict:
    root: dict = {}
    for path, value in flat.items():
        parts = path.split(".")
        cur = root
        for part in parts[:-1]:
            cur = cur.setdefault(part, {})
        cur[parts[-1]] = value
    return root


def write_js(path: Path, data: dict) -> None:
    body = json.dumps(data, ensure_ascii=False, indent=2)
    path.write_text(f"export default {body};\n", encoding="utf-8")


def load_locale(name: str) -> dict[str, str]:
    path = Path(__file__).with_name(f"i18n_{name}.json")
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> None:
    en = load_js_obj(MSG_DIR / "en.js")
    en_flat = flatten(en)

    for locale in ("es", "fr", "pt"):
        mapping = load_locale(locale)
        missing = set(en_flat) - set(mapping)
        extra = set(mapping) - set(en_flat)
        if missing:
            raise SystemExit(f"{locale}: missing {len(missing)} keys: {sorted(missing)[:8]}")
        if extra:
            raise SystemExit(f"{locale}: extra {len(extra)} keys: {sorted(extra)[:8]}")
        write_js(MSG_DIR / f"{locale}.js", unflatten(mapping))
        same = sum(1 for key in en_flat if mapping[key] == en_flat[key])
        print(f"{locale}: keys={len(mapping)}, same_as_en={same}")


if __name__ == "__main__":
    main()
