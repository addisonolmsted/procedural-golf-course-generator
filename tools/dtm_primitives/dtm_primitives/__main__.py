"""CLI: python3 -m dtm_primitives <cmd>

Commands: framecheck | extract | synthcheck | qa | roundtrip | report
"""

from __future__ import annotations

import sys


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    cmd, rest = args[0], args[1:]
    if cmd == "framecheck":
        from . import framecheck
        return framecheck.run()
    if cmd in ("extract", "synthcheck", "qa", "roundtrip", "report"):
        print(f"{cmd}: not implemented yet (arrives in a later Stage-3 commit)")
        return 2
    print(f"unknown command: {cmd}\n{__doc__}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
