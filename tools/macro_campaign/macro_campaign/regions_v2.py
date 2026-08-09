"""Heartland (v2) exemplar regions — the Phase-E corpus campaign.

Same shape and rules as `regions.py` (which is the v1 archetype set, kept
untouched): **centers on protected or near-unpopulated land only** — national
forests, grasslands, wildlife refuges, protected river corridors. National
forests are checkerboards with private inholdings; the OSM develop screen
(`develop.py`) is the backstop that culls what slips through, and centers
that fail are replaced, not masked.

v2 notes:
- `piedmont` and `sandhills` reuse the v1 keys and tile directories — their
  existing clean tiles count toward the ~50/biome target and the fetch's
  resume logic dedupes for free.
- `heathland` fetches the **US kettled-outwash analog** (Hiawatha's pitted
  ground — the densest kettle character the v1 campaign found on protected
  land — plus Seney NWR and the Huron-Manistee kettles). True UK/NL heath
  arrives via the EA LIDAR / AHN path (workplan E3); cross-source
  comparability is checked before the two are ever mixed.
- Plains lidar caveat: 3DEP mosaics best-available sources, and parts of the
  high plains are 10 m-source; a 10 m tile bilinear-resampled to 2 m reads as
  suspiciously smooth. The QA cull watches for it.

Target ~50 centers per biome so ~30 survive the screen (v1 attrition ~40%).
"""

from .regions import TILE_M, CELL_M, tile_id  # noqa: F401  (shared conventions)


def _grid(lat0: float, lon0: float, nlat: int, nlon: int) -> list[tuple[float, float]]:
    """A centered nlat x nlon grid of tile centers spaced one tile (3 km)
    apart. Latitude step is fixed; longitude step compensates for latitude."""
    import math

    dlat = TILE_M / 111_320.0
    dlon = TILE_M / (111_320.0 * math.cos(math.radians(lat0)))
    out = []
    for i in range(nlat):
        for j in range(nlon):
            out.append((
                round(lat0 + (i - (nlat - 1) / 2) * dlat, 4),
                round(lon0 + (j - (nlon - 1) / 2) * dlon, 4),
            ))
    return out


REGIONS_V2 = {
    # ------------------------------------------------------------- piedmont
    # Rolling fluvial dissection. Uwharrie NF (the v1 unit, expanded) plus
    # Sumter NF's Enoree district — the largest protected SC piedmont block.
    "piedmont": {
        "epsg": 26917,
        "utm_zone": 17,
        "tiles": _grid(35.385, -80.015, 4, 4)      # Uwharrie NF core
        + _grid(34.65, -81.68, 4, 4)               # Sumter NF, Enoree district
        # Top-up (2026-08, 19/46 clean at first pass):
        + _grid(33.35, -83.35, 4, 4)               # Oconee NF (GA) granite piedmont
        + _grid(33.95, -82.30, 3, 4),              # Sumter NF, Long Cane district
    },
    # ------------------------------------------------------------ sandhills
    # Nebraska Sandhills dune trains, all refuge/forest ground.
    "sandhills": {
        "epsg": 26914,
        "utm_zone": 14,
        "tiles": _grid(42.63, -100.85, 4, 5)       # Samuel R. McKelvie NF
        + _grid(42.48, -100.65, 3, 5)              # Valentine NWR dune field
        + _grid(41.87, -100.33, 3, 3),             # Nebraska NF Bessey unit
                                                   # (Crescent Lake NWR was
                                                   # dropped: it sits west of
                                                   # 102W, in zone 13)
    },
    # --------------------------------------------------------- great_plains
    # Caprock-held flat: national grasslands on the high plains (zone 13).
    "great_plains": {
        "epsg": 26913,
        "utm_zone": 13,
        "tiles": _grid(40.80, -103.80, 3, 5)       # Pawnee NG, east unit
        + _grid(37.12, -103.05, 3, 5)              # Comanche NG, Carrizo unit
        + _grid(36.42, -103.97, 3, 4)              # Kiowa NG
        + _grid(36.45, -102.70, 3, 4)              # Rita Blanca NG
        # Top-up (2026-08, 28/54 clean — inholdings farmed):
        + _grid(42.93, -103.45, 2, 4),             # Oglala NG tableland (NE)
    },
    # --------------------------------------------------------- river_valley
    # Mature meandering rivers with preserved floodplain + terraces
    # (zone 15). The corridor is narrow, so centers follow the valley by
    # hand rather than by grid.
    #
    # Top-up (2026-08): the first pass survived worst here — 6/20 clean, the
    # rest farmed or graded (valley bottoms are exactly where people build).
    # The top-up leans on large bottomland-hardwood refuges where the whole
    # block is protected, not just the channel: Tensas, Upper Ouachita and
    # Felsenthal (LA/AR), the Cache River corridor, Big Muddy units on the
    # lower Missouri, and a southward extension of White River NWR. Sized to
    # the measured ~30% survival.
    "river_valley": {
        "epsg": 26915,
        "utm_zone": 15,
        "tiles": [
            # Lower Wisconsin State Riverway, following the valley west.
            (42.96, -90.05), (43.00, -90.15), (43.05, -90.25), (43.08, -90.35),
            (43.06, -90.45), (43.10, -90.55), (43.12, -90.65), (43.10, -90.75),
            (43.08, -90.85), (43.05, -90.95),
            # Dale Bumpers White River NWR (AR) — floodplain + terrace flight.
            (34.55, -91.08), (34.48, -91.10), (34.42, -91.06), (34.35, -91.10),
            (34.28, -91.12), (34.21, -91.08), (34.14, -91.10), (34.08, -91.14),
            (34.01, -91.10), (33.95, -91.12),
        ]
        + _grid(32.33, -91.35, 5, 4)               # Tensas River NWR (LA)
        + _grid(32.90, -92.15, 3, 4)               # Upper Ouachita NWR (LA)
        + _grid(33.07, -92.05, 3, 4)               # Felsenthal NWR (AR)
        + [
            # Cache River NWR corridor (AR), north to south.
            (35.30, -91.25), (35.24, -91.22), (35.18, -91.20), (35.12, -91.24),
            (35.06, -91.22), (35.00, -91.25), (34.94, -91.28), (34.88, -91.30),
            # Big Muddy NFWR units, lower Missouri River (MO).
            (39.08, -92.95), (39.03, -92.85), (38.98, -92.75), (38.92, -92.60),
            (38.86, -92.48), (38.80, -92.40), (38.72, -92.30), (38.65, -92.20),
            # White River NWR, southern extension.
            (33.88, -91.14), (33.82, -91.10), (33.75, -91.12), (33.68, -91.08),
            (33.62, -91.10), (33.55, -91.12),
        ],
    },
    # --------------------------------------------------------- hill_country
    # Dissected plateau, stepped slopes (zone 15): Ozark uplands.
    "hill_country": {
        "epsg": 26915,
        "utm_zone": 15,
        "tiles": _grid(36.95, -91.15, 4, 5)        # Mark Twain NF, Eleven Point / Current R.
        + _grid(36.02, -92.60, 3, 5)               # Buffalo National River / Ozark NF Sylamore
        # Top-up (2026-08, 21/35 clean at first pass):
        + _grid(37.55, -91.15, 3, 5),              # Mark Twain NF, Salem/Potosi uplands
    },
    # ------------------------------------------------------------ heathland
    # US kettled-outwash analog (zone 16). Hiawatha's pitted ground is the
    # densest kettle character the v1 campaign found on protected land
    # (26-38 basins/tile); Seney NWR is outwash-and-bog; Huron-Manistee
    # carries the southern kettles.
    "heathland": {
        "epsg": 26916,
        "utm_zone": 16,
        "tiles": _grid(46.23, -86.53, 4, 4)        # Hiawatha NF pitted outwash
        + _grid(46.25, -85.95, 3, 4)               # Seney NWR
        + _grid(44.20, -85.92, 4, 4)               # Huron-Manistee NF kettles
        # Top-up (2026-08, 16/44 clean — heavy graded/ag attrition):
        + _grid(45.75, -89.05, 4, 4)               # Chequamegon-Nicolet NF, Eagle River outwash
        + _grid(46.30, -89.25, 3, 4)               # Ottawa NF interior (Sylvania fringe)
        + _grid(44.05, -85.80, 3, 4),              # Manistee NF, second kettle block
    },
}


def summary() -> str:
    lines = []
    total = 0
    for k, r in REGIONS_V2.items():
        n = len(r["tiles"])
        total += n
        lines.append(f"  {k:14s} zone {r['utm_zone']:2d}  {n:3d} centers")
    lines.append(f"  {'TOTAL':14s}          {total:3d} centers")
    return "\n".join(lines)
