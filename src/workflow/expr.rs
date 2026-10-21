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

impl JobState {
    pub fn success(&self) -> bool {
        !self.failed && !self.cancelled && !self.blocked
    }
}

pub struct Ctx<'a> {
    pub data: &'a Value,
    pub state: JobState,
}

impl<'a> Ctx<'a> {
    pub fn new(data: &'a Value, state: JobState) -> Self {
        Self { data, state }
    }
}

/// Evaluates a bare expression (without the `${{ }}` wrapper).
pub fn evaluate(src: &str, ctx: &Ctx) -> Result<Value, String> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let expr = p.or()?;
    if p.pos != p.toks.len() {
        return Err(format!("unexpected token in expression `{src}`"));
    }
    eval(&expr, ctx)
}
