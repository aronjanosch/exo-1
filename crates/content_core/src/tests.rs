use super::*;
use serde::Deserialize;

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct Thing {
    id: String,
    size: f64,
}

impl Record for Thing {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

#[test]
fn strict_accepts_a_string_comment() {
    let t: Thing = parse_strict("thing.json", r#"{ "_comment": "hi", "id": "a", "size": 1.0 }"#).unwrap();
    assert_eq!(t, Thing { id: "a".into(), size: 1.0 });
}

#[test]
fn strict_rejects_a_non_string_comment() {
    let e = parse_strict::<Thing>("thing.json", r#"{ "_comment": 1, "id": "a", "size": 1.0 }"#).unwrap_err();
    assert!(e.contains("thing.json") && e.contains("_comment"), "{e}");
}

#[test]
fn strict_names_file_and_unknown_field() {
    let e = parse_strict::<Thing>("thing.json", r#"{ "id": "a", "size": 1.0, "colour": 2 }"#).unwrap_err();
    assert!(e.contains("thing.json") && e.contains("colour"), "{e}");
}

#[test]
fn strict_names_a_missing_field() {
    let e = parse_strict::<Thing>("thing.json", r#"{ "id": "a" }"#).unwrap_err();
    assert!(e.contains("size"), "{e}");
}

#[test]
fn records_load_by_folder_and_reject_duplicate_ids() {
    let files = [
        File::new("thing/a.json", r#"{ "id": "a", "size": 1.0 }"#),
        File::new("thing/b.json", r#"{ "id": "a", "size": 2.0 }"#),
        File::new("other/c.json", r#"{ "id": "c", "size": 3.0 }"#),
    ];
    let mut errors = Vec::new();
    let got = load_records::<Thing>(&files, "thing", &mut errors);
    assert_eq!(got.len(), 1);
    assert_eq!(got["a"].path, "thing/a.json");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("thing/b.json: id:") && errors[0].contains("thing/a.json"), "{errors:?}");
}

#[test]
fn ids_follow_the_convention() {
    let mut errors = Vec::new();
    check_id("x.json", "id", "fizzy_mud_2", &mut errors);
    assert!(errors.is_empty());
    for bad in ["", "Fizzy", "fizzy-mud", "fizzy mud"] {
        check_id("x.json", "id", bad, &mut errors);
    }
    assert_eq!(errors.len(), 4, "{errors:?}");
}
