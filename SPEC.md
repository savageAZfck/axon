# SPEC — axon (audited reflex arc)

## Model

`ReflexArc` holds installed `Reflex`es and a hash-chained firing log.
Events arrive via `sense(event)`; matching reflexes fire bounded actions.

## Trigger

Closed grammar:

- `KindIs(<glob>)` — event kind matches
- `FieldIs(<name>=<glob>)` — a field matches a glob (wildcards `*`)
- `Gt|Ge|Lt|Le(<field>=<number>)` — numeric comparisons

## Action

Closed set — reflexes can stop, never steer:

- `Brake` — halt the dispatch
- `Alert` — raise attention
- `Quarantine` — isolate the source

There is deliberately no action that selects what to do instead.

## Bounds

Each reflex carries `cooldown_ms` and `max_firings`. A spent reflex must
be `rearm`ed. An arc-wide storm guard caps total firings per window and
enters `frozen` state — an explicit, auditable condition cleared only by
`clear_freeze()`.

## Firing record

```json
{
  "seq": 0, "reflex": "id", "event_kind": "…", "action": "quarantine",
  "matched": "trigger summary", "prev_hash": "…", "hash": "…", "ts": 0
}
```

`verify()` replays the firing log's chain.

## Invariants

- Reflexes cannot be installed with an out-of-grammar trigger.
- Bounds are enforced before firing, not after.
- The log records every firing — a reflex cannot act silently.
