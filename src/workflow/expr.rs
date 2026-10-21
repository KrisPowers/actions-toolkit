//! Evaluator for GitHub Actions `${{ }}` expressions.
//!
//! Supports literals, context access (`a.b`, `a['b']`, `a.*.b`), the `!`, `&&`, `||`,
//! comparison operators, and the standard functions (`contains`, `startsWith`, `format`,
//! `toJSON`, `fromJSON`, `success()`, `always()`, ...).

use std::cmp::Ordering;

use serde_json::{Map, Number, Value};
use serde_yaml::Value as Yaml;

/// Status of the surrounding job, consulted by `success()`, `failure()` and `cancelled()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct JobState {
    pub failed: bool,
    pub cancelled: bool,
    /// An upstream job didn't succeed (e.g. was skipped) without failing.
    pub blocked: bool,
}
