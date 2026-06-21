use super::model::{Config, Destination};

impl Config {
    /// The deduplicated destinations for `(subject, event)`.
    ///
    /// Unions the destinations of every rule whose pattern matches `subject` and whose
    /// `events` filter allows `event`. If no rule matches the subject at all, falls back to
    /// [`default_destinations`](Config::default_destinations); a subject that matches a rule
    /// but not this event yields nothing (the project is configured, this event isn't routed).
    pub fn destinations_for(&self, subject: &str, event: &str) -> Vec<Destination> {
        let mut out: Vec<Destination> = Vec::new();
        let mut subject_matched = false;
        for rule in &self.rules {
            if !matches(&rule.match_, subject) {
                continue;
            }
            subject_matched = true;
            if !rule.events.is_empty() && !rule.events.iter().any(|e| e == event) {
                continue;
            }
            for d in &rule.destinations {
                push_unique(&mut out, d);
            }
        }
        if !subject_matched {
            for d in &self.default_destinations {
                push_unique(&mut out, d);
            }
        }
        out
    }
}

fn push_unique(out: &mut Vec<Destination>, d: &Destination) {
    if !out.iter().any(|o| o.kind == d.kind && o.target == d.target) {
        out.push(d.clone());
    }
}

/// Matches a subject path against a rule pattern: `"*"` = any; `"base/*"` = `base` or
/// anything under `base/`; otherwise an exact match.
pub(super) fn matches(pattern: &str, subject: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(base) = pattern.strip_suffix("/*") {
        return subject == base || subject.starts_with(&format!("{base}/"));
    }
    pattern == subject
}
