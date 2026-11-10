//! What syntect (written for version 1 of the `.sublime-syntax` format) lacks of version 2,
//! worked out before it reads a file:
//!
//! - `extends`: the parents' (a resource name, or a list of them) variables and contexts are
//!   merged in. A child's context replaces the parent's of the same name, unless it starts with
//!   `meta_prepend: true` / `meta_append: true`: then its patterns go before / after the parent's,
//!   and its `meta_*` keys override the parent's. Other top-level keys are the child's own.
//! - `branch_point` / `branch` / `fail`: syntect cannot go back in the text, so a `branch` pushes
//!   its first context and a `fail` pops. The first alternative is the usual case; text where
//!   Sublime would try the next one is highlighted as the first one says.
//!
//! A file that uses none of them goes to syntect untouched. The others are written back in YAML's
//! flow style, which syntect's YAML reader reads like the original.

use std::collections::HashMap;

use yaml_rust::yaml::Hash;
use yaml_rust::{Yaml, YamlLoader};

/// Parents nested deeper than this are taken for a cycle.
const MAX_DEPTH: usize = 16;

/// The `.sublime-syntax` files of the packages, by resource name (`Packages/<pkg>/<file>`): the
/// ones syntect loads and the ones they extend.
#[derive(Default)]
pub struct SyntaxFiles {
    texts: HashMap<String, String>,
    parsed: HashMap<String, Result<Hash, String>>,
}

impl SyntaxFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, resource: &str, text: String) {
        self.parsed.remove(resource);
        self.texts.insert(resource.to_string(), text);
    }

    /// The text syntect reads for `resource`: `None` when the file can go as it is, else the
    /// definition with its parents merged in and its branches rewritten.
    pub fn for_syntect(&mut self, resource: &str) -> Result<Option<String>, String> {
        let Some(text) = self.texts.get(resource) else {
            return Err(format!("{resource}: not found"));
        };
        if !uses_version_2(text) {
            return Ok(None);
        }
        let mut definition = self.resolve(resource, 0)?;
        if let Some(Yaml::Hash(contexts)) = definition.get_mut(&key("contexts")) {
            for (_, context) in contexts.iter_mut() {
                rewrite_patterns(context);
            }
        }
        let mut out = String::new();
        write_flow(&Yaml::Hash(definition), &mut out);
        Ok(Some(out))
    }

    fn parse(&mut self, resource: &str) -> Result<Hash, String> {
        if !self.parsed.contains_key(resource) {
            let parsed = match self.texts.get(resource) {
                None => Err(format!("{resource} not found")),
                Some(text) => match YamlLoader::load_from_str(text) {
                    Err(e) => Err(format!("{resource}: {e}")),
                    Ok(docs) => match docs.into_iter().next() {
                        Some(Yaml::Hash(hash)) => Ok(hash),
                        _ => Err(format!("{resource}: not a syntax definition")),
                    },
                },
            };
            self.parsed.insert(resource.to_string(), parsed);
        }
        self.parsed[resource].clone()
    }

    /// The definition of `resource` with every parent merged in, without `extends`.
    fn resolve(&mut self, resource: &str, depth: usize) -> Result<Hash, String> {
        if depth > MAX_DEPTH {
            return Err(format!("{resource}: extends itself"));
        }
        let mut definition = self.parse(resource)?;
        let parents: Vec<String> = match definition.remove(&key("extends")) {
            None => Vec::new(),
            Some(Yaml::String(parent)) => vec![parent],
            Some(Yaml::Array(parents)) => parents.into_iter().filter_map(|p| p.into_string()).collect(),
            Some(_) => return Err(format!("{resource}: extends is neither a resource nor a list of them")),
        };
        let mut inherited: Option<Hash> = None;
        for parent in parents {
            let parent = self.resolve(&parent_resource(resource, &parent), depth + 1).map_err(|e| format!("{resource}: extends {e}"))?;
            inherited = Some(match inherited {
                Some(base) => merge(base, parent),
                None => parent,
            });
        }
        Ok(match inherited {
            Some(base) => merge(base, definition),
            None => {
                strip_merge_keys(&mut definition);
                definition
            }
        })
    }
}

/// `extends` names a resource (`Packages/<pkg>/<file>`), or a file next to the child (`HTML (Plain).sublime-syntax`).
fn parent_resource(child: &str, parent: &str) -> String {
    if parent.starts_with("Packages/") {
        return parent.to_string();
    }
    match child.rsplit_once('/') {
        Some((folder, _)) => format!("{folder}/{parent}"),
        None => parent.to_string(),
    }
}

/// A cheap look for the keys this module handles (a false positive only costs a rewrite).
fn uses_version_2(text: &str) -> bool {
    ["extends", "branch_point", "fail:", "meta_prepend", "meta_append"].iter().any(|k| text.contains(k))
}

fn key(name: &str) -> Yaml {
    Yaml::String(name.to_string())
}

/// `child` over `base`: the child's top-level keys, both's variables and contexts.
fn merge(base: Hash, mut child: Hash) -> Hash {
    let mut variables = match base.get(&key("variables")) {
        Some(Yaml::Hash(v)) => v.clone(),
        _ => Hash::new(),
    };
    if let Some(Yaml::Hash(own)) = child.get(&key("variables")) {
        for (name, value) in own {
            variables.insert(name.clone(), value.clone());
        }
    }
    let mut contexts = match base.get(&key("contexts")) {
        Some(Yaml::Hash(c)) => c.clone(),
        _ => Hash::new(),
    };
    if let Some(Yaml::Hash(own)) = child.remove(&key("contexts")) {
        for (name, context) in own {
            let merged = match (contexts.get(&name), context) {
                (Some(Yaml::Array(inherited)), Yaml::Array(own)) => combine(inherited, own),
                (_, Yaml::Array(own)) => Yaml::Array(without_merge_keys(own)),
                (_, other) => other,
            };
            contexts.insert(name, merged);
        }
    }
    child.insert(key("variables"), Yaml::Hash(variables));
    child.insert(key("contexts"), Yaml::Hash(contexts));
    child
}

fn is_meta_key(name: &Yaml) -> bool {
    name.as_str().is_some_and(|n| n.starts_with("meta_") || n == "clear_scopes")
}

fn is_meta(entry: &Yaml) -> bool {
    matches!(entry, Yaml::Hash(h) if !h.is_empty() && h.keys().all(is_meta_key))
}

fn flag(entries: &[Yaml], name: &str) -> bool {
    entries.iter().filter(|e| is_meta(e)).any(|e| e[name].as_bool() == Some(true))
}

/// A child's context over the parent's of the same name.
fn combine(inherited: &[Yaml], own: Vec<Yaml>) -> Yaml {
    let prepend = flag(&own, "meta_prepend");
    let append = flag(&own, "meta_append");
    if !prepend && !append {
        return Yaml::Array(without_merge_keys(own));
    }
    let mut metas = Hash::new();
    for entry in inherited.iter().chain(&own).filter(|e| is_meta(e)) {
        if let Yaml::Hash(h) = entry {
            for (k, v) in h {
                metas.insert(k.clone(), v.clone());
            }
        }
    }
    metas.remove(&key("meta_prepend"));
    metas.remove(&key("meta_append"));
    let patterns = |entries: &[Yaml]| entries.iter().filter(|e| !is_meta(e)).cloned().collect::<Vec<_>>();
    let (first, second) = if prepend { (patterns(&own), patterns(inherited)) } else { (patterns(inherited), patterns(&own)) };
    let mut merged = Vec::new();
    if !metas.is_empty() {
        merged.push(Yaml::Hash(metas));
    }
    merged.extend(first);
    merged.extend(second);
    Yaml::Array(merged)
}

/// A context without `meta_prepend` / `meta_append`, which syntect would take for a pattern.
fn without_merge_keys(entries: Vec<Yaml>) -> Vec<Yaml> {
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            Yaml::Hash(mut h) if is_meta(&Yaml::Hash(h.clone())) => {
                h.remove(&key("meta_prepend"));
                h.remove(&key("meta_append"));
                (!h.is_empty()).then_some(Yaml::Hash(h))
            }
            other => Some(other),
        })
        .collect()
}

fn strip_merge_keys(definition: &mut Hash) {
    if let Some(Yaml::Hash(contexts)) = definition.get_mut(&key("contexts")) {
        for (_, context) in contexts.iter_mut() {
            if let Yaml::Array(entries) = context {
                *entries = without_merge_keys(std::mem::take(entries));
            }
        }
    }
}

/// Every pattern of a context (and of the anonymous contexts inside it): `branch` becomes a push
/// of its first context, `fail` a pop.
fn rewrite_patterns(context: &mut Yaml) {
    let Yaml::Array(entries) = context else { return };
    for entry in entries {
        let Yaml::Hash(pattern) = entry else { continue };
        pattern.remove(&key("branch_point"));
        if let Some(branch) = pattern.remove(&key("branch")) {
            let first = match branch {
                Yaml::Array(alternatives) => alternatives.into_iter().next(),
                other => Some(other),
            };
            if let Some(first) = first {
                pattern.insert(key("push"), first);
            }
        }
        if pattern.remove(&key("fail")).is_some() {
            pattern.insert(key("pop"), Yaml::Boolean(true));
        }
        for action in ["push", "set", "embed"] {
            let Some(target) = pattern.get_mut(&key(action)) else { continue };
            // An anonymous context (a list of patterns), or a list of contexts (names or anonymous).
            let anonymous = matches!(&*target, Yaml::Array(items) if items.iter().all(|i| matches!(i, Yaml::Hash(_))));
            if anonymous {
                rewrite_patterns(target);
            } else if let Yaml::Array(items) = target {
                items.iter_mut().for_each(rewrite_patterns);
            }
        }
        if let Some(prototype) = pattern.get_mut(&key("with_prototype")) {
            rewrite_patterns(prototype);
        }
    }
}

/// YAML in flow style (JSON-like, with plain integer keys for `captures`).
fn write_flow(value: &Yaml, out: &mut String) {
    match value {
        Yaml::String(s) => out.push_str(&serde_json::to_string(s).expect("a string serializes")),
        Yaml::Integer(i) => out.push_str(&i.to_string()),
        Yaml::Real(r) => out.push_str(r),
        Yaml::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Yaml::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_flow(item, out);
            }
            out.push(']');
        }
        Yaml::Hash(hash) => {
            out.push('{');
            for (i, (k, v)) in hash.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_flow(k, out);
                out.push_str(": ");
                write_flow(v, out);
            }
            out.push('}');
        }
        Yaml::Null | Yaml::Alias(_) | Yaml::BadValue => out.push('~'),
    }
}
