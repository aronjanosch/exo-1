//! Alert levels (#288): set by the captain, automatic yellow on a threat, log of changes.
use crew_core::*;

#[test]
fn starts_green_and_only_the_captain_sets_it() {
    let mut a = Alert::default();
    assert_eq!(a.level(), AlertLevel::Green);
    assert_eq!(a.last_change_s(), None);
    assert_eq!(a.set(Role::Engineer, AlertLevel::Red, 5.0), Err(Refusal::NotCaptain));
    assert_eq!(a.level(), AlertLevel::Green);

    let ev = a.set(Role::Captain, AlertLevel::Red, 5.0).unwrap();
    assert_eq!(ev, Some(AlertEvent::Changed { from: AlertLevel::Green, to: AlertLevel::Red, cause: AlertCause::Captain, at_s: 5.0 }));
    assert_eq!(a.level(), AlertLevel::Red);
    assert_eq!(a.last_change_s(), Some(5.0));
    // The same level again changes nothing and logs nothing.
    assert_eq!(a.set(Role::Captain, AlertLevel::Red, 6.0).unwrap(), None);
    assert_eq!(a.last_change_s(), Some(5.0));
    assert_eq!(a.log().len(), 1);
}

#[test]
fn a_new_threat_raises_yellow_once() {
    let mut a = Alert::default();
    assert_eq!(a.update(false, 1.0), None);
    let ev = a.update(true, 2.0);
    assert_eq!(ev, Some(AlertEvent::Changed { from: AlertLevel::Green, to: AlertLevel::Yellow, cause: AlertCause::Auto, at_s: 2.0 }));
    // The threat stays: nothing more. The captain lowers it to green: it stays green while the
    // same threat lasts (the captain decided).
    assert_eq!(a.update(true, 3.0), None);
    a.set(Role::Captain, AlertLevel::Green, 4.0).unwrap();
    assert_eq!(a.update(true, 5.0), None);
    assert_eq!(a.level(), AlertLevel::Green);
    // The threat goes and comes back after the captain's hold: yellow again.
    assert_eq!(a.update(false, 40.0), None);
    assert!(a.update(true, 41.0).is_some());
    assert_eq!(a.level(), AlertLevel::Yellow);
}

#[test]
fn auto_yellow_respects_a_recent_captain_call_and_never_lowers() {
    let mut a = Alert::default();
    a.set(Role::Captain, AlertLevel::Green, 10.0).unwrap();
    // Within the captain's hold (30 s) a new threat does not override the captain.
    assert_eq!(a.update(true, 20.0), None);
    assert_eq!(a.level(), AlertLevel::Green);
    a.update(false, 21.0);
    assert!(a.update(true, 10.0 + alert::CAPTAIN_HOLD_S + 0.5).is_some());

    // Red stays red on a threat; auto never lowers or raises to red.
    let mut b = Alert::default();
    b.set(Role::Captain, AlertLevel::Red, 0.0).unwrap();
    assert_eq!(b.update(true, 100.0), None);
    assert_eq!(b.level(), AlertLevel::Red);
}

#[test]
fn the_log_keeps_the_last_changes() {
    let mut a = Alert::default();
    for i in 0..(alert::LOG_LEN + 5) {
        let to = if i % 2 == 0 { AlertLevel::Yellow } else { AlertLevel::Green };
        a.set(Role::Captain, to, i as f64).unwrap();
    }
    assert_eq!(a.log().len(), alert::LOG_LEN);
    assert_eq!(a.log().last().unwrap().at_s, (alert::LOG_LEN + 4) as f64);
    assert_eq!(AlertLevel::Red.notice().key.as_str(), "notice.alert.red");
}
