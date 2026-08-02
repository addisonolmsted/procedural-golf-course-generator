"""dtm_atlas — Stage 0 of the terrain-v2 pipeline: lidar DTM atlas store.

Fetches USGS 3DEP 1 m DTMs + OSM feature masks for the US subset of the
parkland-atlas courses, builds a clean per-course store (raw + naturalized
rasters + mask stack + meta), and emits the QA gallery that is the Stage 0
assessment gate. See tools/dtm_atlas/README.md and docs/terrain-v2-plan.md.
"""

DTM_ATLAS_VERSION = "0.2.0"
