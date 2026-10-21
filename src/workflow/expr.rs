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

/// Evaluates a YAML value, keeping the type when the whole value is a single expression.
pub fn evaluate_yaml(v: &Yaml, ctx: &Ctx) -> Result<Value, String> {
    match v {
        Yaml::String(s) => match whole_expression(s) {
            Some(inner) => evaluate(inner, ctx),
            None => interpolate(s, ctx).map(Value::String),
        },
        other => serde_json::to_value(other).map_err(|e| e.to_string()),
    }
}

/// Evaluates an `if:` condition, applying GitHub's implicit `success() &&` prefix.
pub fn condition(cond: Option<&Yaml>, ctx: &Ctx) -> Result<bool, String> {
    let src = match cond {
        None | Some(Yaml::Null) => return Ok(ctx.state.success()),
        Some(Yaml::Bool(b)) => return Ok(*b),
        Some(Yaml::Number(n)) => return Ok(n.as_f64().is_some_and(|f| f != 0.0)),
        Some(Yaml::String(s)) => s.trim(),
        Some(_) => return Err("`if` must be a string or boolean".into()),
    };
    let src = whole_expression(src).unwrap_or(src).trim();
    if src.is_empty() {
        return Ok(ctx.state.success());
    }
    let lower = src.to_ascii_lowercase();
    let has_status = ["success(", "failure(", "always(", "cancelled("]
        .iter()
        .any(|f| lower.contains(f));
    let value = truthy(&evaluate(src, ctx)?);
    Ok(if has_status {
        value
    } else {
        value && ctx.state.success()
    })
}

/// If `s` is exactly one `${{ ... }}`, returns the inner expression.
fn whole_expression(s: &str) -> Option<&str> {
    let t = s.trim();
    let inner = t.strip_prefix("${{")?;
    let end = find_close(inner)?;
    (end + 2 == inner.len()).then(|| &inner[..end])
}

fn find_close(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => in_str = !in_str,
            b'}' if !in_str && bytes.get(i + 1) == Some(&b'}') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Lexer

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
    LBrack,
    RBrack,
    Dot,
    Comma,
    Star,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            c if c.is_whitespace() => i += 1,
            '(' | ')' | '[' | ']' | ',' | '*' | '.' => {
                out.push(match c {
                    '(' => Tok::LParen,
                    ')' => Tok::RParen,
                    '[' => Tok::LBrack,
                    ']' => Tok::RBrack,
                    ',' => Tok::Comma,
                    '*' => Tok::Star,
                    _ => Tok::Dot,
                });
                i += 1;
            }
            '\'' => {
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err("unterminated string literal".into()),
                        Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                            s.push('\'');
                            i += 2;
                        }
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(&ch) => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Str(s));
            }
            '=' | '!' | '<' | '>' | '&' | '|' => {
                let op = match (c, next) {
                    ('=', Some('=')) => "==",
                    ('!', Some('=')) => "!=",
                    ('<', Some('=')) => "<=",
                    ('>', Some('=')) => ">=",
                    ('&', Some('&')) => "&&",
                    ('|', Some('|')) => "||",
                    ('!', _) => "!",
                    ('<', _) => "<",
                    ('>', _) => ">",
                    _ => return Err(format!("unexpected operator `{c}`")),
                };
                i += op.len();
                out.push(Tok::Op(op));
            }
            c if c.is_ascii_digit() || (c == '-' && next.is_some_and(|n| n.is_ascii_digit())) => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let num = match text.strip_prefix("0x") {
                    Some(hex) => i64::from_str_radix(hex, 16).map(|n| n as f64).ok(),
                    None => text.parse::<f64>().ok(),
                };
                out.push(Tok::Num(
                    num.ok_or_else(|| format!("invalid number `{text}`"))?,
                ));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                {
                    i += 1;
                }
                out.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(format!("unexpected character `{other}`")),
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Parser

#[derive(Debug)]
enum Expr {
    Lit(Value),
    Context(String),
    Prop(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Star(Box<Expr>),
    Call(String, Vec<Expr>),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cmp(&'static str, Box<Expr>, Box<Expr>),
}
