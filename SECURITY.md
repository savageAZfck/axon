# Security

Report vulnerabilities privately to savagetism@icloud.com — do not open
public issues for exploitable weaknesses.

Scope: trigger matching, hard-bounded action semantics (brake/alert/quarantine — never steer), firing log hash-chaining, cooldown and storm-guard bounds.

Out of scope: the host's interpretation of quarantine/freeze signals, and event producers — a hostile event source is bounded by the storm guard but events themselves are trusted inputs (see THREAT_MODEL.md).
