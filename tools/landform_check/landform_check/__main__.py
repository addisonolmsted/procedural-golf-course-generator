"""CLI: python3 -m landform_check <cmd>

  render    build the 410-item fleet manifest and render via xtask (cached)
  extract   run the dtm_primitives instruments on rendered grids (cached)
  s1        S1 vectors for both populations, same estimator (cached)
  compare   energy/KS/variety vs split-half baseline -> closure_report.json
  gallery   per-archetype generated-vs-nearest-tile hillshade gallery
  all       render -> extract -> s1 -> compare
Common: --force re-runs a cached stage; --workers N for extract.
"""

from __future__ import annotations

import argparse
import sys


def main() -> int:
    ap = argparse.ArgumentParser(prog="landform_check")
    ap.add_argument("cmd", choices=["render", "extract", "s1", "compare",
                                    "gallery", "all"])
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--workers", type=int, default=0)
    args = ap.parse_args()
    from . import run as R
    if args.cmd == "render":
        return R.render(force=args.force)
    if args.cmd == "extract":
        return R.extract(workers=args.workers)
    if args.cmd == "s1":
        return R.s1(force=args.force)
    if args.cmd == "compare":
        return R.compare()
    if args.cmd == "gallery":
        from . import gallery
        return gallery.run()
    if args.cmd == "all":
        return R.all_steps(force=args.force)
    return 2


if __name__ == "__main__":
    sys.exit(main())
