//! gettext `.po` files: the translations of the interface (`Localization/<language>.po`).
//! Entries marked `#, fuzzy` are skipped, as gettext does.

use entities::localization::Catalog;

/// A parsed `.po` file.
pub struct PoFile {
    pub catalog: Catalog,
    /// The `Language` header (e.g. `pt_BR`).
    pub language: Option<String>,
    /// The `X-Language-Name` header: the language's own name (e.g. `Português (Brasil)`).
    pub language_name: Option<String>,
}

#[derive(Default)]
struct Entry {
    context: Option<String>,
    msgid: Option<String>,
    msgid_plural: Option<String>,
    msgstr: Vec<String>,
    fuzzy: bool,
}

/// Which string the following `"…"` continuation lines belong to.
#[derive(Clone, Copy)]
enum Field {
    Context,
    Id,
    IdPlural,
    Str(usize),
}

pub fn parse(text: &str) -> Result<PoFile, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut entries = Vec::new();
    let mut entry = Entry::default();
    let mut field = None;

    for (number, line) in text.lines().enumerate() {
        let error = |message: &str| format!("line {}: {message}", number + 1);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            // Obsolete entries (`#~`) and comments; a comment starts the next entry.
            if entry.msgid.is_some() && !entry.msgstr.is_empty() {
                entries.push(std::mem::take(&mut entry));
                field = None;
            }
            if let Some(flags) = comment.strip_prefix(',') {
                entry.fuzzy |= flags.split(',').any(|flag| flag.trim() == "fuzzy");
            }
            continue;
        }
        if line.starts_with('"') {
            let value = unquote(line).ok_or_else(|| error("unterminated string"))?;
            match field {
                Some(Field::Context) => entry.context.get_or_insert_with(String::new).push_str(&value),
                Some(Field::Id) => entry.msgid.get_or_insert_with(String::new).push_str(&value),
                Some(Field::IdPlural) => entry.msgid_plural.get_or_insert_with(String::new).push_str(&value),
                Some(Field::Str(index)) => entry.msgstr[index].push_str(&value),
                None => return Err(error("string without a keyword")),
            }
            continue;
        }

        let (keyword, rest) = line.split_once(char::is_whitespace).ok_or_else(|| error("expected a keyword and a string"))?;
        let value = unquote(rest.trim()).ok_or_else(|| error("expected a quoted string"))?;
        // A new msgctxt/msgid after a complete entry starts the next one.
        if matches!(keyword, "msgctxt" | "msgid") && entry.msgid.is_some() && !entry.msgstr.is_empty() {
            entries.push(std::mem::take(&mut entry));
        }
        field = Some(match keyword {
            "msgctxt" => {
                entry.context = Some(value);
                Field::Context
            }
            "msgid" => {
                entry.msgid = Some(value);
                Field::Id
            }
            "msgid_plural" => {
                entry.msgid_plural = Some(value);
                Field::IdPlural
            }
            "msgstr" => {
                entry.msgstr = vec![value];
                Field::Str(0)
            }
            _ => {
                let index = keyword
                    .strip_prefix("msgstr[")
                    .and_then(|k| k.strip_suffix(']'))
                    .and_then(|k| k.parse::<usize>().ok())
                    .ok_or_else(|| error(&format!("unknown keyword `{keyword}`")))?;
                if entry.msgstr.len() <= index {
                    entry.msgstr.resize(index + 1, String::new());
                }
                entry.msgstr[index] = value;
                Field::Str(index)
            }
        });
    }
    if entry.msgid.is_some() {
        entries.push(entry);
    }

    // The header is the entry with an empty msgid.
    let header = entries
        .iter()
        .find(|e| e.msgid.as_deref() == Some("") && e.context.is_none())
        .and_then(|e| e.msgstr.first())
        .cloned()
        .unwrap_or_default();
    let header_value = |name: &str| {
        header.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim().eq_ignore_ascii_case(name) && !value.trim().is_empty()).then(|| value.trim().to_string())
        })
    };

    let mut catalog = Catalog::new(header_value("Plural-Forms").as_deref())?;
    for entry in entries {
        if entry.fuzzy {
            continue;
        }
        if let Some(msgid) = &entry.msgid {
            catalog.insert(entry.context.as_deref(), msgid, entry.msgstr);
        }
    }
    Ok(PoFile {
        catalog,
        language: header_value("Language"),
        language_name: header_value("X-Language-Name"),
    })
}

/// `"Line \"{line}\"\n"` → `Line "{line}"` plus a newline.
fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            other => out.push(other),
        }
    }
    Some(out)
}
