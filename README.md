# axon

**An audited reflex arc for autonomous systems — signals that route around deliberation, bounded so they can brake but never steer.**

Deliberation is slow by design: councils vote, policies evaluate, approvals wait. Some events can't wait — a kill phrase, an exfiltration pattern, a hardware-critical threshold. Biology solved this with the reflex arc; `axon` gives agents the same.

## Two invariants

1. **Reflexes brake, never steer.** The complete action space is `Brake`, `Alert`, `Quarantine`. A reflex can stop the organism or raise a hand — it cannot act on the world, dispatch tools, or emit output. Fast *because* bounded.
2. **Every firing is ledgered.** Each firing lands on the arc's own hash-chained log with a hash of its trigger evidence. Sub-deliberative, never sub-accountable.

## Bounds

- Per-reflex cooldowns and `max_firings` — a reflex can be *spent* and must be deliberately re-armed.
- Arc-wide storm window caps total firings — a trigger flood can't flood the log.
- A `Brake` firing freezes the arc until deliberation clears it; frozen mode asserts Brake on everything without evaluating triggers.
- Reflexes cannot create reflexes — `sense()` has no path to `install()`.

## Try it

```bash
cargo run --example reflex
cargo test
```

## Pair with

- `wintercount` — deliberative policy above the arc.
- `akicita` — the sentinel that watches what the arc asserts.
- `flight_tape` — the recorder that makes the arc's log part of the incident bundle.
