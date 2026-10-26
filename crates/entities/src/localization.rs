//! Translations of the user interface. The English text is the key (as in gettext), so anything
//! without a translation — including captions from packages nobody translated — shows in English.
//!
//! Placeholders are written `{name}` and filled with [`fill`]; [`Translations::tr_n`] also fills
//! `{count}`.

use std::collections::HashMap;
use std::fmt::Display;

/// The language of the source text.
pub const ENGLISH: &str = "en";

/// A language the interface can be shown in: `es`, `pt_BR`…, and its own name (`Español`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub code: String,
    pub name: String,
}

/// Translated messages of one language, keyed by their English text.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    messages: HashMap<String, Vec<String>>,
    plural: Option<PluralRule>,
}

impl Catalog {
    /// `plural_forms` is gettext's `Plural-Forms` header (`nplurals=2; plural=(n != 1);`).
    pub fn new(plural_forms: Option<&str>) -> Result<Self, String> {
        Ok(Self {
            messages: HashMap::new(),
            plural: plural_forms.map(PluralRule::parse).transpose()?,
        })
    }

    /// Adds a translation: one string, or one per plural form. Empty translations are skipped.
    pub fn insert(&mut self, context: Option<&str>, msgid: &str, translations: Vec<String>) {
        if msgid.is_empty() || translations.iter().all(String::is_empty) {
            return;
        }
        self.messages.insert(key(context, msgid), translations);
    }

    /// Adds `other`'s translations, replacing those already here (packages loaded later win).
    pub fn merge(&mut self, other: Catalog) {
        self.messages.extend(other.messages);
        if other.plural.is_some() {
            self.plural = other.plural;
        }
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// The English texts that have a translation.
    pub fn msgids(&self) -> impl Iterator<Item = &str> {
        self.messages.keys().map(String::as_str)
    }

    /// The translation of `msgid`: one string, or one per plural form.
    pub fn translations(&self, msgid: &str) -> Option<&[String]> {
        self.messages.get(msgid).map(Vec::as_slice)
    }

    /// How many plural forms the language has (gettext's `nplurals`).
    pub fn plural_forms(&self) -> usize {
        self.plural.as_ref().map_or(2, |rule| rule.count)
    }

    fn get(&self, msgid: &str) -> Option<&str> {
        self.messages.get(msgid).and_then(|forms| forms.first()).map(String::as_str).filter(|s| !s.is_empty())
    }

    fn get_plural(&self, msgid: &str, n: u64) -> Option<&str> {
        let forms = self.messages.get(msgid)?;
        let index = self.plural.as_ref().map_or(usize::from(n != 1), |rule| rule.index(n));
        forms.get(index).map(String::as_str).filter(|s| !s.is_empty())
    }
}

/// gettext keys messages with a context as `context\u{4}msgid`.
fn key(context: Option<&str>, msgid: &str) -> String {
    match context {
        Some(context) => format!("{context}\u{4}{msgid}"),
        None => msgid.to_string(),
    }
}

/// The interface language in use and its catalog.
#[derive(Debug, Clone)]
pub struct Translations {
    language: String,
    catalog: Catalog,
}

impl Default for Translations {
    fn default() -> Self {
        Self::new(ENGLISH, Catalog::default())
    }
}

impl Translations {
    pub fn new(language: &str, catalog: Catalog) -> Self {
        Self { language: language.to_string(), catalog }
    }

    /// The code of the language in use (`en` when nothing is translated).
    pub fn language(&self) -> &str {
        &self.language
    }

    /// The translation of `msgid`, or `msgid` itself.
    pub fn tr<'a>(&'a self, msgid: &'a str) -> &'a str {
        self.catalog.get(msgid).unwrap_or(msgid)
    }

    /// The translation of `msgid` with its `{placeholders}` filled.
    pub fn tr_args(&self, msgid: &str, args: &[(&str, &dyn Display)]) -> String {
        fill(self.tr(msgid), args)
    }

    /// The singular or plural translation for `count`, with `{count}` and `args` filled.
    pub fn tr_n(&self, singular: &str, plural: &str, count: u64, args: &[(&str, &dyn Display)]) -> String {
        let template = self
            .catalog
            .get_plural(singular, count)
            .unwrap_or(if count == 1 { singular } else { plural });
        let mut all: Vec<(&str, &dyn Display)> = vec![("count", &count)];
        all.extend_from_slice(args);
        fill(template, &all)
    }
}

/// Replaces each `{name}` in `template` with its value; unknown names are left as they are.
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after
            .find('}')
            .and_then(|close| args.iter().find(|(name, _)| *name == &after[..close]).map(|(_, v)| (close, v)));
        match value {
            Some((close, value)) => {
                out.push_str(&value.to_string());
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `pt-BR`, `pt_br.UTF-8` → `pt_BR`.
pub fn normalize_language_code(code: &str) -> String {
    let code = code.split(['.', '@']).next().unwrap_or("").replace('-', "_");
    match code.split_once('_') {
        Some((language, region)) => format!("{}_{}", language.to_lowercase(), region.to_uppercase()),
        None => code.to_lowercase(),
    }
}

/// The language to use for the `language` setting: `auto` follows the operating system.
/// A region falls back to its language (`es_CL` → `es`) and a language to any of its regions
/// (`pt` → `pt_BR`). English when nothing matches.
pub fn resolve_language(setting: &str, system: Option<&str>, available: &[Language]) -> String {
    let requested = match setting.trim() {
        "" | "auto" => system.unwrap_or(ENGLISH),
        code => code,
    };
    let requested = normalize_language_code(requested);
    let base = |code: &str| code.split('_').next().unwrap_or("").to_string();
    let codes: Vec<String> = available.iter().map(|l| normalize_language_code(&l.code)).collect();
    codes
        .iter()
        .position(|c| *c == requested)
        .or_else(|| codes.iter().position(|c| *c == base(&requested)))
        .or_else(|| codes.iter().position(|c| base(c) == base(&requested)))
        .map(|i| available[i].code.clone())
        .unwrap_or_else(|| ENGLISH.to_string())
}

/// gettext's plural formula: a C expression of `n` choosing which plural form to use.
#[derive(Debug, Clone)]
struct PluralRule {
    count: usize,
    expression: Expression,
}

impl PluralRule {
    /// `nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);`
    fn parse(header: &str) -> Result<Self, String> {
        let mut count = None;
        let mut expression = None;
        for part in header.split(';') {
            match part.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
                Some(("nplurals", value)) => count = value.parse().ok(),
                Some(("plural", value)) => expression = Some(Parser::parse(value)?),
                _ => {}
            }
        }
        match (count, expression) {
            (Some(count), Some(expression)) if count > 0 => Ok(Self { count, expression }),
            _ => Err(format!("invalid Plural-Forms: {header}")),
        }
    }

    fn index(&self, n: u64) -> usize {
        (self.expression.eval(n) as usize).min(self.count - 1)
    }
}

#[derive(Debug, Clone)]
enum Expression {
    N,
    Number(u64),
    Not(Box<Expression>),
    Binary(Box<Expression>, &'static str, Box<Expression>),
    Choice(Box<Expression>, Box<Expression>, Box<Expression>),
}

impl Expression {
    fn eval(&self, n: u64) -> u64 {
        match self {
            Expression::N => n,
            Expression::Number(value) => *value,
            Expression::Not(e) => u64::from(e.eval(n) == 0),
            Expression::Choice(condition, yes, no) => {
                if condition.eval(n) != 0 {
                    yes.eval(n)
                } else {
                    no.eval(n)
                }
            }
            Expression::Binary(a, op, b) => {
                let (a, b) = (a.eval(n), b.eval(n));
                match *op {
                    "||" => u64::from(a != 0 || b != 0),
                    "&&" => u64::from(a != 0 && b != 0),
                    "==" => u64::from(a == b),
                    "!=" => u64::from(a != b),
                    "<" => u64::from(a < b),
                    "<=" => u64::from(a <= b),
                    ">" => u64::from(a > b),
                    ">=" => u64::from(a >= b),
                    "+" => a.wrapping_add(b),
                    "-" => a.wrapping_sub(b),
                    "*" => a.wrapping_mul(b),
                    "/" => a.checked_div(b).unwrap_or(0),
                    _ => a.checked_rem(b).unwrap_or(0),
                }
            }
        }
    }
}

/// Binary operators from the loosest to the tightest.
const LEVELS: &[&[&str]] = &[&["||"], &["&&"], &["==", "!="], &["<=", ">=", "<", ">"], &["+", "-"], &["*", "/", "%"]];

struct Parser<'a> {
    rest: &'a str,
}

impl Parser<'_> {
    fn parse(text: &str) -> Result<Expression, String> {
        let mut parser = Parser { rest: text };
        let expression = parser.choice()?;
        parser.skip_spaces();
        if parser.rest.is_empty() {
            Ok(expression)
        } else {
            Err(format!("unexpected `{}` in plural formula", parser.rest))
        }
    }

    fn skip_spaces(&mut self) {
        self.rest = self.rest.trim_start();
    }

    /// Two-character operators come first in [`LEVELS`], so `<` never eats the start of `<=`.
    fn eat(&mut self, token: &str) -> bool {
        self.skip_spaces();
        match self.rest.strip_prefix(token) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn choice(&mut self) -> Result<Expression, String> {
        let condition = self.binary(0)?;
        if !self.eat("?") {
            return Ok(condition);
        }
        let yes = self.choice()?;
        if !self.eat(":") {
            return Err("missing `:` in plural formula".into());
        }
        let no = self.choice()?;
        Ok(Expression::Choice(Box::new(condition), Box::new(yes), Box::new(no)))
    }

    fn binary(&mut self, level: usize) -> Result<Expression, String> {
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        'outer: loop {
            for op in LEVELS[level] {
                if self.eat(op) {
                    let right = self.binary(level + 1)?;
                    left = Expression::Binary(Box::new(left), op, Box::new(right));
                    continue 'outer;
                }
            }
            return Ok(left);
        }
    }

    fn unary(&mut self) -> Result<Expression, String> {
        if self.eat("!") {
            return Ok(Expression::Not(Box::new(self.unary()?)));
        }
        if self.eat("(") {
            let inner = self.choice()?;
            return if self.eat(")") { Ok(inner) } else { Err("missing `)` in plural formula".into()) };
        }
        self.skip_spaces();
        if let Some(rest) = self.rest.strip_prefix('n') {
            self.rest = rest;
            return Ok(Expression::N);
        }
        let digits = self.rest.len() - self.rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return Err(format!("unexpected `{}` in plural formula", self.rest));
        }
        let value = self.rest[..digits].parse().map_err(|_| "number too large in plural formula".to_string())?;
        self.rest = &self.rest[digits..];
        Ok(Expression::Number(value))
    }
}
