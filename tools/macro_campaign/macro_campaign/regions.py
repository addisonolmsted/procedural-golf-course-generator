"""Archetype exemplar regions: hand-picked 3 km tile centers (lat, lon)
inside well-known landform provinces, with each region's UTM zone EPSG.

**Centers must sit on protected or near-unpopulated land.** The pilot lists
were picked for landform character alone and every one of them landed in
farmed or suburbanized country: the OSM screen (`develop.py`) measured
8-64% of those tiles as roads, buildings and pits. That is not a cosmetic
problem — a graded road embankment is real topography, so ditches fit as
channels, cut-and-fill edges fit as scarps and borrow pits fit as basins,
and the prior ends up describing civil engineering instead of landscape.

So the selection rule is now: national forests, wildlife refuges, state
forest interiors and wilderness areas, biased away from towns and section
roads. Run `python3 -m macro_campaign develop` after fetching — it measures
each tile and auto-culls anything over MAX_DEVELOPED_FRAC into
out/exclude.json. Centers that fail are replaced, not kept and masked.

Scaling to ~30/archetype: add centers inside the same protected units, then
verify they cluster with their archetype using tools/tile_scout's GMM over
the s1_* metrics (sanity gate, not selector).
"""

TILE_M = 3000.0     # tile edge (matches the generated world box)
CELL_M = 2.0        # canonical working resolution

# archetype -> {"epsg": int, "utm_zone": int, "tiles": [(lat, lon), ...]}
REGIONS = {
    # Nebraska Sandhills — stabilized aeolian dune trains. Already among the
    # emptiest country in the US; centers biased toward Samuel R. McKelvie
    # National Forest and the Valentine NWR sandhills rather than ranch roads.
    "sandhills": {
        "epsg": 26914,
        "utm_zone": 14,
        "tiles": [
            (42.05, -101.40),   # pilot centers that passed the screen
            (41.95, -101.70),
            (42.20, -101.95),
            (41.80, -101.20),
            (42.35, -101.60),
            (41.65, -101.85),
            (42.72, -100.55),   # Samuel R. McKelvie NF
            (42.68, -100.42),
            (42.52, -100.68),   # Valentine NWR dune field
            (42.46, -100.82),
        ],
    },
    # Piedmont creek dissection — Uwharrie National Forest, the largest
    # protected block of dissected NC piedmont.
    "piedmont": {
        "epsg": 26917,
        "utm_zone": 17,
        "tiles": [
            (35.38, -79.99),    # Uwharrie NF core
            (35.32, -80.05),
            (35.44, -79.92),
            (35.28, -79.95),
            (35.50, -80.02),
            (35.35, -80.12),
            (35.20, -79.88),    # Birkhead Mountains Wilderness margin
            (35.25, -79.78),
        ],
    },
    # Florida lowland — FLAT wet country on protected land. The archetype's
    # identity is near-zero relief with surface water, so the exemplars must
    # be flatwoods, dry prairie and swamp, not karst upland: the Ocala NF
    # centers tried first measured 18-31 m of relief on a sandy ridge, which
    # dragged the fitted median to 18 m and broke the "flattest archetype"
    # anchor. Kept for the record, excluded from the list.
    "florida_lowland": {
        "epsg": 26917,
        "utm_zone": 17,
        "tiles": [
            (27.87, -81.12),    # Three Lakes WMA prairie/marsh (1-3 m relief)
            (27.80, -81.05),
            (27.75, -81.18),
            (27.62, -81.05),    # Kissimmee Prairie Preserve SP
            (27.55, -81.00),
            (27.68, -80.95),
            (28.42, -81.95),    # Green Swamp WMA
            (28.35, -82.02),
            (27.25, -82.30),    # Myakka River SP
            (27.32, -82.22),
        ],
    },
    # Kettled glacial topography — northern Wisconsin / Michigan pitted
    # outwash inside national forests. The Kettle Moraine pilot centers sit
    # in southeast-Wisconsin farm and exurb country (8-64% developed).
    "glacial_moraine": {
        "epsg": 26916,
        "utm_zone": 16,
        "tiles": [
            (46.15, -90.92),    # Chequamegon NF, Clam Lake pitted outwash
            (46.08, -90.85),
            (46.02, -90.98),
            (46.25, -90.78),
            (44.32, -85.82),    # Huron-Manistee NF kettles
            (44.25, -85.90),
            (44.12, -85.95),
            (44.05, -86.02),
            (46.22, -86.52),    # Hiawatha NF outwash + kettle lakes — the
            (46.15, -86.60),    # densest kettle character found on protected
            (46.30, -86.45),    # land (26 basins/tile), so weighted heavily
            (46.08, -86.48),
            (46.26, -86.58),
            (46.18, -86.44),
            (46.12, -86.36),
            # Tight cluster around the two genuinely pitted tiles found here
            # (t04622_08652 = 26 basins, t04626_08658 = 38): Hiawatha's
            # kettle character is patchy, so sample the pitted ground rather
            # than the outwash plain between the pits.
            (46.24, -86.55),
            (46.20, -86.56),
            (46.28, -86.54),
            (46.24, -86.60),
            (46.20, -86.49),
            (46.29, -86.60),
        ],
    },
    # Cumberland Plateau benches and scarps — Big South Fork NRRA and the
    # Obed gorge system. Avoids the surface mines that contaminated the
    # pilot's northern-plateau centers.
    "mountain_bench": {
        "epsg": 26916,
        "utm_zone": 16,
        "tiles": [
            (36.50, -84.70),    # Big South Fork NRRA
            (36.58, -84.62),
            (36.42, -84.78),
            (36.62, -84.80),
            (36.10, -84.72),    # Obed WSR / Catoosa WMA
            (36.15, -84.82),
            (36.05, -84.90),
            (36.35, -84.65),
        ],
    },
}


def tile_id(lat: float, lon: float) -> str:
    return f"t{round(lat * 100):05d}_{round(-lon * 100):05d}"
