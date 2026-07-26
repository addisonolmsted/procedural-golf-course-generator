# Step 01 — Seed

**Status:** unclaimed

## Purpose

Establish the single source of randomness and the identity of a generated
course. Everything downstream is a pure function of the master seed (plus
explicit user overrides); two runs with the same seed are byte-identical.

## Position

- Upstream: nothing (pipeline entry).
- Downstream: step 02 consumes the seed; every step derives its random
  streams from it.

## Contract

Input: a user- or caller-supplied `u64` (or none ⇒ drawn from entropy ONCE and
recorded — never silently re-drawn).

Output — `RunIdentity`:

```
RunIdentity {
  seed: u64,
  pipeline_version: u32,      // global contract version
  attempt: u32,               // 0 for the first try; >0 after gate rerolls
}
```

Derived-stream rule (used by all steps): a step obtains randomness only via
`stream(seed, "<step>/<purpose>/v1")` — a keyed, domain-split, platform-stable
generator (prior art: blake3-keyed ChaCha8,
`terrain-v2:golf-core/src/det.rs`). Stream names form a registry in
ARCHITECTURE.md once code exists; reusing a stream for two purposes is
forbidden.

Reroll rule (owned here, triggered by step 07): attempt `n+1` uses
`sub_seed = stream(seed, "reroll/v1").nth(n)`, so a reroll is itself
deterministic and the whole retry sequence replays from the master seed.

## Per-archetype behavior

None — this step is archetype-independent.

## Hard requirements

1. Byte-identical double-run at every downstream step given equal
   `RunIdentity`.
2. Platform-stable: identical output across OS/architecture (no
   `HashMap`-iteration-order dependence, no platform float intrinsics).
3. The seed and attempt number appear in every artifact's manifest so any
   output file is traceable to its run.

## References

- `terrain-v2:golf-core/src/det.rs` — the proven determinism kit (golden
  stream test included).
- `archetype-pipeline:course-contracts/src/rng.rs` — a channel-registry
  pattern for stream names.

## Open questions

- Does the game client need a human-friendly seed encoding (course codes) or
  is raw u64 fine for now?
