use crate::rng::Rng;
use serde::Deserialize;

const KEY_NAME: u64 = 0x4E41_4D45;

/// Name pools. A system is `<first> <second>`; planets and stations come from their own pools.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Names {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub system_first: Vec<String>,
    pub system_second: Vec<String>,
    pub planet: Vec<String>,
    pub station: Vec<String>,
}

impl Names {
    pub fn from_json(json: &str) -> Result<Names, String> {
        let names: Names = serde_json::from_str(json).map_err(|e| e.to_string())?;
        names.validate()?;
        Ok(names)
    }

    fn validate(&self) -> Result<(), String> {
        let pools = [
            ("system_first", &self.system_first),
            ("system_second", &self.system_second),
            ("planet", &self.planet),
            ("station", &self.station),
        ];
        for (name, pool) in pools {
            if pool.is_empty() {
                return Err(format!("{name}: empty"));
            }
            for (i, entry) in pool.iter().enumerate() {
                if entry.is_empty() {
                    return Err(format!("{name}[{i}]: empty entry"));
                }
                if pool[..i].contains(entry) {
                    return Err(format!("{name}[{i}]: duplicate {entry}"));
                }
            }
        }
        Ok(())
    }

    /// The same seed gives the same name. The pools are checked non-empty by `from_json`.
    pub fn system_name(&self, seed: u64) -> String {
        let mut rng = Rng::derive(seed, KEY_NAME);
        let first = rng.pick(&self.system_first).expect("system_first is validated");
        let second = rng.pick(&self.system_second).expect("system_second is validated");
        format!("{first} {second}")
    }
}
