use super::model::Config;
use super::spec::RoutingSpec;

impl Config {
    /// Reject structurally invalid configs (empty match/target) before saving.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with_spec(None)
    }

    /// Reject invalid configs using an App's routing spec for semantic checks.
    pub fn validate_for(&self, spec: &RoutingSpec) -> Result<(), String> {
        self.validate_with_spec(Some(spec))
    }

    fn validate_with_spec(&self, spec: Option<&RoutingSpec>) -> Result<(), String> {
        let event_values = spec.map(RoutingSpec::event_values);
        for rule in &self.rules {
            if rule.match_.trim().is_empty() {
                return Err("rule match must not be empty".into());
            }
            if let Some(values) = &event_values {
                for event in &rule.events {
                    if !values.contains(event.as_str()) {
                        return Err(format!("unknown routing event '{event}'"));
                    }
                }
            }
            for d in &rule.destinations {
                d.validate()?;
            }
        }
        for d in &self.default_destinations {
            d.validate()?;
        }
        if let Some(spec) = spec {
            if !spec.features.user_map && !self.user_map.is_empty() {
                return Err("user_map is not supported by this app".into());
            }
            if !spec.features.alert_labels && !self.alert_labels.is_empty() {
                return Err("alert_labels is not supported by this app".into());
            }
        }
        Ok(())
    }
}
