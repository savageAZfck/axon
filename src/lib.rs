//! axon — a reflex arc for autonomous systems.
//!
//! Deliberation is slow by design: councils vote, policies evaluate,
//! approvals wait. But some events cannot wait — a kill phrase, an
//! exfiltration pattern, a hardware-critical threshold. Biological
//! nervous systems solved this with the reflex arc: signals that route
//! around the brain entirely.
//!
//! `axon` gives an agent the same: [`Reflex`]es that fire below
//! deliberation. Two invariants make it safe to keep reflexes fast:
//!
//! 1. **Reflexes can brake, never steer.** The only actions are
//!    [`ReflexAction::Brake`], `Alert`, and `Quarantine` — a reflex can
//!    stop the organism or raise a hand, but it cannot act on the world,
//!    dispatch tools, or produce output. Fast because bounded.
//! 2. **Every firing is ledgered.** Each firing lands on the arc's own
//!    hash-chained log with its trigger evidence — reflexes are
//!    sub-deliberative, never sub-accountable.
//!
//! ```no_run
//! use axon::{ReflexArc, Reflex, ReflexAction, Trigger, SenseEvent};
//!
//! let mut arc = ReflexArc::new();
//! arc.install(Reflex::new(
//!     "exfil-brake",
//!     Trigger::FieldMatch { field: "tool".into(), pattern: "send_*".into() },
//!     ReflexAction::Quarantine,
//! ));
//! let firings = arc.sense(&SenseEvent::new("tool_dispatch",
//!     serde_json::json!({"tool": "send_file", "dest": "external"})));
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[derive(Debug)]
pub enum Error {
    BadReflex(String),
    BoundExceeded(String),
    BadChain(String),
    Serde(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadReflex(m) => write!(f, "invalid reflex: {m}"),
            Error::BoundExceeded(m) => write!(f, "reflex bound exceeded: {m}"),
            Error::BadChain(m) => write!(f, "arc chain broken: {m}"),
            Error::Serde(e) => write!(f, "serde: {e}"),
        }
    }
}

impl std::error::Error for Error {}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serde(e)
    }
}

// ---------------------------------------------------------------- trigger

/// What a reflex responds to. Deliberately inexpressive — reflexes
/// match structure, they do not compute policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Trigger {
    /// Event kind equals (or glob-matches with trailing `*`) this value.
    KindIs(String),
    /// A named field's string form matches (trailing `*` glob).
    FieldMatch { field: String, pattern: String },
    /// A named numeric field crosses a threshold.
    Threshold {
        field: String,
        op: ThresholdOp,
        value: f64,
    },
    /// All of the sub-triggers match.
    All(Vec<Trigger>),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ThresholdOp {
    Gt,
    Lt,
    Ge,
    Le,
}

fn glob_match(pattern: &str, value: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        value.starts_with(prefix)
    } else {
        pattern == value
    }
}

impl Trigger {
    fn matches(&self, ev: &SenseEvent) -> bool {
        match self {
            Trigger::KindIs(k) => glob_match(k, &ev.kind),
            Trigger::FieldMatch { field, pattern } => ev
                .data
                .get(field)
                .map(|v| match v {
                    Value::String(s) => glob_match(pattern, s),
                    other => glob_match(pattern, &other.to_string()),
                })
                .unwrap_or(false),
            Trigger::Threshold { field, op, value } => ev
                .data
                .get(field)
                .and_then(|v| v.as_f64())
                .map(|n| match op {
                    ThresholdOp::Gt => n > *value,
                    ThresholdOp::Lt => n < *value,
                    ThresholdOp::Ge => n >= *value,
                    ThresholdOp::Le => n <= *value,
                })
                .unwrap_or(false),
            Trigger::All(ts) => ts.iter().all(|t| t.matches(ev)),
        }
    }
}

// ---------------------------------------------------------------- action

/// What a reflex may do. The complete action space — reflexes brake,
/// they never steer.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReflexAction {
    /// Halt the current dispatch/generation — pull the hand off the stove.
    Brake,
    /// Raise an owner-visible alert without stopping anything.
    Alert,
    /// Isolate the triggering subject (tool, peer, process) from the
    /// organism until deliberation reviews it.
    Quarantine,
}

// ---------------------------------------------------------------- reflex

/// A single installed reflex: trigger, action, and its own bounds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reflex {
    pub id: String,
    pub trigger: Trigger,
    pub action: ReflexAction,
    /// Minimum milliseconds between firings of *this* reflex.
    pub cooldown_ms: u64,
    /// Maximum firings — after exhaustion the reflex is spent and must be
    /// deliberately re-armed. Reflexes cannot run forever unnoticed.
    pub max_firings: u64,
}

impl Reflex {
    pub fn new(id: &str, trigger: Trigger, action: ReflexAction) -> Self {
        Reflex {
            id: id.to_string(),
            trigger,
            action,
            cooldown_ms: 0,
            max_firings: 0, // 0 = unlimited, still audited
        }
    }

    pub fn with_bounds(mut self, cooldown_ms: u64, max_firings: u64) -> Self {
        self.cooldown_ms = cooldown_ms;
        self.max_firings = max_firings;
        self
    }

    fn validate(&self) -> Result<(), Error> {
        if self.id.is_empty() {
            return Err(Error::BadReflex("empty id".into()));
        }
        if let Trigger::All(ts) = &self.trigger {
            if ts.is_empty() {
                return Err(Error::BadReflex("empty conjunction".into()));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- events + log

/// An event the arc senses — a tool dispatch, an inbound message, a
/// resource reading. Structured but inert.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SenseEvent {
    pub kind: String,
    pub data: Value,
    pub ts: u64,
}

impl SenseEvent {
    pub fn new(kind: &str, data: Value) -> Self {
        SenseEvent {
            kind: kind.to_string(),
            data,
            ts: now_millis(),
        }
    }
}

/// One firing on the arc log: what fired, on what evidence, and the
/// action asserted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Firing {
    pub seq: u64,
    pub reflex_id: String,
    pub action: ReflexAction,
    /// SHA-256 of the triggering event — evidence without copying payload.
    pub evidence_hash: String,
    pub event_kind: String,
    pub ts: u64,
    pub prev: String,
    pub hash: String,
}

// ---------------------------------------------------------------- the arc

/// The reflex arc: installed reflexes, per-reflex firing state, and a
/// hash-chained log of everything that ever fired.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReflexArc {
    pub reflexes: Vec<Reflex>,
    pub log: Vec<Firing>,
    /// reflex_id -> (last_fired_ms, total_firings).
    #[serde(default)]
    state: BTreeMap<String, (u64, u64)>,
    /// Arc-wide guard: at most this many firings across all reflexes in
    /// any `window_ms`. A storm of triggers cannot flood the arc.
    pub global_rate: u32,
    pub window_ms: u64,
    #[serde(default)]
    window_start: u64,
    #[serde(default)]
    window_count: u32,
    /// When true the arc asserts Brake on every sense — engaged after a
    /// kill-switch firing until deliberation clears it.
    pub frozen: bool,
}

impl Default for ReflexArc {
    fn default() -> Self {
        Self::new()
    }
}

impl ReflexArc {
    pub fn new() -> Self {
        ReflexArc {
            reflexes: Vec::new(),
            log: Vec::new(),
            state: BTreeMap::new(),
            global_rate: 64,
            window_ms: 60_000,
            window_start: 0,
            window_count: 0,
            frozen: false,
        }
    }

    /// Install a reflex. Ids must be unique.
    pub fn install(&mut self, reflex: Reflex) -> Result<(), Error> {
        reflex.validate()?;
        if self.reflexes.iter().any(|r| r.id == reflex.id) {
            return Err(Error::BadReflex(format!("duplicate id: {}", reflex.id)));
        }
        self.reflexes.push(reflex);
        Ok(())
    }

    /// Remove a reflex by id (deliberative act — not itself a reflex).
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.reflexes.len();
        self.reflexes.retain(|r| r.id != id);
        self.reflexes.len() != before
    }

    fn tip(&self) -> String {
        self.log
            .last()
            .map(|f| f.hash.clone())
            .unwrap_or_else(|| "0".repeat(64))
    }

    fn record(&mut self, reflex_id: &str, action: ReflexAction, ev: &SenseEvent) -> Firing {
        let seq = self.log.len() as u64;
        let prev = self.tip();
        let evidence_hash = sha256_hex(
            &serde_json::to_vec(&json!({"kind": ev.kind, "data": ev.data, "ts": ev.ts}))
                .unwrap_or_default(),
        );
        let hash = sha256_hex(
            &serde_json::to_vec(&json!({
                "seq": seq, "reflex_id": reflex_id, "action": action,
                "evidence_hash": evidence_hash, "event_kind": ev.kind,
                "ts": ev.ts, "prev": prev,
            }))
            .unwrap_or_default(),
        );
        let firing = Firing {
            seq,
            reflex_id: reflex_id.to_string(),
            action,
            evidence_hash,
            event_kind: ev.kind.clone(),
            ts: ev.ts,
            prev,
            hash,
        };
        self.log.push(firing.clone());
        firing
    }

    /// Sense an event: evaluate every installed reflex, enforce bounds,
    /// record firings. Returns the firings — the *caller* decides how to
    /// act on Brake/Alert/Quarantine assertions; the arc asserts, it does
    /// not execute.
    pub fn sense(&mut self, ev: &SenseEvent) -> Vec<Firing> {
        // Global storm guard.
        if ev.ts.saturating_sub(self.window_start) > self.window_ms {
            self.window_start = ev.ts;
            self.window_count = 0;
        }

        let mut fired = Vec::new();
        if self.frozen {
            // Engaged freeze asserts Brake on everything — deliberately
            // cheap: no trigger evaluation while frozen. Still bounded
            // by the global storm window so the log can't flood.
            if self.window_count < self.global_rate {
                fired.push(self.record("freeze", ReflexAction::Brake, ev));
                self.window_count += 1;
            }
            return fired;
        }

        for reflex in self.reflexes.clone() {
            if self.window_count >= self.global_rate {
                break;
            }
            if !reflex.trigger.matches(ev) {
                continue;
            }
            let (last, count) = self.state.get(&reflex.id).copied().unwrap_or((0, 0));
            if reflex.cooldown_ms > 0 && ev.ts.saturating_sub(last) < reflex.cooldown_ms {
                continue;
            }
            if reflex.max_firings > 0 && count >= reflex.max_firings {
                continue; // spent until re-armed
            }
            let firing = self.record(&reflex.id, reflex.action, ev);
            self.state.insert(reflex.id.clone(), (ev.ts, count + 1));
            self.window_count += 1;
            if reflex.action == ReflexAction::Brake {
                // A brake firing freezes the arc until deliberation clears.
                self.frozen = true;
            }
            fired.push(firing);
        }
        fired
    }

    /// Clear the freeze — a deliberative act. Returns false if not frozen.
    pub fn clear_freeze(&mut self) -> bool {
        std::mem::replace(&mut self.frozen, false)
    }

    /// Re-arm a spent reflex (deliberative act).
    pub fn rearm(&mut self, reflex_id: &str) -> bool {
        if let Some(s) = self.state.get_mut(reflex_id) {
            s.1 = 0;
            true
        } else {
            false
        }
    }

    /// Verify the firing log's hash chain.
    pub fn verify(&self) -> Result<(), Error> {
        let mut prev = "0".repeat(64);
        for (i, f) in self.log.iter().enumerate() {
            if f.seq != i as u64 {
                return Err(Error::BadChain(format!("seq gap at {i}")));
            }
            if f.prev != prev {
                return Err(Error::BadChain(format!("prev mismatch at {i}")));
            }
            let hash = sha256_hex(
                &serde_json::to_vec(&json!({
                    "seq": f.seq, "reflex_id": f.reflex_id, "action": f.action,
                    "evidence_hash": f.evidence_hash, "event_kind": f.event_kind,
                    "ts": f.ts, "prev": f.prev,
                }))
                .unwrap_or_default(),
            );
            if hash != f.hash {
                return Err(Error::BadChain(format!("hash mismatch at {i}")));
            }
            prev = f.hash.clone();
        }
        Ok(())
    }
}
