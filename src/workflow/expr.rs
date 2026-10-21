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

/// Replaces every `${{ expr }}` in `s` with its string value.
pub fn interpolate(s: &str, ctx: &Ctx) -> Result<String, String> {
    if !s.contains("${{") {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("${{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let end = find_close(after).ok_or_else(|| format!("unterminated expression in `{s}`"))?;
        out.push_str(&to_str(&evaluate(&after[..end], ctx)?));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Interpolates a YAML scalar into a string.
pub fn interpolate_yaml(v: &Yaml, ctx: &Ctx) -> Result<String, String> {
    interpolate(&crate::workflow::model::scalar(v), ctx)
}
