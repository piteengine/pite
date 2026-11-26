// SPDX-License-Identifier: MIT OR Apache-2.0
//! Project input action mapping (`res://input.pite.toml`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ProjectError, Result};

/// Filename of the project input action map at the project root.
pub const INPUT_MAP_FILE: &str = "input.pite.toml";

/// Current schema version for `input.pite.toml`.
pub const INPUT_MAP_FORMAT_VERSION: u32 = 1;

/// All canonical key and mouse-button names accepted by Pite's runtime input layer.
pub const KNOWN_INPUT_KEYS: &[&str] = &[
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "ArrowDown",
    "Space",
    "Enter",
    "Escape",
    "Tab",
    "Backspace",
    "Shift",
    "Control",
    "Alt",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "MouseLeft",
    "MouseRight",
    "MouseMiddle",
];

/// Parsed `res://input.pite.toml` document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputMapFile {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub actions: BTreeMap<String, Vec<String>>,
}

impl Default for InputMapFile {
    fn default() -> Self {
        Self {
            format_version: INPUT_MAP_FORMAT_VERSION,
            actions: BTreeMap::new(),
        }
    }
}

fn default_format_version() -> u32 {
    INPUT_MAP_FORMAT_VERSION
}

/// Parse one chord binding string (e.g. `"Space"`, `"Control+S"`, `"W+Shift"`).
/// Returns canonical key names or an error describing the invalid token.
pub fn parse_chord(binding: &str) -> std::result::Result<Vec<String>, String> {
    let trimmed = binding.trim();
    if trimmed.is_empty() {
        return Err("empty binding string".to_string());
    }
    let mut keys = Vec::new();
    for part in trimmed.split('+') {
        let token = part.trim();
        if token.is_empty() {
            return Err(format!("invalid chord `{binding}`: empty key token"));
        }
        let Some(&canonical) = KNOWN_INPUT_KEYS
            .iter()
            .find(|k| k.eq_ignore_ascii_case(token))
        else {
            return Err(format!("unknown key `{token}` in binding `{binding}`"));
        };
        if !keys.iter().any(|k| k == canonical) {
            keys.push(canonical.to_string());
        }
    }
    Ok(keys)
}

/// Load `input.pite.toml` from `project_dir`. Missing file returns an empty [`InputMapFile`].
pub fn load_input_map(project_dir: &Path) -> Result<InputMapFile> {
    let path = project_dir.join(INPUT_MAP_FILE);
    if !path.exists() {
        return Ok(InputMapFile::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|source| ProjectError::Io {
        path: path.clone(),
        source,
    })?;
    let map: InputMapFile = toml::from_str(&text).map_err(|source| ProjectError::TomlParse {
        path: path.clone(),
        source,
    })?;
    if map.format_version != INPUT_MAP_FORMAT_VERSION {
        return Err(ProjectError::Validation(format!(
            "{}: unsupported format_version {} (expected {})",
            path.display(),
            map.format_version,
            INPUT_MAP_FORMAT_VERSION
        )));
    }
    Ok(map)
}

/// Validate action names and chord bindings in `map`, returning human-readable errors.
pub fn validate_input_map(map: &InputMapFile) -> Vec<String> {
    let mut errors = Vec::new();
    for (action, bindings) in &map.actions {
        if action.trim().is_empty() {
            errors.push("action name cannot be empty".to_string());
        }
        for binding in bindings {
            if let Err(err) = parse_chord(binding) {
                errors.push(format!("action `{action}`: {err}"));
            }
        }
    }
    errors
}

/// Save `map` deterministically to `<project_dir>/input.pite.toml`.
pub fn save_input_map(project_dir: &Path, map: &InputMapFile) -> Result<()> {
    let path = project_dir.join(INPUT_MAP_FILE);
    let text = toml::to_string_pretty(map)?;
    std::fs::write(&path, text).map_err(|source| ProjectError::Io { path, source })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_input_map_loads_empty() {
        let dir = std::env::temp_dir().join(format!("pite-input-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let map = load_input_map(&dir).unwrap();
        assert_eq!(map.format_version, INPUT_MAP_FORMAT_VERSION);
        assert!(map.actions.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn input_map_round_trips_and_validates_chords() {
        let dir = std::env::temp_dir().join(format!("pite-input-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut map = InputMapFile::default();
        map.actions.insert(
            "jump".to_string(),
            vec!["Space".to_string(), "W".to_string()],
        );
        map.actions
            .insert("sprint_jump".to_string(), vec!["Shift+Space".to_string()]);
        assert!(validate_input_map(&map).is_empty());

        save_input_map(&dir, &map).unwrap();
        let loaded = load_input_map(&dir).unwrap();
        assert_eq!(loaded, map);

        assert_eq!(
            parse_chord("control + s").unwrap(),
            vec!["Control".to_string(), "S".to_string()]
        );
        assert!(parse_chord("BogusKey").is_err());
        assert!(parse_chord("Space+").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
