use std::collections::HashMap;

/// Recorded design for the M2 signal system (SCAFFOLD-SPEC §12).
/// A signal is a name plus ordered payload type names, registered at
/// class-load. No dispatch yet — Python surface stays reserved
/// (`connect(callable)` / `emit(typed values)` land here in M2).
#[derive(Debug, Clone, PartialEq)]
pub struct SignalDef {
    pub name: String,
    pub payload: Vec<String>,
}

#[derive(Debug, Default)]
pub struct SignalRegistry {
    defs: HashMap<String, SignalDef>,
}

impl SignalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: impl Into<String>, payload: Vec<String>) {
        let name = name.into();
        self.defs.insert(
            name.clone(),
            SignalDef {
                name,
                payload,
            },
        );
    }

    pub fn get(&self, name: &str) -> Option<&SignalDef> {
        self.defs.get(name)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_records_name_and_payload() {
        let mut registry = SignalRegistry::new();
        registry.register("health_changed", vec!["int".to_string()]);
        let def = registry.get("health_changed").unwrap();
        assert_eq!(def.payload, vec!["int".to_string()]);
    }
}
