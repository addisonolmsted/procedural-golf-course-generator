"""CLI: python3 -m landform_prior <cmd>

  fit      fit the prior from the extraction parquets ->
           out/landform_prior.json + golf-landform/data/landform_prior.json
  report   calibration report (w-A, S-A, lambda-W fits, per-archetype
           tables + effective-n audit) -> out/report/index.html
"""

from __future__ import annotations

import argparse
import sys


def main() -> int:
    ap = argparse.ArgumentParser(prog="landform_prior")
    ap.add_argument("cmd", nargs="?", default="fit",
                    choices=["fit", "report"])
    args = ap.parse_args()
    if args.cmd == "fit":
        from . import fit
        return fit.run()
    if args.cmd == "report":
        from . import report
        return report.run()
    return 2


if __name__ == "__main__":
    sys.exit(main())
