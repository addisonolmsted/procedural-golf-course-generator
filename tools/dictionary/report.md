# F2 dictionary build report

asset: `assets/dictionary_v2.bin` — 44.4 MB, fingerprint `sha256:afcb21ee4…`

| biome | level | candidates | clean-rej | rect-rej | dup-rej | kept | buckets filled | <5-tile buckets (borrowed) |
|---|---|---|---|---|---|---|---|---|
| piedmont | mid | 12167 | 1278 | 3620 | 1 | 1623 | 105 | 0 (197 borrowed) |
| piedmont | fine | 198927 | 23588 | 34806 | 0 | 1728 | 108 | 0 (0 borrowed) |
| sandhills | mid | 20102 | 13924 | 1026 | 0 | 1633 | 108 | 0 (163 borrowed) |
| sandhills | fine | 328662 | 162754 | 40191 | 0 | 1728 | 108 | 0 (0 borrowed) |
| great_plains | mid | 16399 | 11626 | 2108 | 0 | 1480 | 102 | 0 (342 borrowed) |
| great_plains | fine | 268119 | 160090 | 31932 | 0 | 1717 | 108 | 0 (20 borrowed) |
| river_valley | mid | 14283 | 12431 | 823 | 0 | 1292 | 84 | 0 (798 borrowed) |
| river_valley | fine | 233523 | 177062 | 9861 | 0 | 1714 | 108 | 0 (13 borrowed) |
| hill_country | mid | 12167 | 506 | 6424 | 0 | 1540 | 100 | 0 (224 borrowed) |
| hill_country | fine | 198927 | 14751 | 62507 | 2 | 1718 | 108 | 0 (6 borrowed) |
| heathland | mid | 16928 | 11952 | 1578 | 0 | 1536 | 107 | 0 (170 borrowed) |
| heathland | fine | 276768 | 159298 | 12440 | 0 | 1728 | 108 | 0 (0 borrowed) |

held-out tiles (never harvested — F3's QA set):

- piedmont: t03336_08333, t03464_08163, t03537_08000, t03543_08006
- sandhills: t04184_10037, t04220_10195, t04246_10082, t04251_10061, t04259_10085, t04264_10081
- great_plains: t03642_10265, t03709_10298, t03712_10305, t04077_10376, t04292_10351
- river_valley: t03230_09137, t03238_09137, t03304_09207, t03375_09112, t04310_09055
- hill_country: t03599_09267, t03691_09108, t03699_09112, t03755_09122
- heathland: t04416_08586, t04571_08907, t04579_08899, t04624_08655, t04630_08927
