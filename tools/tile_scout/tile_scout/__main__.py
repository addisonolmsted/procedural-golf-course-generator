"""CLI: python3 -m tile_scout <cmd>

Stage-0T campaign, in funnel order (only f0/f1/f2 touch the network):
  archetypes             course S1 vectors -> soft archetypes (out/archetypes.json)
  regions                course centers -> disc-cover regions (out/regions.json)
  f0 [--region r_x]      regional 60 m DEM screen -> out/f0_candidates.parquet
  f1 [--region r_x]      batched OSM development screen -> out/f1_survivors.parquet
  f2 [--region r_x]      10 m full-S1 match -> out/f2_scores.parquet
  finalize               write tools/dtm_atlas/tiles_us.{txt,json} (finalists)
  select                 final ~200 by facility location (needs tile metrics)
  report                 coverage table + energy/KS + side-by-side gallery
Common: --force re-runs a cached stage.
"""

from __future__ import annotations

import argparse
import sys


def main() -> int:
    ap = argparse.ArgumentParser(prog="tile_scout")
    ap.add_argument("cmd", choices=[
        "archetypes", "regions", "f0", "f1", "f2", "finalize", "select",
        "report",
    ])
    ap.add_argument("--region", default="",
                    help="restrict a funnel stage to one region (pilot)")
    ap.add_argument("--force", action="store_true")
    args = ap.parse_args()

    from . import sconfig
    cfg = sconfig.load_config()

    if args.cmd == "archetypes":
        from . import archetypes
        return archetypes.run(cfg)
    if args.cmd == "regions":
        from . import regions
        return regions.run(cfg)
    if args.cmd in ("f0", "f1", "f2", "finalize", "select", "report"):
        print(f"{args.cmd}: not implemented yet (arrives in a later "
              "Stage-0T commit)")
        return 2
    return 2


if __name__ == "__main__":
    sys.exit(main())
