// SPDX-License-Identifier: MIT OR Apache-2.0
//! The JSON envelope and the exit-status contract shared by every command.
//!
//! Commands return an [`Outcome`] with an optional serializable payload; `main`
//! owns process exit and serialization, so no command calls `process::exit`.

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use crate::ui::Ui;

/// Version of the `{schema_version, command, ok, data, error}` envelope.
///
/// Payload structs carry their own `schema_version` (currently 1): the envelope
/// version changes only if the envelope shape changes, while a command's payload
/// version changes when that command's data changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Machine-readable envelope printed by `main` when `--json` is set.
#[derive(Debug, Serialize)]
pub struct Envelope {
    pub schema_version: u32,
    pub command: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

/// A failed command: the top-level message plus its `anyhow` causes.
#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub message: String,
    pub chain: Vec<String>,
}

impl ErrorPayload {
    pub fn from_error(error: &anyhow::Error) -> Self {
        Self {
            message: error.to_string(),
            chain: error
                .chain()
                .skip(1)
                .map(|cause| cause.to_string())
                .collect(),
        }
    }
}

/// What a command hands back to `main`.
#[derive(Debug)]
pub struct Outcome {
    /// Process exit status: 0 for success, 1 for a failed check/verification.
    pub exit: u8,
    /// Serializable payload for `--json`; `None` for commands without data.
    pub data: Option<Value>,
}

impl Outcome {
    /// A successful command with a serializable payload.
    pub fn completed<T: Serialize>(data: T) -> Result<Self> {
        Ok(Self {
            exit: 0,
            data: Some(serde_json::to_value(data)?),
        })
    }

    /// A command that finished but reports failure (e.g. strict check).
    pub fn failed<T: Serialize>(data: T) -> Result<Self> {
        Ok(Self {
            exit: 1,
            data: Some(serde_json::to_value(data)?),
        })
    }

    /// A successful command with nothing to report in JSON.
    pub fn silent() -> Self {
        Self {
            exit: 0,
            data: None,
        }
    }
}

/// Build the JSON envelope for a completed command.
pub fn envelope(command: &str, outcome: &Outcome) -> Result<String> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        command: command.to_string(),
        ok: outcome.exit == 0,
        data: outcome.data.clone(),
        error: None,
    };
    Ok(serde_json::to_string_pretty(&envelope)?)
}

/// Build the JSON envelope for a command that returned an error.
pub fn error_envelope(command: &str, error: &anyhow::Error) -> Result<String> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        command: command.to_string(),
        ok: false,
        data: None,
        error: Some(ErrorPayload::from_error(error)),
    };
    Ok(serde_json::to_string_pretty(&envelope)?)
}

/// Build the JSON envelope for a usage problem that never reaches a command.
pub fn usage_error_envelope(command: &str, message: &str) -> String {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        command: command.to_string(),
        ok: false,
        data: None,
        error: Some(ErrorPayload {
            message: message.to_string(),
            chain: Vec::new(),
        }),
    };
    serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
}

/// Human error rendering: colored label plus any `anyhow` causes.
pub fn print_error(ui: &mut Ui, error: &anyhow::Error) {
    ui.error(error.to_string());
    for cause in error.chain().skip(1) {
        ui.detail("Caused by", cause.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Outcome {
        Outcome {
            exit: 0,
            data: Some(serde_json::json!({"schema_version": 1, "value": 7})),
        }
    }

    #[test]
    fn success_envelope_has_data_and_no_error() {
        let rendered = envelope("vendor", &sample()).unwrap();
        let value: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["command"], "vendor");
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"]["value"], 7);
        assert!(value.get("error").is_none());
    }

    #[test]
    fn failed_outcome_sets_ok_false_but_keeps_data() {
        let outcome = Outcome {
            exit: 1,
            data: Some(serde_json::json!({"schema_version": 1, "diagnostics": []})),
        };
        let rendered = envelope("check", &outcome).unwrap();
        let value: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value.get("data").is_some());
    }

    #[test]
    fn error_envelope_carries_the_chain() {
        let error = anyhow::anyhow!("outer").context("inner");
        let rendered = error_envelope("export", &error).unwrap();
        let value: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("inner"));
        assert!(!value["error"]["chain"].as_array().unwrap().is_empty());
    }

    #[test]
    fn usage_error_has_no_data() {
        let rendered = usage_error_envelope("pite", "no command given");
        let value: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], false);
        assert!(value.get("data").is_none());
    }

    #[test]
    fn success_envelope_snapshot() {
        insta::assert_snapshot!("envelope_success", envelope("vendor", &sample()).unwrap());
    }

    #[test]
    fn error_envelope_snapshot() {
        let error = anyhow::anyhow!("outer").context("inner");
        insta::assert_snapshot!("envelope_error", error_envelope("export", &error).unwrap());
    }
}
