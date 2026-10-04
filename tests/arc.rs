use axon::*;
use serde_json::{json, Value};

fn ev(kind: &str, data: Value) -> SenseEvent {
    SenseEvent::new(kind, data)
}

#[test]
fn field_match_fires() {
    let mut arc = ReflexArc::new();
    arc.install(Reflex::new(
        "exfil",
        Trigger::FieldMatch {
            field: "tool".into(),
            pattern: "send_*".into(),
        },
        ReflexAction::Quarantine,
    ))
    .unwrap();
    let f = arc.sense(&ev("tool_dispatch", json!({"tool": "send_file"})));
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].action, ReflexAction::Quarantine);
    arc.verify().unwrap();
}

#[test]
fn non_matching_event_is_silent() {
    let mut arc = ReflexArc::new();
    arc.install(Reflex::new(
        "exfil",
        Trigger::KindIs("exfil_attempt".into()),
        ReflexAction::Alert,
    ))
    .unwrap();
    assert!(arc.sense(&ev("tool_dispatch", json!({}))).is_empty());
}

#[test]
fn threshold_trigger() {
    let mut arc = ReflexArc::new();
    arc.install(Reflex::new(
        "thermal",
        Trigger::Threshold {
            field: "temp_c".into(),
            op: ThresholdOp::Gt,
            value: 95.0,
        },
        ReflexAction::Brake,
    ))
    .unwrap();
    assert!(arc.sense(&ev("sensor", json!({"temp_c": 80.0}))).is_empty());
    assert_eq!(arc.sense(&ev("sensor", json!({"temp_c": 99.0}))).len(), 1);
}

#[test]
fn conjunction_requires_all() {
    let mut arc = ReflexArc::new();
    arc.install(Reflex::new(
        "exfil-net",
        Trigger::All(vec![
            Trigger::KindIs("tool_dispatch".into()),
            Trigger::FieldMatch {
                field: "dest".into(),
                pattern: "external".into(),
            },
        ]),
        ReflexAction::Alert,
    ))
    .unwrap();
    assert!(arc
        .sense(&ev("tool_dispatch", json!({"dest": "local"})))
        .is_empty());
    assert_eq!(
        arc.sense(&ev("tool_dispatch", json!({"dest": "external"})))
            .len(),
        1
    );
}

#[test]
fn brake_freezes_until_deliberation_clears() {
    let mut arc = ReflexArc::new();
    arc.install(Reflex::new(
        "kill",
        Trigger::KindIs("kill_switch".into()),
        ReflexAction::Brake,
    ))
    .unwrap();
    arc.sense(&ev("kill_switch", json!({})));
    assert!(arc.frozen);
    // While frozen, everything asserts Brake — no trigger evaluation.
    let f = arc.sense(&ev("tool_dispatch", json!({"tool": "rm"})));
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].reflex_id, "freeze");
    assert!(arc.clear_freeze());
    assert!(!arc.frozen);
    arc.verify().unwrap();
}

#[test]
fn cooldown_limits_reflex() {
    let mut arc = ReflexArc::new();
    arc.install(
        Reflex::new(
            "chatty",
            Trigger::KindIs("ping".into()),
            ReflexAction::Alert,
        )
        .with_bounds(60_000, 0),
    )
    .unwrap();
    let e = ev("ping", json!({}));
    assert_eq!(arc.sense(&e).len(), 1);
    // Same ts — inside cooldown, silent.
    assert!(arc.sense(&e).is_empty());
}

#[test]
fn max_firings_spends_the_reflex() {
    let mut arc = ReflexArc::new();
    arc.install(
        Reflex::new(
            "limited",
            Trigger::KindIs("tick".into()),
            ReflexAction::Alert,
        )
        .with_bounds(0, 2),
    )
    .unwrap();
    let mut e1 = ev("tick", json!({}));
    let mut e2 = ev("tick", json!({}));
    let mut e3 = ev("tick", json!({}));
    e1.ts = 1;
    e2.ts = 2;
    e3.ts = 3;
    assert_eq!(arc.sense(&e1).len(), 1);
    assert_eq!(arc.sense(&e2).len(), 1);
    assert!(arc.sense(&e3).is_empty()); // spent
    assert!(arc.rearm("limited"));
    let mut e4 = ev("tick", json!({}));
    e4.ts = 4;
    assert_eq!(arc.sense(&e4).len(), 1);
}

#[test]
fn global_storm_window_bounds_the_arc() {
    let mut arc = ReflexArc::new();
    arc.global_rate = 3;
    arc.window_ms = 60_000;
    arc.install(Reflex::new(
        "any",
        Trigger::KindIs("*".into()),
        ReflexAction::Alert,
    ))
    .unwrap();
    let mut n = 0;
    for i in 0..10 {
        let mut e = ev("anything", json!({}));
        e.ts = 1000;
        n += arc.sense(&e).len();
        let _ = i;
    }
    assert_eq!(n, 3);
}

#[test]
fn chain_detects_tamper() {
    let mut arc = ReflexArc::new();
    arc.install(Reflex::new(
        "any",
        Trigger::KindIs("*".into()),
        ReflexAction::Alert,
    ))
    .unwrap();
    arc.sense(&ev("x", json!({})));
    arc.log[0].action = ReflexAction::Quarantine;
    assert!(arc.verify().is_err());
}

#[test]
fn reflexes_cannot_create_reflexes() {
    // The API has no path from sense() to install() — this test documents
    // that: firings return assertions only, the arc's reflex table is
    // unchanged by any sensed event.
    let mut arc = ReflexArc::new();
    arc.install(Reflex::new(
        "a",
        Trigger::KindIs("x".into()),
        ReflexAction::Alert,
    ))
    .unwrap();
    arc.sense(&ev("x", json!({"install": "evil-reflex"})));
    assert_eq!(arc.reflexes.len(), 1);
}
