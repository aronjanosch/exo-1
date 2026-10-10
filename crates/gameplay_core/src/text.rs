//! Texts the player reads (#165): a table of keys to one line or a pool of lines, and a picker
//! that takes a line from a pool by seed and never the same line twice in a row (research:
//! "the same bark again and again"). English only in D; #131 turns the table into checked
//! localisation tables.
use std::collections::BTreeMap;

use serde::Deserialize;

use crate::id::TextKey;
use crate::notice::{Arg, Notice};
use crate::rng::Rng;

#[derive(Deserialize)]
#[serde(untagged)]
enum Lines {
    One(String),
    Pool(Vec<String>),
}

/// Key to lines. A pool has at least one line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextTable {
    lines: BTreeMap<String, Vec<String>>,
}

impl TextTable {
    /// Parses `{ "key": "line" | ["line", ...] }` with an optional `_comment`. `what` names the
    /// file in errors.
    pub fn from_json(what: &str, text: &str) -> Result<TextTable, String> {
        let raw: BTreeMap<String, serde_json::Value> = serde_json::from_str(text).map_err(|e| format!("{what}: {e}"))?;
        let mut lines = BTreeMap::new();
        for (k, v) in raw {
            if k == "_comment" {
                continue;
            }
            let l: Lines = serde_json::from_value(v).map_err(|_| format!("{what}: {k}: must be a string or a list of strings"))?;
            let v = match l {
                Lines::One(s) => vec![s],
                Lines::Pool(p) if p.is_empty() => return Err(format!("{what}: {k}: empty pool")),
                Lines::Pool(p) => p,
            };
            lines.insert(k, v);
        }
        Ok(TextTable { lines })
    }

    pub fn lines(&self, key: &str) -> Option<&[String]> {
        self.lines.get(key).map(Vec::as_slice)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.lines.keys().map(String::as_str)
    }

    pub fn has(&self, key: &str) -> bool {
        self.lines.contains_key(key)
    }
}

/// Picks lines from pools. The same key never gives the same line twice in a row (a pool of one
/// has no choice). Deterministic for a seed and the order of calls.
#[derive(Clone, Debug)]
pub struct Picker {
    rng: Rng,
    last: BTreeMap<String, usize>,
}

impl Picker {
    pub fn new(seed: u64) -> Picker {
        Picker { rng: Rng::new(seed), last: BTreeMap::new() }
    }

    /// A line of `key`; the key itself when the table has none.
    pub fn pick(&mut self, table: &TextTable, key: &str) -> String {
        let Some(lines) = table.lines(key) else { return key.to_string() };
        let i = match (lines.len(), self.last.get(key)) {
            (1, _) => 0,
            // Draw from the others: skip the last index.
            (n, Some(&last)) => {
                let i = self.rng.below(n - 1);
                if i >= last { i + 1 } else { i }
            }
            (n, None) => self.rng.below(n),
        };
        self.last.insert(key.to_string(), i);
        lines[i].clone()
    }

    /// The notice's text: a line of its key with `{name}` filled from its arguments.
    pub fn render(&mut self, table: &TextTable, n: &Notice) -> String {
        let mut s = self.pick(table, n.key.as_str());
        for (name, arg) in &n.args {
            let v = match arg {
                Arg::Text(t) => t.clone(),
                Arg::Number(x) => x.to_string(),
                Arg::Key(k) => self.pick(table, k.as_str()),
            };
            s = s.replace(&format!("{{{name}}}"), &v);
        }
        s
    }

    /// A key as text.
    pub fn text(&mut self, table: &TextTable, key: &TextKey) -> String {
        self.pick(table, key.as_str())
    }
}
