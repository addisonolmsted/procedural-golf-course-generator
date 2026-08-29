"""CLI: python3 -m corpus <discover|assign|index> (run from tools/golf)."""
import sys

from . import discover, assign_greens, registry


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "help"
    if cmd == "discover":
        got, miss = discover.run_curated()
        print(f"curated: {got} resolved, {miss} missed")
        n = discover.run_boxes()
        print(f"boxes: {n} new polygons")
        print(f"index: {registry.write_index()} records")
    elif cmd == "assign":
        out = assign_greens.run()
        print(out)
    elif cmd == "fetch":
        from . import fetch_dem
        print(fetch_dem.run())
    elif cmd == "water":
        from . import water
        print(water.run())
    elif cmd == "fame":
        from . import fame
        fame.seed()
        print(fame.apply())
    elif cmd == "extract":
        from . import extract
        print(extract.run(procs=int(sys.argv[2]) if len(sys.argv) > 2 else 4))
    elif cmd == "fit":
        from . import fit
        r = fit.run()
        print({k: v for k, v in r.items() if k.startswith("auc") or k.startswith("n_")})
    elif cmd == "coverage":
        from . import windows
        r = windows.coverage()
        print({k: r[k] for k in ("n", "median", "frac_ge70")})
    elif cmd == "dispersion":
        from . import dispersion
        print(dispersion.run()["agg"])
    elif cmd == "report1":
        from . import report
        print(report.phase1())
    elif cmd == "index":
        print(registry.write_index())
    else:
        print(__doc__)


if __name__ == "__main__":
    main()
