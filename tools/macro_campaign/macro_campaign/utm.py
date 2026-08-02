"""WGS84 → UTM forward projection (standard transverse-Mercator series,
GRS80/WGS84 ellipsoid) — accurate to well under a metre, which is plenty for
placing 3 km tile windows; avoids a pyproj dependency."""

import math

_A = 6378137.0
_F = 1.0 / 298.257223563
_K0 = 0.9996
_E2 = _F * (2.0 - _F)
_EP2 = _E2 / (1.0 - _E2)


def to_utm(lat_deg: float, lon_deg: float, zone: int) -> tuple[float, float]:
    """Return (easting, northing) metres in the given UTM zone (northern
    hemisphere)."""
    lat = math.radians(lat_deg)
    lon = math.radians(lon_deg)
    lon0 = math.radians(zone * 6 - 183)

    n = _A / math.sqrt(1.0 - _E2 * math.sin(lat) ** 2)
    t = math.tan(lat) ** 2
    c = _EP2 * math.cos(lat) ** 2
    a = math.cos(lat) * (lon - lon0)

    m = _A * (
        (1 - _E2 / 4 - 3 * _E2**2 / 64 - 5 * _E2**3 / 256) * lat
        - (3 * _E2 / 8 + 3 * _E2**2 / 32 + 45 * _E2**3 / 1024) * math.sin(2 * lat)
        + (15 * _E2**2 / 256 + 45 * _E2**3 / 1024) * math.sin(4 * lat)
        - (35 * _E2**3 / 3072) * math.sin(6 * lat)
    )

    easting = _K0 * n * (
        a + (1 - t + c) * a**3 / 6 + (5 - 18 * t + t**2 + 72 * c - 58 * _EP2) * a**5 / 120
    ) + 500000.0
    northing = _K0 * (
        m
        + n
        * math.tan(lat)
        * (
            a**2 / 2
            + (5 - t + 9 * c + 4 * c**2) * a**4 / 24
            + (61 - 58 * t + t**2 + 600 * c - 330 * _EP2) * a**6 / 720
        )
    )
    return easting, northing
