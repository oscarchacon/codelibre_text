use serde_json::Value;

/// A command invocation in Sublime's form: a name plus JSON arguments,
/// e.g. `show_overlay {"overlay": "goto", "text": ":"}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub name: String,
    pub args: Value,
}

impl Command {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            args: Value::Null,
        }
    }

    pub fn with_args(name: &str, args: Value) -> Self {
        Self {
            name: name.to_string(),
            args,
        }
    }

    pub fn str_arg(&self, key: &str) -> &str {
        self.args.get(key).and_then(Value::as_str).unwrap_or("")
    }

    pub fn bool_arg(&self, key: &str, default: bool) -> bool {
        self.args.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn usize_arg(&self, key: &str, default: usize) -> usize {
        self.args
            .get(key)
            .and_then(Value::as_u64)
            .map(|v| v as usize)
            .unwrap_or(default)
    }

    /// Same command and arguments; missing arguments and `{}` are equivalent.
    pub fn same_as(&self, other: &Command) -> bool {
        let empty = |v: &Value| v.is_null() || v.as_object().is_some_and(|m| m.is_empty());
        self.name == other.name && (self.args == other.args || (empty(&self.args) && empty(&other.args)))
    }
}
