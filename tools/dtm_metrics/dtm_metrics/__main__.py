"""CLI: python3 -m dtm_metrics <cmd>

  extract   [--course k1,k2] [--limit N] [--workers K] [--out PATH]
  robustness / stability / gates / explorer      (Stage-1 harnesses)
  campaign  --which a|b|c    emulate    identify  coverage  decide  (sandbox)
"""

from __future__ import annotations

import argparse
import os
import sys


def main() -> int:
    ap = argparse.ArgumentParser(prog="dtm_metrics")
    ap.add_argument("cmd", choices=[
        "extract", "robustness", "stability", "gates", "explorer",
        "campaign", "emulate", "identify", "coverage", "decide",
    ])
    ap.add_argument("--course", default="")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--workers", type=int, default=max(1, (os.cpu_count() or 2) - 1))
    ap.add_argument("--out", default="")
    ap.add_argument("--which", default="a", help="campaign: a|b|c")
    args = ap.parse_args()

    from . import store
    keys = None
    if args.course:
        keys = [k.strip() for k in args.course.split(",") if k.strip()]
    elif args.limit:
        keys = store.course_keys()[: args.limit]

    if args.cmd == "extract":
        from . import extract
        out = args.out or os.path.join(store.OUT, "metrics.parquet")
        extract.run(keys, args.workers, out)
        return 0
    if args.cmd == "robustness":
        from . import robustness
        return robustness.run(args.workers)
    if args.cmd == "stability":
        from . import stability
        return stability.run(args.workers)
    if args.cmd == "gates":
        from . import gates
        return gates.run()
    if args.cmd == "explorer":
        from . import explorer
        return explorer.run()
    if args.cmd == "campaign":
        from .sandbox import campaign
        return campaign.run(args.which, args.workers)
    if args.cmd == "emulate":
        from .sandbox import emulate
        return emulate.run()
    if args.cmd == "identify":
        from .sandbox import identify
        return identify.run()
    if args.cmd == "coverage":
        from .sandbox import coverage
        return coverage.run()
    if args.cmd == "decide":
        from .sandbox import decide
        return decide.run()
    return 1


if __name__ == "__main__":
    sys.exit(main())
