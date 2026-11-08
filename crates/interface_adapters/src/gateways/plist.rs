//! XML property lists (`.tmPreferences`, the TextMate metadata Sublime reads: comment tokens,
//! symbol lists, indentation rules), read into JSON values: `dict` → object, `array` → array,
//! `string` / `date` / `data` → string, `integer` / `real` → number, `true` / `false` → bool.

use serde_json::{Map, Number, Value};

/// The value inside `<plist>` (or the first value of the file).
pub fn parse(source: &str) -> Result<Value, String> {
    let tokens = tokenize(source)?;
    let mut at = 0;
    while let Some(token) = tokens.get(at) {
        match token {
            Token::Open(name) if name == "plist" => at += 1,
            Token::Open(_) | Token::Empty(_) => return Parser { tokens: &tokens, at }.value(),
            _ => at += 1,
        }
    }
    Err("no value in the property list".into())
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open(String),
    Close(String),
    /// `<true/>`
    Empty(String),
    Text(String),
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut rest = source;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after.find("-->").ok_or("unclosed comment")?;
            rest = &after[end + 3..];
        } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>").ok_or("unclosed CDATA")?;
            tokens.push(Token::Text(after[..end].to_string()));
            rest = &after[end + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest.find('>').ok_or("unclosed declaration")?;
            rest = &rest[end + 1..];
        } else if let Some(after) = rest.strip_prefix('<') {
            let end = after.find('>').ok_or("unclosed tag")?;
            let tag = after[..end].trim();
            rest = &after[end + 1..];
            if let Some(name) = tag.strip_prefix('/') {
                tokens.push(Token::Close(name.trim().to_string()));
            } else if let Some(inner) = tag.strip_suffix('/') {
                tokens.push(Token::Empty(tag_name(inner)));
            } else {
                tokens.push(Token::Open(tag_name(tag)));
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            tokens.push(Token::Text(decode_entities(&rest[..end])));
            rest = &rest[end..];
        }
    }
    Ok(tokens)
}

fn tag_name(tag: &str) -> String {
    tag.split_whitespace().next().unwrap_or_default().to_string()
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..end];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl Parser<'_> {
    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.at);
        self.at += 1;
        token
    }

    /// Skips the whitespace between elements.
    fn skip_blank(&mut self) {
        while matches!(self.tokens.get(self.at), Some(Token::Text(t)) if t.trim().is_empty()) {
            self.at += 1;
        }
    }

    /// The text up to `</name>`.
    fn text_until(&mut self, name: &str) -> Result<String, String> {
        let mut text = String::new();
        loop {
            match self.next() {
                Some(Token::Text(t)) => text.push_str(t),
                Some(Token::Close(n)) if n == name => return Ok(text),
                other => return Err(format!("expected </{name}>, found {other:?}")),
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_blank();
        match self.next().cloned() {
            Some(Token::Empty(name)) => match name.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "string" | "data" | "date" => Ok(Value::String(String::new())),
                "dict" => Ok(Value::Object(Map::new())),
                "array" => Ok(Value::Array(Vec::new())),
                other => Err(format!("unknown element <{other}/>")),
            },
            Some(Token::Open(name)) => match name.as_str() {
                "dict" => self.dict(),
                "array" => self.array(),
                "string" | "data" | "date" => self.text_until(&name).map(Value::String),
                "integer" => {
                    let text = self.text_until(&name)?;
                    text.trim().parse::<i64>().map(Value::from).map_err(|_| format!("bad integer {text:?}"))
                }
                "real" => {
                    let text = self.text_until(&name)?;
                    let number = text.trim().parse::<f64>().ok().and_then(Number::from_f64);
                    number.map(Value::Number).ok_or_else(|| format!("bad real {text:?}"))
                }
                "true" | "false" => {
                    self.text_until(&name)?;
                    Ok(Value::Bool(name == "true"))
                }
                other => Err(format!("unknown element <{other}>")),
            },
            other => Err(format!("expected a value, found {other:?}")),
        }
    }

    fn dict(&mut self) -> Result<Value, String> {
        let mut map = Map::new();
        loop {
            self.skip_blank();
            match self.next().cloned() {
                Some(Token::Close(name)) if name == "dict" => return Ok(Value::Object(map)),
                Some(Token::Open(name)) if name == "key" => {
                    let key = self.text_until("key")?;
                    let value = self.value()?;
                    map.insert(key, value);
                }
                other => return Err(format!("expected <key> or </dict>, found {other:?}")),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        let mut items = Vec::new();
        loop {
            self.skip_blank();
            if matches!(self.tokens.get(self.at), Some(Token::Close(name)) if name == "array") {
                self.at += 1;
                return Ok(Value::Array(items));
            }
            items.push(self.value()?);
        }
    }
}
