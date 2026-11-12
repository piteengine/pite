// SPDX-License-Identifier: MIT OR Apache-2.0
//! Signal definitions and registry for engine and user signal routing.

use std::collections::HashMap;

/// A registered signal definition specifying its event name and typed payload arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalDef {
    /// Signal identifier (e.g. `"timeout"`, `"pressed"`, or `"health_changed"`).
    pub name: String,
    /// Ordered list of payload argument type names (e.g. `["int", "string"]`).
    pub payload: Vec<String>,
}

/// Registry storing signal definitions for validation and dynamic dispatch.
#[derive(Debug, Default)]
pub struct SignalRegistry {
    defs: HashMap<String, SignalDef>,
}

impl SignalRegistry {
    /// Creates a new empty signal registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new signal definition with the given name and payload signature.
    pub fn register(&mut self, name: impl Into<String>, payload: Vec<String>) {
        let name = name.into();
        self.defs.insert(name.clone(), SignalDef { name, payload });
    }

    /// Looks up a signal definition by name.
    pub fn get(&self, name: &str) -> Option<&SignalDef> {
        self.defs.get(name)
    }

    /// Returns the number of registered signals.
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Returns true if no signals are registered.
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
