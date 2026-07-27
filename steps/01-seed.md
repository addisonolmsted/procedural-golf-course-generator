# Step 01 — Seed

**Status:** built (claude, 2026-07-26)

## Purpose

Establish the single source of randomness and the identity of a generated
course. Everything downstream is a pure function of the master seed (plus
explicit user overrides); two runs with the same seed are byte-identical.

## Position

- Upstream: nothing (pipeline entry).
- Downstream: step 02 consumes the seed; every step derives its random
  streams from it.

## Contract

Implementation: [`crates/course-seed/`](../crates/course-seed/).

Input: a user- or caller-supplied `u64` (`RunIdentity::from_seed`), or none ⇒
drawn from OS entropy ONCE and recorded (`RunIdentity::from_entropy`) — never
silently re-drawn. The caller persists the artifact before running the
pipeline.

Output — `RunIdentity`:

```
RunIdentity {
  seed: u64,              // ALWAYS the master seed, across rerolls
  pipeline_version: u32,  // global contract version (PIPELINE_VERSION = 1)
  attempt: u32,           // 0 for the first try; >0 after gate rerolls
}
```

Artifact: `run.json` — compact canonical JSON, fixed field order
`seed, pipeline_version, attempt`, u64 as a JSON number, no trailing newline
(`RunIdentity::canonical_json`). Integer-only, so round-trips are exact.
Loading (`RunIdentity::from_json`) is loud on any contract mismatch: unknown
fields, a `pipeline_version` other than the current one, or an `attempt`
outside `0..MAX_ATTEMPTS` are all rejected, never silently run.

Derived-stream rule (used by all steps): a step obtains randomness only via
`RunIdentity::stream("<step>/<purpose>/v1")` — a keyed, domain-split,
platform-stable generator (blake3-keyed ChaCha8, domain tag
`course-seed/detrng/v1`; changing the tag or any stream name rekeys streams
and requires a `pipeline_version` bump + golden re-bless). Stream names form
a registry in `crates/course-seed/src/streams.rs`, mirrored (test-enforced)
in ARCHITECTURE.md; reusing a stream for two purposes is forbidden, and
`stream()` rejects unregistered names.

Reroll rule (owned here, triggered by step 07): `RunIdentity::reroll()`
returns the same master seed with `attempt + 1`, bounded by
`MAX_ATTEMPTS = 8` (attempts `0..8`, then `AttemptsExhausted`). The working
seed for an attempt is derived, never stored — `stream_seed()`:

- attempt `0` → the master seed;
- attempt `a > 0` → draw `a - 1` (0-based) of `stream(seed, "reroll/v1")`,
  i.e. attempt `n+1` uses `.nth(n)`.

So a reroll is itself deterministic and the whole retry sequence replays from
the master seed alone. `reroll/v1` is internal to this step: it is keyed off
the MASTER seed while `stream()` keys off the attempt seed, so `stream()`
refuses to open it.

## Per-archetype behavior

None — this step is archetype-independent.

## Hard requirements

1. Byte-identical double-run at every downstream step given equal
   `RunIdentity`. (Rehearsed here by `tests/double_run.rs`: multi-stream
   transcript + canonical JSON, run twice, byte-equal; reroll sequence
   replayed from the master seed.)
2. Platform-stable: identical output across OS/architecture (no
   `HashMap`-iteration-order dependence, no platform float intrinsics).
   Holds by construction: blake3 + ChaCha8 are pure integer code; the only
   float op is the IEEE-exact 53-bit mantissa mapping
   `(u64 >> 11) as f64 * 2⁻⁵³`.
3. The seed and attempt number appear in every artifact's manifest so any
   output file is traceable to its run. (Constraint on the P1 artifact
   store: embed `RunIdentity` — it is `Copy` + serde — in every manifest.)

Golden values (pinned in tests; bless only on an intentional RNG-contract
change, with a `pipeline_version` bump): `golden_stream_u64` /
`golden_stream_f64` (raw DetRng stream), `golden_reroll_sub_seeds` (reroll
rule), `canonical_json_golden` (artifact bytes).

## References

- `terrain-v2:golf-core/src/det.rs` — the determinism kit this crate's
  `det.rs` was ported from (new domain tag, so all streams are disjoint from
  every stream older branches ever produced).
- `archetype-pipeline:course-contracts/src/rng.rs` — the channel-registry
  pattern `streams.rs` follows (this branch drops the `cp/` prefix; the step
  docs' bare names are canonical).

## Open questions

- ~~Does the game client need a human-friendly seed encoding (course codes)
  or is raw u64 fine for now?~~ Resolved 2026-07-26: raw u64 for now; a
  human-friendly encoding is deferred until a game-client need exists (it
  would be a pure, non-breaking presentation layer over the u64).
