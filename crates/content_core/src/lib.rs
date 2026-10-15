//! content_core: strict loading of JSON content, shared by the tuning loaders and the gameplay
use std::collections::BTreeMap;

use serde::de::DeserializeOwned;

/// Parses one object: every field required unless its type says otherwise, unknown fields
/// rejected, except an optional `_comment` string (as in the planet recipes). `what` names the
/// file in errors.
pub fn parse_strict<T: DeserializeOwned>(what: &str, s: &str) -> Result<T, String> {
    let mut v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("{what}: {e}"))?;
    if let Some(o) = v.as_object_mut()
        && let Some(c) = o.remove("_comment")
        && !c.is_string()
    {
        return Err(format!("{what}: _comment must be a string"));
    }
    serde_json::from_value(v).map_err(|e| format!("{what}: {e}"))
}

/// Like `parse_strict`, but an optional `_comment` string is dropped at every depth, not only at
/// the top: the ship tuning files keep one comment per block (`reactor`, `room`, `grid`, ...). A non-string
/// comment is an error wherever it sits.
pub fn parse_strict_commented<T: DeserializeOwned>(what: &str, s: &str) -> Result<T, String> {
    fn strip(v: &mut serde_json::Value) -> Result<(), String> {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(c) = o.remove("_comment") && !c.is_string() {
                    return Err("_comment must be a string".to_string());
                }
                o.values_mut().try_for_each(strip)
            }
            serde_json::Value::Array(a) => a.iter_mut().try_for_each(strip),
            _ => Ok(()),
        }
    }
    let mut v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("{what}: {e}"))?;
    strip(&mut v).map_err(|m| format!("{what}: {m}"))?;
    serde_json::from_value(v).map_err(|e| format!("{what}: {e}"))
}

/// Parses one value whose top-level blocks sit in several files, each file holding only the blocks it
/// owns (`owned` lists them by file path). A block in a file that does not own it is an error that
/// names the owner; a file that is not there is one too. The files' top-level `_comment` is dropped,
/// the rest is parsed like `parse_strict_commented`.
pub fn parse_fragments<T: DeserializeOwned>(what: &str, files: &[File], owned: &[(&str, &[&str])]) -> Result<T, Vec<String>> {
    let mut errors = Vec::new();
    let mut merged = serde_json::Map::new();
    for (path, blocks) in owned {
        let Some(f) = files.iter().find(|f| f.path == *path) else {
            errors.push(format!("{path}: missing"));
            continue;
        };
        let o = match serde_json::from_str::<serde_json::Value>(&f.text) {
            Ok(serde_json::Value::Object(o)) => o,
            Ok(_) => {
                errors.push(format!("{path}: must be one object"));
                continue;
            }
            Err(e) => {
                errors.push(format!("{path}: {e}"));
                continue;
            }
        };
        for (k, v) in o {
            if k == "_comment" {
                continue;
            }
            if !blocks.contains(&k.as_str()) {
                let owner = owned.iter().find(|(_, b)| b.contains(&k.as_str())).map(|(p, _)| *p);
                errors.push(match owner {
                    Some(p) => format!("{path}: {k}: belongs in {p}"),
                    None => format!("{path}: {k}: unknown block"),
                });
                continue;
            }
            merged.insert(k, v);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let text = serde_json::Value::Object(merged).to_string();
    parse_strict_commented::<T>(what, &text).map_err(|m| vec![m])
}

/// One content file: its path relative to the content root of its system (`commodity/mud.json`)
/// and its text.
#[derive(Clone, Debug)]
pub struct File {
    pub path: String,
    pub text: String,
}

impl File {
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> File {
        File { path: path.into(), text: text.into() }
    }

    pub fn folder(&self) -> &str {
        self.path.split('/').next().unwrap_or("")
    }
}

pub trait Record {
    type Id: Ord + Clone + std::fmt::Display;
    fn id(&self) -> &Self::Id;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Loaded<T> {
    pub path: String,
    pub record: T,
}

/// Parses every file in `folder` as a `T` and keys them by id. Parse errors and duplicate ids are
/// pushed to `errors`; the good records are returned either way, so later checks can report more.
pub fn load_records<T: Record + DeserializeOwned>(files: &[File], folder: &str, errors: &mut Vec<String>) -> BTreeMap<T::Id, Loaded<T>> {
    let mut out: BTreeMap<T::Id, Loaded<T>> = BTreeMap::new();
    for f in files.iter().filter(|f| f.folder() == folder) {
        match parse_strict::<T>(&f.path, &f.text) {
            Ok(r) => {
                let id = r.id().clone();
                if let Some(prev) = out.get(&id) {
                    errors.push(format!("{}: id: '{id}' is already used by {}", f.path, prev.path));
                } else {
                    out.insert(id, Loaded { path: f.path.clone(), record: r });
                }
            }
            Err(e) => errors.push(e),
        }
    }
    out
}

/// Checks an id for the content convention: lower case letters, digits and `_`, not empty.
pub fn check_id(path: &str, field: &str, id: &str, errors: &mut Vec<String>) {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        errors.push(format!("{path}: {field}: '{id}' must be lower case letters, digits and _"));
    }
}

#[cfg(test)]
mod tests;
