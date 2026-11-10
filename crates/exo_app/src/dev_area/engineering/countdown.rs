//! The implosion countdown aboard (#325): what the crew gets while the core's implosion timer runs. The klaxon and the red
//! pulse are the ship's Danger alarm (`hull::asks::alarm_level` forces it, `hull::lights` dims the room's warm lamps); the
//! notices (at the start, at 30 s, at 10 s) come from the host's news in every process (`ReactorNews::implosion_eta_s`).
//! A timer that runs back under critical heat stops it and re-arms the notices; an ejected core's countdown is the one outside
//! (`outside_core`), not this one. Every number is a start value, TODO(initiator).
use crate::gameplay::Gameplay;
use crate::items::Items;
use gameplay_core::notice::{Notice, NoticeKind};
use item_core::wire::ReactorNews;

/// Seconds left at which the second and third notice show. TODO(initiator).
pub const NOTICE_AT_S: [f64; 2] = [30.0, 10.0];
pub const KEYS: [&str; 3] = ["notice.reactor.countdown", "notice.reactor.countdown_30", "notice.reactor.countdown_10"];

/// The core's timer runs aboard: news with a countdown that is not an ejected core's (its eta is the outside one).
pub fn countdown_on(n: &ReactorNews) -> bool {
    n.implosion_eta_s.is_some_and(f64::is_finite) && !n.ejected && !n.imploded
}

/// Which notices are out for the running countdown.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct Notes {
    started: bool,
    said: [bool; 2],
}

impl Notes {
    /// One look at the news: the keys of the notices that are new now. A countdown that is off re-arms everything. One that is
    /// already below a line when first seen (a late joiner) does not replay the lines it is past.
    pub fn step(&mut self, news: Option<&ReactorNews>) -> Vec<&'static str> {
        let eta = news.filter(|n| countdown_on(n)).and_then(|n| n.implosion_eta_s);
        let Some(eta) = eta else {
            *self = Notes::default();
            return Vec::new();
        };
        let mut out = Vec::new();
        if !self.started {
            self.started = true;
            out.push(KEYS[0]);
            for (i, at) in NOTICE_AT_S.iter().enumerate() {
                self.said[i] = eta <= *at;
            }
            return out;
        }
        for (i, at) in NOTICE_AT_S.iter().enumerate() {
            if eta <= *at && !self.said[i] {
                self.said[i] = true;
                out.push(KEYS[i + 1]);
            }
        }
        out
    }
}

/// Every process: the countdown's notices.
pub fn notices(items: Res<Items>, mut notes: Local<Notes>, mut gp: ResMut<Gameplay>) {
    for key in notes.step(items.reactor.as_ref()) {
        gp.notify(Notice::new(NoticeKind::Warning, key));
    }
}

use bevy::prelude::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn news(eta: Option<f64>) -> ReactorNews {
        ReactorNews { implosion_eta_s: eta, ..Default::default() }
    }

    #[test]
    fn the_notices_come_at_the_start_at_30_and_at_10_once_each() {
        let mut n = Notes::default();
        assert!(n.step(Some(&news(None))).is_empty());
        assert_eq!(n.step(Some(&news(Some(100.0)))), vec![KEYS[0]]);
        assert!(n.step(Some(&news(Some(60.0)))).is_empty());
        assert_eq!(n.step(Some(&news(Some(29.9)))), vec![KEYS[1]]);
        assert!(n.step(Some(&news(Some(25.0)))).is_empty());
        assert_eq!(n.step(Some(&news(Some(9.0)))), vec![KEYS[2]]);
        assert!(n.step(Some(&news(Some(5.0)))).is_empty());
    }

    #[test]
    fn a_timer_that_ran_back_re_arms_and_a_late_joiner_does_not_replay_past_lines() {
        let mut n = Notes::default();
        n.step(Some(&news(Some(20.0))));
        assert!(n.step(Some(&news(None))).is_empty());
        assert_eq!(n.step(Some(&news(Some(50.0)))), vec![KEYS[0]], "a new run starts again");
        let mut late = Notes::default();
        assert_eq!(late.step(Some(&news(Some(8.0)))), vec![KEYS[0]], "only the start line, the others are past");
        assert!(late.step(Some(&news(Some(7.0)))).is_empty());
    }

    #[test]
    fn an_ejected_or_imploded_core_has_no_countdown_aboard_and_a_bad_eta_is_none() {
        let mut n = Notes::default();
        assert!(n.step(Some(&ReactorNews { ejected: true, ..news(Some(40.0)) })).is_empty());
        assert!(n.step(Some(&ReactorNews { imploded: true, ..news(Some(40.0)) })).is_empty());
        assert!(n.step(Some(&news(Some(f64::NAN)))).is_empty());
        assert!(n.step(None).is_empty());
    }
}
