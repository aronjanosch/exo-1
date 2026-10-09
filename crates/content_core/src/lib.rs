//! content_core: strict loading of JSON content, shared by the tuning loaders and the gameplay
//! systems, without engine types (#122).
//!
//! - `parse_strict`: one JSON object, every field as its type says, unknown fields rejected
//!   (`deny_unknown_fields` on the type), except an optional top-level `_comment` string.
//! - `File` and `load_records`: one file per record, grouped by folder (`commodity/…`), keyed by
//!   the record's id; duplicate ids are errors. Errors name the file and the field.
//!
//! No file I/O here: the glue reads the files and hands over path and text.
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

    /// The first path component: the record kind (`commodity`).
    pub fn folder(&self) -> &str {
        self.path.split('/').next().unwrap_or("")
    }
}

/// A record with an id, loaded one per file.
pub trait Record {
    type Id: Ord + Clone + std::fmt::Display;
    fn id(&self) -> &Self::Id;
}

/// A loaded record and the file it came from (for error messages in later checks).
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
