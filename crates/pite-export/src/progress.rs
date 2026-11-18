// SPDX-License-Identifier: MIT OR Apache-2.0
//! Progress reporting seam for export and toolchain operations.
//!
//! The engine core never styles terminal output: the CLI supplies a [`Progress`]
//! that renders through its own output layer (colors, quiet mode, JSON). Library
//! callers and tests that do not care about progress pass `&mut ()`.

/// User-facing progress events emitted while downloading, verifying, and
/// staging export artifacts.
///
/// Every method has a no-op default so a sink only implements what it renders.
pub trait Progress {
    /// A note about work in progress (downloads, cache misses).
    fn info(&mut self, _message: &str) {}
    /// A discrete step inside a longer operation.
    fn step(&mut self, _message: &str) {}
    /// A step completed successfully.
    fn success(&mut self, _message: &str) {}
    /// A recoverable problem worth surfacing.
    fn warn(&mut self, _message: &str) {}
    /// Start a titled block; later events belong to it.
    fn section(&mut self, _title: &str) {}
    /// Show `message` as work in progress. On a terminal the line is later
    /// rewritten in place by [`Progress::complete`]; when output is piped it
    /// stays a normal line.
    fn pending(&mut self, _message: &str) {}
    /// Finish the pending line, replacing it with the completed phrasing.
    fn complete(&mut self, message: &str) {
        self.success(message);
    }
    /// Extra per-item detail, shown only in verbose output.
    fn detail(&mut self, _message: &str) {}
}

/// Silent sink for callers that do not render progress.
impl Progress for () {}
