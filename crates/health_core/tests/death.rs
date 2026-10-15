//! Death costs skill; a crew wiped at once is game over (#254).
use health_core::skills::{CrewSkills, Skill, Skills, START_SKILL};
use health_core::{crew_wiped, Life};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_death_takes_the_share_of_what_is_above_the_start_level() {
    // 80 is 30 above the start (50): 20 % of 30 is 6, so 74 is left.
    let mut s = Skills { mechanical: 80.0, electrical: 80.0, medical: 80.0 };
    s.lose_on_death(20.0);
    assert!(close(s.mechanical, 74.0) && close(s.electrical, 74.0) && close(s.medical, 74.0), "{s:?}");
}

#[test]
fn a_skill_at_or_below_the_start_level_is_kept() {
    let mut s = Skills { mechanical: 30.0, electrical: START_SKILL, medical: 12.0 };
    s.lose_on_death(20.0);
    assert_eq!((s.mechanical, s.electrical, s.medical), (30.0, START_SKILL, 12.0));
}

#[test]
fn no_percentage_takes_nothing() {
    let mut s = Skills { mechanical: 90.0, ..Skills::default() };
    s.lose_on_death(0.0);
    assert!(close(s.mechanical, 90.0));
}

#[test]
fn a_gain_after_a_death_still_adds() {
    let mut store: CrewSkills<u32> = CrewSkills::new();
    store.gain(7, Skill::Mechanical, 30.0);
    store.died(7, 20.0);
    let after_death = store.of(7).mechanical;
    assert!(close(after_death, 80.0 - 0.2 * 30.0), "{after_death}");
    store.gain(7, Skill::Mechanical, 2.0);
    assert!(close(store.of(7).mechanical, after_death + 2.0));
}

#[test]
fn a_gain_stops_at_100() {
    let mut store: CrewSkills<u32> = CrewSkills::new();
    store.gain(1, Skill::Mechanical, 80.0);
    assert!(close(store.of(1).mechanical, 100.0));
}

#[test]
fn a_player_without_a_record_has_the_start_level_and_the_loss_is_per_player() {
    let mut store: CrewSkills<u32> = CrewSkills::new();
    assert!(close(store.of(3).mechanical, START_SKILL));
    store.gain(1, Skill::Mechanical, 30.0);
    store.died(2, 20.0);
    assert!(close(store.of(1).mechanical, 80.0), "a death of player 2 does not touch player 1");
    assert!(close(store.of(2).mechanical, START_SKILL), "player 2 had nothing above the start");
}

#[test]
fn a_crew_is_wiped_when_every_member_is_dead_at_once() {
    assert!(crew_wiped([Life::Dead]));
    assert!(crew_wiped([Life::Dead, Life::Dead]));
}

#[test]
fn a_crew_is_not_wiped_while_one_member_lives_or_with_no_crew() {
    assert!(!crew_wiped(std::iter::empty::<Life>()));
    assert!(!crew_wiped([Life::Dead, Life::Conscious]));
    assert!(!crew_wiped([Life::Unconscious, Life::Dead]));
}
