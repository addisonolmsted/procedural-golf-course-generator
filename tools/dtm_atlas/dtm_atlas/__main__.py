"""CLI dispatcher: python3 -m dtm_atlas <cmd> [options]

Subcommands (only `fetch` touches the network):
  courses [--write]      derive/check the committed US course list
  fetch                  3DEP DEM + OSM boundary/features -> out/fetch_cache/
  raster                 fetch cache -> out/store/{key}/raw.tif (+ partial meta)
  masks                  OSM classes -> out/store/{key}/masks/*.tif
  artifacts              curvature/seam/bridge detectors -> masks + inpaint.tif
  naturalize             Laplace fill -> naturalized.tif + final meta.json
  qa                     hillshade PNGs + per-course pages + index + coverage CSV
  verify [--write]       hash the store into MANIFEST.sha256 / check it
  all [--offline]        fetch (unless --offline) + every derive stage + qa

Common options: --course k1,k2  --limit N  --force
"""

from __future__ import annotations

import argparse
import sys


def main() -> int:
    ap = argparse.ArgumentParser(prog="dtm_atlas")
    ap.add_argument("cmd", choices=[
        "courses", "fetch", "raster", "masks", "artifacts", "naturalize",
        "qa", "verify", "all",
    ])
    ap.add_argument("--write", action="store_true",
                    help="courses: write the list; verify: write the manifest")
    ap.add_argument("--course", default="",
                    help="comma-separated course keys (default: all listed)")
    ap.add_argument("--limit", type=int, default=0, help="first N courses only")
    ap.add_argument("--force", action="store_true",
                    help="recompute even when outputs exist")
    ap.add_argument("--offline", action="store_true",
                    help="all: skip fetch; error per-course if cache missing")
    args = ap.parse_args()

    if args.cmd == "courses":
        from . import uscourses
        return uscourses.run(args.write)

    from . import pipeline
    keys = pipeline.select_courses(args.course, args.limit)
    if args.cmd == "verify":
        from . import meta
        return meta.verify(write=args.write)
    return pipeline.run(args.cmd, keys, force=args.force, offline=args.offline)


if __name__ == "__main__":
    sys.exit(main())
