use std::collections::HashMap;

/// A deliberately small evaluator for the subset of GitHub Actions' `${{ ... }}` expression
/// syntax this toolkit supports: `==`, `!=`, `&&`, `||`, `contains(a, b)`, `always()`,
/// `success()`, `failure()`, and dotted context lookups like `github.event_name` or
/// `needs.build.result`. This is not a general expression language; unsupported syntax
/// should be flagged clearly in the visual builder's rule UI rather than silently mis-evaluated.
pub struct ExprContext {
    pub values: HashMap<String, String>,
    pub job_results: HashMap<String, String>,
    pub any_failed_dependency: bool,
}

impl ExprContext {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            job_results: HashMap::new(),
            any_failed_dependency: false,
        }
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.values.insert(key.to_string(), value.into());
    }
}

impl Default for ExprContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate an `if:` condition string. Bare `${{ }}` wrapper is optional and stripped if
/// present. Returns `true` if the step/job should run.
pub fn evaluate(expr: &str, ctx: &ExprContext) -> bool {
    let expr = strip_wrapper(expr.trim());

    if expr.is_empty() {
        // No condition means "run on success" (default GHA behavior), unless a
        // dependency failed, matching implicit `success()`.
        return !ctx.any_failed_dependency;
    }

    if expr == "always()" {
        return true;
    }
    if expr == "failure()" {
        return ctx.any_failed_dependency;
    }
    if expr == "success()" {
        return !ctx.any_failed_dependency;
    }

    if let Some((lhs, rhs)) = split_once_top_level(expr, "&&") {
        return evaluate(lhs, ctx) && evaluate(rhs, ctx);
    }
    if let Some((lhs, rhs)) = split_once_top_level(expr, "||") {
        return evaluate(lhs, ctx) || evaluate(rhs, ctx);
    }

    if let Some(inner) = expr.strip_prefix("contains(").and_then(|s| s.strip_suffix(')')) {
        let mut parts = inner.splitn(2, ',');
        let haystack = parts.next().unwrap_or_default().trim();
        let needle = parts.next().unwrap_or_default().trim();
        let haystack_val = resolve(haystack, ctx);
        let needle_val = unquote(needle);
        return haystack_val.contains(&needle_val);
    }

    if let Some((lhs, rhs)) = split_once_top_level(expr, "==") {
        return resolve(lhs.trim(), ctx) == unquote(rhs.trim());
    }
    if let Some((lhs, rhs)) = split_once_top_level(expr, "!=") {
        return resolve(lhs.trim(), ctx) != unquote(rhs.trim());
    }

    // Bare boolean-ish value/context lookup.
    let value = resolve(expr, ctx);
    !value.is_empty() && value != "false"
}

fn strip_wrapper(expr: &str) -> &str {
    expr.strip_prefix("${{")
        .and_then(|s| s.strip_suffix("}}"))
        .map(str::trim)
        .unwrap_or(expr)
}

fn split_once_top_level<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    // No parens/quotes nesting support beyond simple cases; sufficient for the supported subset.
    expr.find(op).map(|idx| (&expr[..idx], &expr[idx + op.len()..]))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn resolve(token: &str, ctx: &ExprContext) -> String {
    let token = token.trim();
    if let Some(job) = token.strip_prefix("needs.").and_then(|s| s.strip_suffix(".result")) {
        return ctx.job_results.get(job).cloned().unwrap_or_default();
    }
    if let Some(v) = ctx.values.get(token) {
        return v.clone();
    }
    unquote(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_and_context() {
        let mut ctx = ExprContext::new();
        ctx.set("github.event_name", "push");
        assert!(evaluate("${{ github.event_name == 'push' }}", &ctx));
        assert!(!evaluate("${{ github.event_name == 'pull_request' }}", &ctx));
    }

    #[test]
    fn needs_result() {
        let mut ctx = ExprContext::new();
        ctx.job_results.insert("build".to_string(), "success".to_string());
        assert!(evaluate("needs.build.result == 'success'", &ctx));
    }

    #[test]
    fn default_and_always() {
        let mut ctx = ExprContext::new();
        ctx.any_failed_dependency = true;
        assert!(!evaluate("", &ctx));
        assert!(evaluate("always()", &ctx));
    }
}
