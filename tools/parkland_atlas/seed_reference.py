"""Seed the collector cache with the reference 27 courses.

They are already collected in the exact schema (including ``tb64``), so we copy
them straight into ``out/cache/<key>.json`` — keeping them byte-identical and
saving 27 sets of network fetches. Idempotent.

    python3 seed_reference.py [path/to/parkland_atlas.html]
"""

import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CACHE = os.path.join(HERE, "out", "cache")
DEFAULT_HTML = os.path.expanduser("~/Downloads/parkland_atlas.html")

# Preserve the reference display order / architect strings by re-reading them.
REQUIRED = {"label", "bbox", "b64", "tb64", "wb64", "treepct", "waterpct",
            "wm", "hm", "emin", "emax", "holes"}


def extract_courses(html):
    marker = "window.COURSES="
    i = html.index(marker) + len(marker)
    # brace-match to the closing } (strings may contain braces — track quotes)
    depth = 0
    j = i
    instr = False
    esc = False
    while j < len(html):
        c = html[j]
        if esc:
            esc = False
        elif c == "\\":
            esc = True
        elif c == '"':
            instr = not instr
        elif not instr:
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    j += 1
                    break
        j += 1
    return json.loads(html[i:j])


def main():
    html_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_HTML
    with open(html_path, encoding="utf-8") as f:
        html = f.read()
    courses = extract_courses(html)
    os.makedirs(CACHE, exist_ok=True)
    n = 0
    for key, c in courses.items():
        missing = REQUIRED - set(c)
        if missing:
            print(f"  skip {key}: missing {missing}")
            continue
        c = dict(c)
        c.setdefault("arch", "")
        c["key"] = key
        c["source"] = "reference"
        with open(os.path.join(CACHE, f"{key}.json"), "w") as out:
            json.dump(c, out)
        n += 1
    print(f"seeded {n} reference courses into {CACHE}")


if __name__ == "__main__":
    main()
