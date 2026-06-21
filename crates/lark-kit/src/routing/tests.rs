use super::*;
use crate::routing::matcher::matches;

fn chat(id: &str) -> Destination {
    Destination {
        kind: DestKind::Chat,
        target: id.into(),
    }
}

fn dm(email: &str) -> Destination {
    Destination {
        kind: DestKind::Dm,
        target: email.into(),
    }
}

fn rule(m: &str, events: &[&str], dests: Vec<Destination>) -> Rule {
    Rule {
        match_: m.into(),
        events: events.iter().map(|s| s.to_string()).collect(),
        destinations: dests,
    }
}

fn spec(features: RoutingFeatures) -> RoutingSpec {
    const EVENTS: &[RoutingEvent] = &[RoutingEvent {
        value: "merge_request",
        label: "Merge request",
        description: "Merge request events",
    }];
    RoutingSpec {
        namespace: "test",
        subject: SubjectSpec {
            label: "Project",
            placeholder: "group/project",
            help: "Project path",
        },
        events: EVENTS,
        features,
    }
}

#[test]
fn matches_wildcard_prefix_and_exact() {
    assert!(matches("*", "any/thing"));
    assert!(matches("grp/*", "grp"));
    assert!(matches("grp/*", "grp/proj"));
    assert!(matches("grp/*", "grp/sub/proj"));
    assert!(!matches("grp/*", "grpx"));
    assert!(!matches("grp/*", "other/proj"));
    assert!(matches("grp/proj", "grp/proj"));
    assert!(!matches("grp/proj", "grp/other"));
}

#[test]
fn destinations_union_and_dedup() {
    let cfg = Config {
        rules: vec![
            rule("grp/*", &[], vec![chat("c1"), dm("a@x")]),
            rule("grp/proj", &["merge_request"], vec![chat("c1"), chat("c2")]),
        ],
        ..Default::default()
    };
    let got = cfg.destinations_for("grp/proj", "merge_request");
    // c1 deduped across rules; order = first-seen.
    assert_eq!(got, vec![chat("c1"), dm("a@x"), chat("c2")]);
}

#[test]
fn event_filter_excludes_but_does_not_fall_back_to_default() {
    let cfg = Config {
        rules: vec![rule("grp/proj", &["pipeline"], vec![chat("c1")])],
        default_destinations: vec![dm("fallback@x")],
        ..Default::default()
    };
    // subject matches the rule, event doesn't → no destinations, no default.
    assert!(cfg.destinations_for("grp/proj", "issue").is_empty());
}

#[test]
fn default_used_only_when_no_rule_matches_subject() {
    let cfg = Config {
        rules: vec![rule("grp/*", &[], vec![chat("c1")])],
        default_destinations: vec![dm("fallback@x")],
        ..Default::default()
    };
    assert_eq!(
        cfg.destinations_for("other/proj", "issue"),
        vec![dm("fallback@x")]
    );
    assert_eq!(cfg.destinations_for("grp/proj", "issue"), vec![chat("c1")]);
}

#[test]
fn tolerant_decode_and_lookups() {
    // Missing fields decode to defaults.
    let cfg: Config = serde_json::from_str("{}").unwrap();
    assert!(cfg.rules.is_empty());
    // Partial blob with only user_map + alert_labels.
    let cfg: Config = serde_json::from_str(
        r#"{"user_map":[{"username":"octo","lark_email":"o@x"}],"alert_labels":["bug","P0"]}"#,
    )
    .unwrap();
    assert_eq!(cfg.lark_email("octo"), Some("o@x"));
    assert_eq!(cfg.lark_email("nobody"), None);
    assert!(cfg.is_alert_label("BUG"));
    assert!(cfg.is_alert_label("p0"));
    assert!(!cfg.is_alert_label("wontfix"));
}

#[test]
fn validate_rejects_empty_target_and_match() {
    let bad_target = Config {
        rules: vec![rule("grp/*", &[], vec![chat("")])],
        ..Default::default()
    };
    assert!(bad_target.validate().is_err());
    let bad_match = Config {
        rules: vec![rule("  ", &[], vec![chat("c1")])],
        ..Default::default()
    };
    assert!(bad_match.validate().is_err());
    let ok = Config {
        rules: vec![rule("grp/*", &[], vec![chat("c1")])],
        ..Default::default()
    };
    assert!(ok.validate().is_ok());
}

#[test]
fn validate_for_rejects_unknown_events() {
    let cfg = Config {
        rules: vec![rule("grp/*", &["issue"], vec![chat("c1")])],
        ..Default::default()
    };
    let err = cfg
        .validate_for(&spec(RoutingFeatures::SOURCE_WITH_ALERTS))
        .unwrap_err();
    assert!(err.contains("unknown routing event 'issue'"));
}

#[test]
fn validate_for_rejects_unsupported_feature_fields() {
    let cfg = Config {
        user_map: vec![UserMap {
            username: "octo".into(),
            lark_email: "octo@example.com".into(),
        }],
        ..Default::default()
    };
    let err = cfg
        .validate_for(&spec(RoutingFeatures::ROUTING_ONLY))
        .unwrap_err();
    assert_eq!(err, "user_map is not supported by this app");
}
