# F2 dictionary build report

asset: `assets/dictionary_v2.bin` — 45.2 MB, fingerprint `sha256:1e6d32de3…`

| biome | level | candidates | clean-rej | rect-rej | dup-rej | kept | buckets filled | <5-tile buckets (borrowed) |
|---|---|---|---|---|---|---|---|---|
| piedmont | mid | 12167 | 1278 | 900 | 2 | 1658 | 108 | 0 (166 borrowed) |
| piedmont | fine | 198927 | 23588 | 9461 | 0 | 1728 | 108 | 0 (0 borrowed) |
| sandhills | mid | 20102 | 13924 | 153 | 0 | 1640 | 108 | 0 (135 borrowed) |
| sandhills | fine | 328662 | 162754 | 7893 | 1 | 1728 | 108 | 0 (0 borrowed) |
| great_plains | mid | 16399 | 11626 | 737 | 0 | 1541 | 103 | 0 (268 borrowed) |
| great_plains | fine | 268119 | 160090 | 10397 | 0 | 1716 | 108 | 0 (8 borrowed) |
| river_valley | mid | 14283 | 12431 | 339 | 0 | 1418 | 92 | 0 (776 borrowed) |
| river_valley | fine | 233523 | 177062 | 4505 | 1 | 1715 | 108 | 0 (8 borrowed) |
| hill_country | mid | 12167 | 506 | 2845 | 0 | 1601 | 104 | 0 (189 borrowed) |
| hill_country | fine | 198927 | 14751 | 24022 | 6 | 1716 | 108 | 0 (0 borrowed) |
| heathland | mid | 16928 | 11952 | 631 | 0 | 1600 | 108 | 0 (146 borrowed) |
| heathland | fine | 276768 | 159298 | 3523 | 0 | 1728 | 108 | 0 (0 borrowed) |

held-out tiles (never harvested — F3's QA set):

- piedmont: t03336_08333, t03464_08163, t03537_08000, t03543_08006
- sandhills: t04184_10037, t04220_10195, t04246_10082, t04251_10061, t04259_10085, t04264_10081
- great_plains: t03642_10265, t03709_10298, t03712_10305, t04077_10376, t04292_10351
- river_valley: t03230_09137, t03238_09137, t03304_09207, t03375_09112, t04310_09055
- hill_country: t03599_09267, t03691_09108, t03699_09112, t03755_09122
- heathland: t04416_08586, t04571_08907, t04579_08899, t04624_08655, t04630_08927
