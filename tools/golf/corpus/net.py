"""Network clients with etiquette: rotation, backoff, verbatim caching.

Every response is cached to disk keyed by a hash of the request, so re-runs
are offline and deterministic (parkland_atlas/collect.py resumability +
dtm_atlas/osm.py retry, combined).
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import time

import requests

from . import config


class FetchError(RuntimeError):
    pass


_last = {"overpass": 0.0, "nominatim": 0.0}


def _sleep(kind: str, gap: float) -> None:
    dt = time.time() - _last[kind]
    if dt < gap:
        time.sleep(gap - dt)
    _last[kind] = time.time()


def _cache_path(kind: str, key: str) -> pathlib.Path:
    h = hashlib.sha256(key.encode()).hexdigest()[:24]
    d = config.FETCH_CACHE / kind
    d.mkdir(parents=True, exist_ok=True)
    return d / f"{h}.json"


def overpass(query: str, tries: int = 4, timeout: int = 180) -> dict:
    cp = _cache_path("overpass", query)
    if cp.exists():
        return json.loads(cp.read_text())
    last = None
    for attempt in range(tries):
        ep = config.OVERPASS_ENDPOINTS[attempt % len(config.OVERPASS_ENDPOINTS)]
        _sleep("overpass", config.COURTESY_SLEEP_S)
        try:
            r = requests.post(ep, data={"data": query}, headers=config.UA,
                              timeout=timeout)
            if r.status_code == 200:
                out = r.json()
                # Overpass sometimes 200s with zero elements and no remark
                # under load (verified: the Long Island box cached 0 elements,
                # a fresh identical query returned 76). Never cache empties;
                # retry them, and return uncached if they persist.
                if out.get("elements") or out.get("remark"):
                    cp.write_text(json.dumps(out))
                    return out
                last = f"empty-200 @ {ep}"
            else:
                last = f"{r.status_code} @ {ep}"
        except (requests.RequestException, ValueError) as e:
            last = f"{e} @ {ep}"
        time.sleep(3.0 * (attempt + 1))
    if last and last.startswith("empty-200"):
        return {"elements": []}
    raise FetchError(f"overpass failed: {last}")


def nominatim(query: str, tries: int = 3) -> list[dict]:
    cp = _cache_path("nominatim", query)
    if cp.exists():
        return json.loads(cp.read_text())
    last = None
    for attempt in range(tries):
        _sleep("nominatim", config.NOMINATIM_SLEEP_S)
        try:
            r = requests.get(config.NOMINATIM,
                             params={"q": query, "format": "json", "limit": 3},
                             headers=config.UA, timeout=60)
            if r.status_code == 200:
                out = r.json()
                cp.write_text(json.dumps(out))
                return out
            last = f"{r.status_code}"
        except (requests.RequestException, ValueError) as e:
            last = f"{e}"
        time.sleep(1.5 * (attempt + 1))
    raise FetchError(f"nominatim failed for {query!r}: {last}")
