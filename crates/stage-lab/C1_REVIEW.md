# C1 review guide — what success looks like *before* structure exists

C1 is S1's output: the **predisposition fields** (tilt, relief, hardness,
accommodation) plus structural metadata. It is deliberately incomplete
terrain. S2 has not carved drainage; S3 has not applied texture. Judging C1
by "does this look like real terrain" is a category error that will produce
false failures — this guide says exactly what to judge and what to refuse to
judge.

Band ownership (docs/00-architecture.md): S1 owns **≥ 400 m** only. Every
wavelength below that belongs to a later stage.

## Where to look

Stage-lab tabs **`0 Spec (v2)`**, **`1 C1 (S1)`**, **`C1 gallery`**
(`cargo run -p stage-lab --release`), and the pre-rendered legibility set in
`out/c1_legibility/` (`cargo run -p stage-lab --release --example
c1_legibility`): `labeled/` = 2 study sheets per class with overlays ON;
`blind/` = 18 shuffled images, overlays OFF, answers in `answer_key.json`.

The default **Implied** view sums tilt + relief and hillshades it — the
closest thing to "terrain" C1 can offer. The other views isolate one field.

## MUST PASS — the six success criteria

1. **Class legibility (the gate, ≥ 80 % blind).** Each window class must be
   nameable from macro form alone:
   - *valley_floor* — an axial low band with both flanks rising away from it
   - *interfluve* — a broad axial high, flanks falling to either side
   - *escarpment_face* — one steep contour-dense band splitting the box into
     an upper and a lower bench
   - *basin_margin* — one side ramps down into a wide closed-ish flat
   - *piedmont_slope* — a single sustained ramp, highest at the edge
     opposite base level
   - *terrace_flight* — the piedmont ramp broken into 2–3 discernible treads
   The blind protocol: classify all 18 in `blind/`, check against
   `answer_key.json`, gate is ≥ 80 % (15/18). Record confusions — a
   *specific* confusion pair (e.g. piedmont_slope ↔ terrace_flight) is a
   shape-authoring bug with a targeted fix; diffuse confusion means the
   class:mode amplitude ratio regressed.
2. **Base-level obedience.** The tilt field must fall toward the drawn
   base-level edge (blue band). No class shape may reverse it — a valley
   floor still drains somewhere.
3. **Grain readability.** In Relief view, the elongation of highs/lows must
   visibly align with the grain glyphs (white dashes). Isotropic blobs =
   grain strength too weak for the biome.
4. **Discontinuity as a character line.** On two-province seeds the colored
   polyline (red scarp / blue valley-wall / orange contact) must coincide
   with a visible change in the surface — a step, a bench edge, or a
   hardness change — spanning the full box. A line floating over unchanged
   terrain is a failure.
5. **Field coherence.** Hardness and accommodation must co-vary sensibly
   with the form: accommodation high where the class puts storage (valley
   axis, basin flat, treads), hardness banding aligned with strike where
   strata exist. Fields that look mutually independent are wrong.
6. **Determinism & spread.** Same seed twice ⇒ identical image (goldens
   enforce this). Across the 14-seed spec table, biomes/classes/provinces
   should visibly vary — no monoculture.

## MUST NOT JUDGE — expected "failures" that are correct

- **No drainage.** No valleys-within-valleys, no dendritic anything, no
  channels. That is S2's entire job. Their absence here is *required*.
- **No texture.** Surfaces are smooth, band-limited, "candle-wax". The
  spike proved texture (S3 quilting from real residuals) carries realism —
  C1 carrying it would violate band ownership.
- **Not realism.** Do not compare C1 against real hillshades or lidar. The
  only realism question C1 answers is "is the macro *predisposition*
  plausible enough to condition S2/S3", and that is covered by criteria 1–5.
- **Not golfability.** No play window scoring, no slope caps — S5's job,
  measured on the finished surface.
- **Not amplitude.** Relief magnitude is the drawn budget × share dial;
  whether the *budget* is right is an S0 envelope question, not an S1
  rendering question.

## What a failure here predicts downstream

C1 is the conditioning input for everything after it. If a class is
illegible now, S2 will carve drainage into a shape that doesn't express the
drawn identity, and S3 will texture it faithfully — producing a *realistic
surface of the wrong landform*. That failure is invisible to the metric
battery (the shared invariants still pass) and expensive to trace later.
This gate is cheap now and unpayable after S3. That asymmetry is why the
blind test runs before Phase D starts.

## Gate scope (revised 2026-08-10, after five blind sessions)

The blind gate covers FIVE classes: valley_floor, interfluve,
escarpment_face, terrace_flight, piedmont_slope. **basin_margin is
retained as a class but exempted from C1-level blind legibility**, by
the following evidence and rationale:

- Across five sessions and three targeted shape rounds (pocket added;
  pocket enlarged + rim; wave-suppressed calm floor), every other
  confusion was resolved — but PS↔BM flips persisted to the end, while
  all non-BM judgments in the final sessions were correct (15/15 on the
  five-class basis in session 5).
- The reviewer's verdict: the basin_margin CONCEPT is worth keeping;
  its C1 expression is inherently subtle — a rim window onto a basin
  mostly beyond the tile reads as a slope until water and drainage make
  the basin literal.
- basin_margin's real work is downstream and unaffected: its
  accommodation signature conditions S2 (ponding, embryo country), its
  class-relief multiplier drives river-valley margin sites (E6 fit),
  and S4's water will render the basin floor as the visibly enclosed
  wet flat it is. **BM legibility is therefore judged at S2/S4 review**
  (a BM window should show a calm, accommodation-rich, pond-prone
  terminal flat), not on the bare C1 hillshade.
- Future coupling noted, not yet implemented: S2's closed-basin embryo
  placement could weight by C1 accommodation, which would make BM
  windows literally kettle/pond-rich — strengthening the downstream
  legibility this re-scope relies on.

## Session log

| Date | Reviewer | Blind score | Confusions | Verdict |
|------|----------|------------|------------|---------|
| 2026-08-08 | davisolmsted | 17/18 | basin_margin→piedmont_slope (c1_d only; low-relief seed — flat + toe break sink into the modes; c1_j, same class at normal relief, read correctly) | **PASS** (gate 15/18). Not a shape bug; revisit only if the pair recurs. |
| 2026-08-10 s2 | davisolmsted | 11/18 | all 7 in ramp family {BM,PS,TF} after the many-wave field landed | ramp family under-authored; BM given a pocket, TF risers sharpened |
| 2026-08-10 s3 | davisolmsted | 11/18 | BM black hole: 0/3 recognized + 3 false positives | pocket enlarged + rim; curves decorrelated + heavy-tailed |
| 2026-08-10 s4 | davisolmsted | 12/18 | all 6 = PS↔BM flips | wave-suppressed calm basin floor (geomorphic: depositional flats) |
| 2026-08-10 s5 | davisolmsted | 12/18 (15/15 on five classes) | PS↔BM only, again | **GATE RE-SCOPED** to five classes: PASS. BM retained, judged at S2/S4 (see Gate scope). |
| 2026-08-13 | davisolmsted | **15/15** | none | **PASS** — five-class gate re-certified on the post-variety-fixes synthesis (per-biome wave dials, rv grain lock, sandhills macro continuum, height-scaled terrace risers). |
