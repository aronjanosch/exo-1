//! Walker and suit tuning as data (#20).
use walker_core::{SuitConfig, WalkerConfig};

const WALKER: &str = include_str!("../../../content/tuning/walker.json");
const SUIT: &str = include_str!("../../../content/tuning/suit.json");

#[test]
fn shipped_files_equal_default() {
    assert_eq!(WalkerConfig::from_json(WALKER).unwrap(), WalkerConfig::default());
    assert_eq!(SuitConfig::from_json(SUIT).unwrap(), SuitConfig::default());
}

#[test]
fn rejects_unknown_and_missing_fields() {
    let e = WalkerConfig::from_json(&WALKER.replacen("\"skin\"", "\"skinn\": 1.0, \"skin\"", 1)).unwrap_err();
    assert!(e.contains("skinn"), "{e}");
    let e = SuitConfig::from_json(&SUIT.replacen("\"roll_rate\": 1.5", "\"roll\": 1.5", 1)).unwrap_err();
    assert!(e.contains("roll"), "{e}");
    let e = WalkerConfig::from_json(&WALKER.replacen("\"pitch_limit\": 1.5,", "", 1)).unwrap_err();
    assert!(e.contains("pitch_limit"), "{e}");
}
