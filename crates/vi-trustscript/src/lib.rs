//! TrustScript: a tiny, declarative rule language for abuse-detection *leads*.
//!
//!   when case.plea_sentence_ratio < 0.5
//!        and case.evidence_strength == "weak"
//!   then flag "Plea coercion suspected" severity high
//!
//! Design contract:
//!  * No arithmetic in rules. Derived features (ratios, z-scores) are computed
//!    in Rust, visible in code review, and injected into the case context.
//!  * Every flag carries the exact conditions that matched → human-reviewable.
//!  * Flags are leads for the Evidence Review Committee, never auto-published.
#![forbid(unsafe_code)]

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
#[error("parse error at token {pos}: {msg}")]
pub struct ParseError {
    pub pos: usize,
    pub msg: String,
}

impl ParseError {
    fn at(pos: usize, msg: impl Into<String>) -> Self {
        Self {
            pos,
            msg: msg.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Str(String),
    Num(f64),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    fn sym(self) -> &'static str {
        match self {
            Cmp::Eq => "==",
            Cmp::Ne => "!=",
            Cmp::Lt => "<",
            Cmp::Le => "<=",
            Cmp::Gt => ">",
            Cmp::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Cond {
    Cmp { path: String, op: Cmp, lit: Lit },
    Contains { path: String, needle: String },
    In { path: String, set: Vec<Lit> },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: Option<String>,
    pub conditions: Vec<Cond>,
    pub flag: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize)]
pub struct Flag {
    pub label: String,
    pub severity: Severity,
    /// One human-readable line per matched condition — the audit trail.
    pub matched: Vec<String>,
}

// ---------------- lexer ----------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    Str(String),
    Op(Cmp),
    LParen,
    RParen,
    Comma,
}

fn lex(src: &str) -> Result<Vec<Tok>, ParseError> {
    let cs: Vec<char> = src.chars().collect();
    let (mut i, mut out) = (0, Vec::new());
    while i < cs.len() {
        let c = cs[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '=' => {
                if cs.get(i + 1) == Some(&'=') {
                    out.push(Tok::Op(Cmp::Eq));
                    i += 2;
                } else {
                    return Err(ParseError::at(i, "use '==', not '='"));
                }
            }
            '!' => {
                if cs.get(i + 1) == Some(&'=') {
                    out.push(Tok::Op(Cmp::Ne));
                    i += 2;
                } else {
                    return Err(ParseError::at(i, "unexpected '!'"));
                }
            }
            '>' => {
                if cs.get(i + 1) == Some(&'=') {
                    out.push(Tok::Op(Cmp::Ge));
                    i += 2;
                } else {
                    out.push(Tok::Op(Cmp::Gt));
                    i += 1;
                }
            }
            '<' => {
                if cs.get(i + 1) == Some(&'=') {
                    out.push(Tok::Op(Cmp::Le));
                    i += 2;
                } else {
                    out.push(Tok::Op(Cmp::Lt));
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                let start = i;
                while i < cs.len() && cs[i] != '"' {
                    i += 1;
                }
                if i >= cs.len() {
                    return Err(ParseError::at(start, "unterminated string"));
                }
                out.push(Tok::Str(cs[start..i].iter().collect()));
                i += 1;
            }
            c if c.is_ascii_digit()
                || (c == '-' && cs.get(i + 1).is_some_and(|n| n.is_ascii_digit())) =>
            {
                let start = i;
                if c == '-' {
                    i += 1;
                }
                while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                    i += 1;
                }
                let text: String = cs[start..i].iter().collect();
                let n = text
                    .parse::<f64>()
                    .map_err(|_| ParseError::at(start, format!("bad number '{text}'")))?;
                out.push(Tok::Num(n));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < cs.len() && (cs[i].is_alphanumeric() || matches!(cs[i], '_' | '.' | '-'))
                {
                    i += 1;
                }
                out.push(Tok::Ident(cs[start..i].iter().collect()));
            }
            other => return Err(ParseError::at(i, format!("unexpected '{other}'"))),
        }
    }
    Ok(out)
}

// ---------------- parser ----------------

pub fn parse_rule(src: &str) -> Result<Rule, ParseError> {
    struct P {
        toks: Vec<Tok>,
        i: usize,
    }
    impl P {
        fn peek(&self) -> Option<&Tok> {
            self.toks.get(self.i)
        }
        fn next(&mut self) -> Option<Tok> {
            let t = self.toks.get(self.i).cloned();
            if t.is_some() {
                self.i += 1;
            }
            t
        }
        fn at_kw(&self, kw: &str) -> bool {
            matches!(self.peek(), Some(Tok::Ident(k)) if k == kw)
        }
        fn expect_kw(&mut self, kw: &str) -> Result<(), ParseError> {
            if self.at_kw(kw) {
                self.i += 1;
                Ok(())
            } else {
                Err(ParseError::at(self.i, format!("expected '{kw}'")))
            }
        }
        fn ident(&mut self) -> Result<String, ParseError> {
            match self.next() {
                Some(Tok::Ident(s)) => Ok(s),
                other => Err(ParseError::at(
                    self.i,
                    format!("expected identifier, got {other:?}"),
                )),
            }
        }
        fn string(&mut self) -> Result<String, ParseError> {
            match self.next() {
                Some(Tok::Str(s)) => Ok(s),
                other => Err(ParseError::at(
                    self.i,
                    format!("expected string, got {other:?}"),
                )),
            }
        }
        fn literal(&mut self) -> Result<Lit, ParseError> {
            match self.next() {
                Some(Tok::Str(s)) => Ok(Lit::Str(s)),
                Some(Tok::Num(n)) => Ok(Lit::Num(n)),
                Some(Tok::Ident(k)) if k == "true" => Ok(Lit::Bool(true)),
                Some(Tok::Ident(k)) if k == "false" => Ok(Lit::Bool(false)),
                other => Err(ParseError::at(
                    self.i,
                    format!("expected literal, got {other:?}"),
                )),
            }
        }
        fn cond(&mut self) -> Result<Cond, ParseError> {
            let path = self.ident()?;
            match self.peek().cloned() {
                Some(Tok::Op(op)) => {
                    self.i += 1;
                    Ok(Cond::Cmp {
                        path,
                        op,
                        lit: self.literal()?,
                    })
                }
                Some(Tok::Ident(k)) if k == "contains" => {
                    self.i += 1;
                    Ok(Cond::Contains {
                        path,
                        needle: self.string()?,
                    })
                }
                Some(Tok::Ident(k)) if k == "in" => {
                    self.i += 1;
                    match self.next() {
                        Some(Tok::LParen) => {}
                        other => {
                            return Err(ParseError::at(
                                self.i,
                                format!("'in' needs '(' got {other:?}"),
                            ))
                        }
                    }
                    let mut set = vec![self.literal()?];
                    while matches!(self.peek(), Some(Tok::Comma)) {
                        self.i += 1;
                        set.push(self.literal()?);
                    }
                    match self.next() {
                        Some(Tok::RParen) => Ok(Cond::In { path, set }),
                        other => Err(ParseError::at(
                            self.i,
                            format!("expected ')', got {other:?}"),
                        )),
                    }
                }
                other => Err(ParseError::at(
                    self.i,
                    format!("expected operator, 'contains', or 'in'; got {other:?}"),
                )),
            }
        }
    }

    let mut p = P {
        toks: lex(src)?,
        i: 0,
    };
    p.expect_kw("when")?;
    let mut conditions = vec![p.cond()?];
    while p.at_kw("and") {
        p.i += 1;
        conditions.push(p.cond()?);
    }
    p.expect_kw("then")?;
    p.expect_kw("flag")?;
    let flag = p.string()?;
    let mut severity = Severity::Medium;
    if p.at_kw("severity") {
        p.i += 1;
        severity = match p.ident()?.as_str() {
            "low" => Severity::Low,
            "medium" => Severity::Medium,
            "high" => Severity::High,
            "critical" => Severity::Critical,
            other => return Err(ParseError::at(p.i, format!("unknown severity '{other}'"))),
        };
    }
    if p.peek().is_some() {
        return Err(ParseError::at(p.i, "trailing tokens after rule"));
    }
    Ok(Rule {
        name: None,
        conditions,
        flag,
        severity,
    })
}

// ---------------- evaluator ----------------

pub fn resolve<'a>(ctx: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(ctx, |v, key| v.get(key))
}

fn cmp_lit(op: Cmp, lit: &Lit, v: &Value) -> bool {
    match (lit, v) {
        (Lit::Num(a), Value::Number(n)) => n.as_f64().is_some_and(|b| match op {
            Cmp::Eq => *a == b,
            Cmp::Ne => *a != b,
            Cmp::Lt => b < *a,
            Cmp::Le => b <= *a,
            Cmp::Gt => b > *a,
            Cmp::Ge => b >= *a,
        }),
        (Lit::Str(a), Value::String(b)) => match op {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            _ => false,
        },
        (Lit::Bool(a), Value::Bool(b)) => match op {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            _ => false,
        },
        _ => false, // missing field or type mismatch → condition fails closed
    }
}

fn lit_eq(lit: &Lit, v: &Value) -> bool {
    cmp_lit(Cmp::Eq, lit, v)
}

/// Returns Some(Flag) iff ALL conditions match. Fail-closed by construction.
pub fn evaluate(rule: &Rule, ctx: &Value) -> Option<Flag> {
    let mut matched = Vec::with_capacity(rule.conditions.len());
    for c in &rule.conditions {
        let line = match c {
            Cond::Cmp { path, op, lit } => {
                let v = resolve(ctx, path)?;
                cmp_lit(*op, lit, v)
                    .then(|| format!("{path} {} {lit:?}  (actual: {v})", op.sym()))?
            }
            Cond::Contains { path, needle } => {
                let v = resolve(ctx, path)?;
                v.as_str()
                    .filter(|s| s.contains(needle.as_str()))
                    .map(|_| format!("{path} contains {needle:?}  (actual: {v})"))?
            }
            Cond::In { path, set } => {
                let v = resolve(ctx, path)?;
                set.iter()
                    .any(|l| lit_eq(l, v))
                    .then(|| format!("{path} in ({set:?})  (actual: {v})"))?
            }
        };
        matched.push(line);
    }
    Some(Flag {
        label: rule.flag.clone(),
        severity: rule.severity,
        matched,
    })
}

pub fn evaluate_all(rules: &[Rule], ctx: &Value) -> Vec<Flag> {
    rules.iter().filter_map(|r| evaluate(r, ctx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COERCION: &str = r#"when case.plea_sentence_ratio < 0.5 and case.evidence_strength == "weak" then flag "Plea coercion suspected" severity high"#;

    fn ctx(ratio: f64) -> Value {
        json!({"case": {
            "plea_sentence_ratio": ratio,
            "evidence_strength": "weak",
            "outcome": "conviction",
            "office": "Demo County DA",
            "judge": "Smith J."
        }})
    }

    #[test]
    fn parses_and_fires() {
        let r = parse_rule(COERCION).unwrap();
        assert_eq!(r.conditions.len(), 2);
        let f = evaluate(&r, &ctx(0.33)).unwrap();
        assert_eq!(f.label, "Plea coercion suspected");
        assert_eq!(f.severity, Severity::High);
        assert_eq!(f.matched.len(), 2);
    }

    #[test]
    fn does_not_fire_when_ratio_high() {
        let r = parse_rule(COERCION).unwrap();
        assert!(evaluate(&r, &ctx(0.9)).is_none());
    }

    #[test]
    fn missing_field_fails_closed() {
        let r = parse_rule(COERCION).unwrap();
        assert!(evaluate(&r, &json!({"case": {}})).is_none());
    }

    #[test]
    fn contains_and_in() {
        let r = parse_rule(
            r#"when case.judge contains "Smith" and case.office in ("Demo County DA", "Other DA") then flag "x""#,
        )
        .unwrap();
        assert!(evaluate(&r, &ctx(0.9)).is_some());
    }

    #[test]
    fn parse_errors_are_loud() {
        assert!(parse_rule("when case.x = 1 then flag \"y\"").is_err());
        assert!(parse_rule("when case.x == 1 flag \"y\"").is_err());
    }

    #[test]
    fn ne_and_numeric_compare() {
        let r = parse_rule(r#"when case.plea_sentence_ratio != 0.5 then flag "y" severity low"#)
            .unwrap();
        assert!(evaluate(&r, &ctx(0.33)).is_some());
        assert!(evaluate(&r, &json!({"case": {"plea_sentence_ratio": 0.5}})).is_none());
    }
}
