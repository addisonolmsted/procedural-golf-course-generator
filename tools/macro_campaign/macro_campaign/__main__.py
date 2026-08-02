import argparse


def main():
    ap = argparse.ArgumentParser(prog="macro_campaign")
    sub = ap.add_subparsers(dest="cmd", required=True)

    f = sub.add_parser("fetch", help="fetch exemplar tiles from 3DEP as CGRID1")
    f.add_argument("--limit", type=int, default=None, help="max tiles this run")
    f.add_argument("--archetype", default=None)

    e = sub.add_parser("extract", help="measure landform knobs per tile")
    e.add_argument("--archetype", default=None)
    e.add_argument(
        "--force", action="store_true", help="re-extract even if current-version output exists"
    )

    s = sub.add_parser("shape", help="deep transect fits (meander knobs); minutes per tile")
    s.add_argument("--archetype", default=None)
    s.add_argument("--force", action="store_true")

    d = sub.add_parser("develop", help="OSM development screen per tile (auto-culls built-up tiles)")
    d.add_argument("--archetype", default=None)
    d.add_argument("--force", action="store_true")

    cr = sub.add_parser("courses", help="measure real golf-course core relief (OSM + 3DEP)")
    cr.add_argument("--archetype", default=None)
    cr.add_argument("--limit", type=int, default=12)

    ch = sub.add_parser("cull", help="archetype-membership cull (tiles lacking the defining feature)")
    ch.add_argument("--dry-run", action="store_true")

    c = sub.add_parser("compare", help="measure generated tiles, report vs real")
    c.add_argument("--force", action="store_true", help="re-measure generated tiles")
    c.add_argument("--gate", action="store_true", help="exit non-zero if a structural gate fails")

    rp = sub.add_parser("report", help="self-contained HTML page of the generated-vs-real gap")
    rp.add_argument("--force", action="store_true", help="re-render the terrain images")
    rp.add_argument("--seeds", type=int, default=None, help="cap generated seeds per archetype")

    rf = sub.add_parser("reframe", help="before/after page for the macro reframe")
    rf.add_argument("--force", action="store_true", help="re-render the tiles")

    t = sub.add_parser("fit", help="fit quantile tables, merge into the prior")
    t.add_argument("--apply", action="store_true", help="install into course-spec")
    t.add_argument("--version", default="campaign-pilot-1")

    args = ap.parse_args()
    if args.cmd == "fetch":
        from . import fetch

        fetch.run(limit=args.limit, archetype=args.archetype)
    elif args.cmd == "extract":
        from . import extract

        extract.run(archetype=args.archetype, force=args.force)
    elif args.cmd == "shape":
        from . import shape

        shape.run(archetype=args.archetype, force=args.force)
    elif args.cmd == "develop":
        from . import develop

        develop.run(archetype=args.archetype, force=args.force)
    elif args.cmd == "courses":
        from . import courses

        courses.run(archetype=args.archetype, limit=args.limit)
    elif args.cmd == "cull":
        from . import character

        character.run(dry_run=args.dry_run)
    elif args.cmd == "compare":
        from . import compare

        raise SystemExit(compare.run(force=args.force, gate=args.gate))
    elif args.cmd == "report":
        from . import report

        raise SystemExit(report.run(force=args.force, seeds=args.seeds))
    elif args.cmd == "reframe":
        from . import reframe

        raise SystemExit(reframe.run(force=args.force))
    elif args.cmd == "fit":
        from . import fit_knobs

        fit_knobs.run(apply=args.apply, version=args.version)


if __name__ == "__main__":
    main()
