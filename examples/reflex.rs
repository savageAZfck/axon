//! A reflex arc in action: exfiltration brake, thermal guard, audit log.
use axon::*;
use serde_json::json;

fn main() {
    let mut arc = ReflexArc::new();

    arc.install(Reflex::new(
        "exfil-brake",
        Trigger::All(vec![
            Trigger::KindIs("tool_dispatch".into()),
            Trigger::FieldMatch {
                field: "tool".into(),
                pattern: "send_*".into(),
            },
            Trigger::FieldMatch {
                field: "dest".into(),
                pattern: "external".into(),
            },
        ]),
        ReflexAction::Quarantine,
    ))
    .unwrap();

    arc.install(
        Reflex::new(
            "thermal",
            Trigger::Threshold {
                field: "temp_c".into(),
                op: ThresholdOp::Gt,
                value: 95.0,
            },
            ReflexAction::Brake,
        )
        .with_bounds(0, 5),
    )
    .unwrap();

    let events = vec![
        SenseEvent::new(
            "tool_dispatch",
            json!({"tool": "read_file", "dest": "local"}),
        ),
        SenseEvent::new(
            "tool_dispatch",
            json!({"tool": "send_file", "dest": "external"}),
        ),
        SenseEvent::new("sensor", json!({"temp_c": 98.5})),
        SenseEvent::new("tool_dispatch", json!({"tool": "anything"})),
    ];

    for e in &events {
        let fired = arc.sense(e);
        for f in &fired {
            println!(
                "FIRED {} -> {:?} (evidence {}…)",
                f.reflex_id,
                f.action,
                &f.evidence_hash[..12]
            );
        }
        if fired.is_empty() {
            println!("quiet: {}", e.kind);
        }
    }

    println!("\narc frozen: {}", arc.frozen);
    arc.verify().expect("arc log verifies");
    println!("arc log: {} firings, chain verified", arc.log.len());
}
