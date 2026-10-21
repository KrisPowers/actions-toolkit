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

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: Tok) -> Result<(), String> {
        if self.eat(&t) {
            Ok(())
        } else {
            Err(format!("expected {t:?}"))
        }
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.eat(&Tok::Op("||")) {
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.equality()?;
        while self.eat(&Tok::Op("&&")) {
            left = Expr::And(Box::new(left), Box::new(self.equality()?));
        }
        Ok(left)
    }

    fn equality(&mut self) -> Result<Expr, String> {
        let mut left = self.comparison()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(op @ ("==" | "!="))) => *op,
                _ => return Ok(left),
            };
            self.pos += 1;
            left = Expr::Cmp(op, Box::new(left), Box::new(self.comparison()?));
        }
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(op @ ("<" | "<=" | ">" | ">="))) => *op,
                _ => return Ok(left),
            };
            self.pos += 1;
            left = Expr::Cmp(op, Box::new(left), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat(&Tok::Op("!")) {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&Tok::Dot) {
                if self.eat(&Tok::Star) {
                    e = Expr::Star(Box::new(e));
                    continue;
                }
                match self.toks.get(self.pos).cloned() {
                    Some(Tok::Ident(name)) => {
                        self.pos += 1;
                        e = Expr::Prop(Box::new(e), name);
                    }
                    _ => return Err("expected a property name after `.`".into()),
                }
            } else if self.eat(&Tok::LBrack) {
                if self.eat(&Tok::Star) {
                    self.expect(Tok::RBrack)?;
                    e = Expr::Star(Box::new(e));
                } else {
                    let idx = self.or()?;
                    self.expect(Tok::RBrack)?;
                    e = Expr::Index(Box::new(e), Box::new(idx));
                }
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let tok = self
            .toks
            .get(self.pos)
            .cloned()
            .ok_or("unexpected end of expression")?;
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Lit(num(n))),
            Tok::Str(s) => Ok(Expr::Lit(Value::String(s))),
            Tok::LParen => {
                let e = self.or()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Tok::Ident(name) => match name.as_str() {
                "true" => Ok(Expr::Lit(Value::Bool(true))),
                "false" => Ok(Expr::Lit(Value::Bool(false))),
                "null" => Ok(Expr::Lit(Value::Null)),
                _ if self.eat(&Tok::LParen) => {
                    let mut args = Vec::new();
                    if !self.eat(&Tok::RParen) {
                        loop {
                            args.push(self.or()?);
                            if self.eat(&Tok::RParen) {
                                break;
                            }
                            self.expect(Tok::Comma)?;
                        }
                    }
                    Ok(Expr::Call(name, args))
                }
                _ => Ok(Expr::Context(name)),
            },
            other => Err(format!("unexpected token {other:?}")),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Evaluation

fn eval(e: &Expr, ctx: &Ctx) -> Result<Value, String> {
    Ok(match e {
        Expr::Lit(v) => v.clone(),
        Expr::Context(name) => prop(ctx.data, name),
        Expr::Prop(inner, name) => prop(&eval(inner, ctx)?, name),
        Expr::Index(inner, idx) => {
            let base = eval(inner, ctx)?;
            match (&base, eval(idx, ctx)?) {
                (Value::Array(items), Value::Number(n)) => n
                    .as_f64()
                    .and_then(|f| items.get(f as usize).cloned())
                    .unwrap_or(Value::Null),
                (_, Value::String(key)) => prop(&base, &key),
                _ => Value::Null,
            }
        }
        Expr::Star(inner) => match eval(inner, ctx)? {
            Value::Array(items) => Value::Array(items),
            Value::Object(map) => Value::Array(map.into_iter().map(|(_, v)| v).collect()),
            _ => Value::Array(Vec::new()),
        },
        Expr::Not(inner) => Value::Bool(!truthy(&eval(inner, ctx)?)),
        Expr::And(a, b) => {
            let left = eval(a, ctx)?;
            if truthy(&left) {
                eval(b, ctx)?
            } else {
                left
            }
        }
        Expr::Or(a, b) => {
            let left = eval(a, ctx)?;
            if truthy(&left) {
                left
            } else {
                eval(b, ctx)?
            }
        }
        Expr::Cmp(op, a, b) => {
            let (l, r) = (eval(a, ctx)?, eval(b, ctx)?);
            Value::Bool(match *op {
                "==" => loose_eq(&l, &r),
                "!=" => !loose_eq(&l, &r),
                "<" => compare(&l, &r) == Some(Ordering::Less),
                "<=" => matches!(compare(&l, &r), Some(Ordering::Less | Ordering::Equal)),
                ">" => compare(&l, &r) == Some(Ordering::Greater),
                ">=" => matches!(compare(&l, &r), Some(Ordering::Greater | Ordering::Equal)),
                _ => unreachable!(),
            })
        }
        Expr::Call(name, args) => call(name, args, ctx)?,
    })
}

fn call(name: &str, args: &[Expr], ctx: &Ctx) -> Result<Value, String> {
    let lname = name.to_ascii_lowercase();
    match lname.as_str() {
        "success" => return Ok(Value::Bool(ctx.state.success())),
        "failure" => return Ok(Value::Bool(ctx.state.failed)),
        "always" => return Ok(Value::Bool(true)),
        "cancelled" => return Ok(Value::Bool(ctx.state.cancelled)),
        _ => {}
    }
    let vals = args
        .iter()
        .map(|a| eval(a, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    let arg = |i: usize| vals.get(i).cloned().unwrap_or(Value::Null);
    let need = |n: usize| {
        if vals.len() < n {
            Err(format!("{name}() expects {n} argument(s)"))
        } else {
            Ok(())
        }
    };
    Ok(match lname.as_str() {
        "contains" => {
            need(2)?;
            match arg(0) {
                Value::Array(items) => Value::Bool(items.iter().any(|i| loose_eq(i, &arg(1)))),
                other => Value::Bool(
                    to_str(&other)
                        .to_lowercase()
                        .contains(&to_str(&arg(1)).to_lowercase()),
                ),
            }
        }
        "startswith" => {
            need(2)?;
            Value::Bool(
                to_str(&arg(0))
                    .to_lowercase()
                    .starts_with(&to_str(&arg(1)).to_lowercase()),
            )
        }
        "endswith" => {
            need(2)?;
            Value::Bool(
                to_str(&arg(0))
                    .to_lowercase()
                    .ends_with(&to_str(&arg(1)).to_lowercase()),
            )
        }
        "format" => {
            need(1)?;
            Value::String(format_fn(&to_str(&arg(0)), &vals[1..])?)
        }
        "join" => {
            need(1)?;
            let sep = if vals.len() > 1 {
                to_str(&arg(1))
            } else {
                ",".into()
            };
            match arg(0) {
                Value::Array(items) => {
                    Value::String(items.iter().map(to_str).collect::<Vec<_>>().join(&sep))
                }
                other => Value::String(to_str(&other)),
            }
        }
        "tojson" => Value::String(serde_json::to_string_pretty(&arg(0)).unwrap_or_default()),
        "fromjson" => {
            need(1)?;
            serde_json::from_str(&to_str(&arg(0))).map_err(|e| format!("fromJSON: {e}"))?
        }
        // File hashing is only used for cache keys, and caching is not supported locally.
        "hashfiles" => Value::String(String::new()),
        _ => return Err(format!("unknown function `{name}`")),
    })
}

fn format_fn(fmt: &str, args: &[Value]) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let mut idx = String::new();
                for d in chars.by_ref() {
                    if d == '}' {
                        break;
                    }
                    idx.push(d);
                }
                let i: usize = idx
                    .parse()
                    .map_err(|_| format!("format: invalid placeholder {{{idx}}}"))?;
                out.push_str(&to_str(args.get(i).ok_or("format: not enough arguments")?));
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

fn prop(v: &Value, name: &str) -> Value {
    match v {
        Value::Object(map) => map
            .get(name)
            .or_else(|| {
                map.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v)
            })
            .cloned()
            .unwrap_or(Value::Null),
        // Property access on an object filter (`a.*.b`) maps over the elements.
        Value::Array(items) => Value::Array(items.iter().map(|i| prop(i, name)).collect()),
        _ => Value::Null,
    }
}

fn num(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        Value::Number(Number::from(f as i64))
    } else {
        Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}
