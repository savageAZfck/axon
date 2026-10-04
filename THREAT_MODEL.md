# Threat model — axon

An attacker floods events to hide a firing — the storm guard caps firings and freeze-mode logs bounded summaries. A reflex is weaponized to steer behavior — actions are hard-bounded to brake/alert/quarantine; there is no action that selects behavior. The reflex log is rewritten — it is hash-chained. A trigger is bypassed by malformed fields — matching is byte-exact over canonical JSON.

## What this crate guarantees

- Reflex actions are a closed set: brake, alert, quarantine — none can select behavior.
- Every firing is committed to a hash-chained log with bounds (cooldown, max-firings, storm caps).
- Freeze state is explicit, auditable, and requires deliberate clearing.

## What it does not guarantee

- Protection against a verifier who never calls `verify()`.
- Integrity of inputs produced by other systems — this crate verifies
  signatures and chains over what it is given; garbage that verifies is
  still garbage.
