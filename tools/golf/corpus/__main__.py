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
    elif cmd == "index":
        print(registry.write_index())
    else:
        print(__doc__)


if __name__ == "__main__":
    main()
