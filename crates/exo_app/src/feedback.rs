//! Round 6b: felt feedback. What a player does answers with more than a line of text: a stamp
//! punched onto the screen, a camera kick, a burst at the object, a wallet delta floating up, a
//! buyer who jumps. The fixed step turns world events into `Cue`s (headless, scenarios check
//! them); `feedback_view` draws them in the window. Values and words are `TODO(initiator)`.
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::content::WALLET;
use gameplay_core::{LocationId, ScanChoice, TextKey, ViolationKind, WorldEvent};

use crate::gameplay::Gameplay;

/// The colour family of a cue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Bad,
    Info,
    Gold,
}

/// Where a burst happens.
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    Ship,
    Walker,
    Kettle(String),
    Buyer(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cue {
    /// A word punched onto the screen: a verdict, a grade, a quality. Shown `hold` s late (the
    /// pause before a verdict is the tension).
    Stamp { text: String, tone: Tone, hold: f64 },
    /// A camera kick (trauma 0..1) for the seated view and the walker in a flying cabin.
    Kick(f64),
    /// Dust spreading on the ground under the ship (touchdown).
    Dust,
    /// A burst of small cubes at an object: crates poofing, a splash in the kettle.
    Burst { at: Anchor, tone: Tone },
    /// The wallet changed: the delta floats up beside it.
    Money(i64),
    /// A heat star came (`up`) or went.
    Heat { up: bool },
    /// The goal line changed.
    Goal,
    /// A character jumps (a buyer paid).
    Hop(String),
}

/// Cues so far with their game time, oldest first, and what the cues compare against.
#[derive(Debug, Default)]
pub struct Feedback {
    pub cues: Vec<(f64, Cue)>,
    stars: u8,
    goal: String,
}

impl Feedback {
    pub fn push(&mut self, at: f64, c: Cue) {
        self.cues.push((at, c));
    }

    /// The goal line as shown now; a change cues a pulse.
    pub fn goal_line(&mut self, at: f64, line: &str) {
        if self.goal != line {
            if !self.goal.is_empty() {
                self.push(at, Cue::Goal);
            }
            self.goal = line.to_string();
        }
    }

    pub fn stamps(&self) -> impl Iterator<Item = &str> {
        self.cues.iter().filter_map(|(_, c)| if let Cue::Stamp { text, .. } = c { Some(text.as_str()) } else { None })
    }
}

/// Kick per m/s of touchdown speed, and its bounds. TODO(initiator).
const KICK_PER_SPEED: f64 = 0.12;
const KICK_MIN: f64 = 0.12;
const KICK_MAX: f64 = 0.6;
/// The pause before a scan's verdict (s). TODO(initiator).
pub const VERDICT_HOLD: f64 = 0.8;
/// Quality tiers of a brew: at least this quality for the tier's word. TODO(initiator).
pub const QUALITY_TIERS: [(f64, &str, Tone); 4] = [(0.9, "stamp.quality.perfect", Tone::Gold), (0.7, "stamp.quality.good", Tone::Good), (0.4, "stamp.quality.ok", Tone::Info), (0.0, "stamp.quality.sloppy", Tone::Bad)];

/// The cues of one applied world event, plus the text the HUD lacked (the payday's banner, a
/// new order's toast). Called for every world event in `gameplay_step`.
pub fn on_world(g: &mut Gameplay, ev: &WorldEvent) {
    let at = g.clock();
    let mut cues = Vec::new();
    let stamp = |g: &mut Gameplay, key: &str, args: &[(&str, Arg)], tone: Tone, hold: f64| {
        let mut n = Notice::new(NoticeKind::Updated, key);
        for (k, a) in args {
            n = n.arg(k, a.clone());
        }
        let text = g.render(&n);
        Cue::Stamp { text, tone, hold }
    };
    match ev {
        WorldEvent::ClearanceGranted { pad, window_s, inspection, .. } => {
            let (key, tone) = if *inspection { ("stamp.cleared_inspection", Tone::Bad) } else { ("stamp.cleared", Tone::Info) };
            let pad = pad_name(g, pad);
            cues.push(stamp(g, key, &[("pad", pad), ("s", Arg::Number(window_s.round() as i64))], tone, 0.0));
        }
        WorldEvent::ClearanceHeld { wait_s, .. } => cues.push(stamp(g, "stamp.held", &[("s", Arg::Number(wait_s.round() as i64))], Tone::Info, 0.0)),
        WorldEvent::ClearanceExpired { .. } => cues.push(stamp(g, "stamp.expired", &[], Tone::Bad, 0.0)),
        WorldEvent::Landed { speed, .. } => {
            cues.push(Cue::Dust);
            cues.push(Cue::Kick((speed * KICK_PER_SPEED).clamp(KICK_MIN, KICK_MAX)));
        }
        WorldEvent::LandingGraded { grade, on_time, .. } => {
            let stars = "*".repeat(*grade as usize) + &"-".repeat(3usize.saturating_sub(*grade as usize));
            let key = if *on_time { "stamp.graded_on_time" } else { "stamp.graded" };
            let word = format!("stamp.grade.{}", (*grade).min(3));
            cues.push(stamp(g, key, &[("word", Arg::Key(TextKey::new(&word))), ("stars", Arg::Text(stars))], if *grade >= 2 { Tone::Good } else { Tone::Info }, 0.3));
        }
        WorldEvent::Violation { kind, .. } => {
            let key = match kind {
                ViolationKind::NoClearance => "stamp.violation.no_clearance",
                ViolationKind::WrongPad => "stamp.violation.wrong_pad",
                ViolationKind::Speeding => "stamp.violation.speeding",
                ViolationKind::FledScan => "stamp.violation.fled",
            };
            cues.push(stamp(g, key, &[], Tone::Bad, 0.0));
            cues.push(Cue::Kick(0.25));
        }
        WorldEvent::TrackChanged { track, delta, .. } if track.as_str() == WALLET && *delta != 0 => cues.push(Cue::Money(*delta)),
        WorldEvent::HeatStars { stars } => {
            let up = *stars > g.feedback.stars;
            g.feedback.stars = *stars;
            cues.push(Cue::Heat { up });
        }
        WorldEvent::ScanStarted { .. } => {
            cues.push(stamp(g, "stamp.scan", &[], Tone::Bad, 0.0));
            cues.push(Cue::Kick(0.15));
        }
        WorldEvent::ScanResolved { choice, confiscate, fine, .. } => {
            let money = Arg::Number(fine.abs());
            let (key, tone) = match (choice, confiscate, *fine > 0) {
                (_, true, _) => ("stamp.verdict.confiscated", Tone::Bad),
                (ScanChoice::Flee, _, _) => ("stamp.verdict.fled", Tone::Bad),
                (ScanChoice::Bribe, _, _) => ("stamp.verdict.bribed", Tone::Gold),
                (ScanChoice::Talk, _, false) => ("stamp.verdict.talked", Tone::Good),
                (_, _, true) => ("stamp.verdict.fined", Tone::Bad),
                _ => ("stamp.verdict.clean", Tone::Good),
            };
            cues.push(stamp(g, key, &[("money", money)], tone, VERDICT_HOLD));
            if *confiscate {
                cues.push(Cue::Burst { at: Anchor::Ship, tone: Tone::Bad });
            }
            if matches!(choice, ScanChoice::Flee) {
                cues.push(Cue::Kick(0.4));
            }
        }
        WorldEvent::Brewed { station, product, crates, discovered } => {
            let (_, key, tone) = QUALITY_TIERS.iter().find(|(min, _, _)| product.quality >= *min).copied().unwrap_or(QUALITY_TIERS[3]);
            cues.push(stamp(g, "stamp.brewed", &[("word", Arg::Key(TextKey::new(key))), ("q", Arg::Number((product.quality * 100.0).round() as i64)), ("n", Arg::Number(*crates as i64))], tone, 0.1));
            cues.push(Cue::Burst { at: Anchor::Kettle(station.clone()), tone });
            if *discovered {
                cues.push(stamp(g, "stamp.recipe", &[], Tone::Gold, 1.0));
            }
        }
        WorldEvent::GoodsBought { commodity, amount, .. } => {
            let name = g.kernel.commodities.get(commodity).map(|c| Arg::Key(c.record.name.clone())).unwrap_or_else(|| Arg::Text(commodity.to_string()));
            cues.push(stamp(g, "stamp.bought", &[("n", Arg::Number(*amount as i64)), ("what", name)], Tone::Good, 0.0));
        }
        WorldEvent::CounterAnswered { accepted, price, .. } => {
            let (key, tone) = if *accepted { ("stamp.counter_yes", Tone::Good) } else { ("stamp.counter_no", Tone::Bad) };
            cues.push(stamp(g, key, &[("money", Arg::Number(*price))], tone, 0.4));
        }
        WorldEvent::ProductOrderSettled { paid, .. } if *paid > 0 => {
            // The payday as a banner that counts up, like a job's payout.
            g.notify(Notice::new(NoticeKind::Completed, "deal.paid").arg("money", Arg::Number(*paid)));
        }
        WorldEvent::ProductOrderPlaced { by, amount, price, .. } => {
            let name = customer_name(g, by);
            g.notify(Notice::new(NoticeKind::Available, "deal.order").arg("customer", name).arg("n", Arg::Number(*amount as i64)).arg("money", Arg::Number(*price)));
        }
        WorldEvent::CustomerReferred { new, .. } => {
            let name = customer_name(g, new);
            g.notify(Notice::new(NoticeKind::Available, "deal.referred").arg("customer", name));
        }
        WorldEvent::CustomerPoached { by } => {
            let name = customer_name(g, by);
            g.notify(Notice::new(NoticeKind::Warning, "deal.poached").arg("customer", name));
        }
        WorldEvent::RankReached { ladder, rank } => {
            let name = g.smuggling.ladders.ladders.get(ladder).and_then(|l| l.record.ranks.get((*rank as usize).saturating_sub(1))).map(|r| Arg::Key(r.name.clone())).unwrap_or_else(|| Arg::Number(*rank as i64));
            cues.push(stamp(g, "stamp.rank", &[("name", name)], Tone::Gold, 0.5));
        }
        WorldEvent::AirspaceEntered { town } => {
            let name = g.smuggling.town(town).map(|t| Arg::Key(t.name.clone())).unwrap_or_else(|| Arg::Text(town.to_string()));
            let key = if g.smuggling.town(town).is_some_and(|t| t.free_port) { "stamp.airspace_free" } else { "stamp.airspace" };
            cues.push(stamp(g, key, &[("town", name)], Tone::Info, 0.0));
        }
        _ => {}
    }
    for c in cues {
        g.feedback.push(at, c);
    }
}

fn pad_name(g: &Gameplay, pad: &LocationId) -> Arg {
    g.kernel.locations.get(pad).map(|l| Arg::Key(l.record.name.clone())).unwrap_or_else(|| Arg::Text(pad.to_string()))
}

fn customer_name(g: &Gameplay, id: &str) -> Arg {
    g.customer_content.get(&customers_core::CustomerId::new(id)).map(|c| Arg::Key(c.name.clone())).unwrap_or_else(|| Arg::Text(id.to_string()))
}

/// Whether a cue is felt, not read (the test that every player action answers with one).
pub fn felt(c: &Cue) -> bool {
    !matches!(c, Cue::Stamp { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::Crates;
    use gameplay_core::{EffectId, ProductSpec, TownId, TrackId};

    fn cues_for(g: &mut Gameplay, ev: WorldEvent) -> Vec<Cue> {
        let n = g.feedback.cues.len();
        on_world(g, &ev);
        g.feedback.cues[n..].iter().map(|(_, c)| c.clone()).collect()
    }

    /// Every player action of the smuggling run answers with something felt, not only a line:
    /// a kick, dust, a burst, a wallet delta, a heat punch.
    #[test]
    fn every_player_action_answers_with_a_felt_cue() {
        let mut g = Gameplay::load(&Crates::default());
        let town = TownId::new("bent_spoon");
        let at = LocationId::new("bent_spoon_east");
        let product = ProductSpec { base: gameplay_core::CommodityId::new("blorp_paste"), effects: [EffectId::new("giggly")].into_iter().collect(), quality: 0.93 };
        let cases = vec![
            ("touchdown", WorldEvent::Landed { at: Some(at.clone()), speed: 2.0 }),
            ("violation", WorldEvent::Violation { town: town.clone(), kind: ViolationKind::WrongPad }),
            ("fine or pay", WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta: -120, player: None }),
            ("heat", WorldEvent::HeatStars { stars: 2 }),
            ("scan", WorldEvent::ScanStarted { scan: 1, at: at.clone(), visible: 2, hidden: 0 }),
            ("confiscated", WorldEvent::ScanResolved { scan: 1, choice: ScanChoice::Comply, confiscate: true, fine: 50, heat: 0.0 }),
            ("fled", WorldEvent::ScanResolved { scan: 2, choice: ScanChoice::Flee, confiscate: false, fine: 0, heat: 2.0 }),
            ("brewed", WorldEvent::Brewed { station: "kettle_1".into(), product, crates: 2, discovered: true }),
        ];
        for (what, ev) in cases {
            let cues = cues_for(&mut g, ev);
            assert!(cues.iter().any(felt), "{what}: only text: {cues:?}");
        }
    }

    #[test]
    fn verdicts_and_grades_are_stamped_with_their_words() {
        let mut g = Gameplay::load(&Crates::default());
        let town = TownId::new("bent_spoon");
        let at = LocationId::new("bent_spoon_east");
        let s = |cues: Vec<Cue>| cues.into_iter().find_map(|c| if let Cue::Stamp { text, hold, .. } = c { Some((text, hold)) } else { None }).expect("a stamp");
        let (t, hold) = s(cues_for(&mut g, WorldEvent::ScanResolved { scan: 1, choice: ScanChoice::Bribe, confiscate: false, fine: 240, heat: 0.0 }));
        assert_eq!(t, "BRIBED -240 cr");
        assert_eq!(hold, VERDICT_HOLD, "the verdict waits a beat");
        assert_eq!(s(cues_for(&mut g, WorldEvent::ScanResolved { scan: 2, choice: ScanChoice::Comply, confiscate: false, fine: 0, heat: 0.0 })).0, "CLEAN");
        assert_eq!(s(cues_for(&mut g, WorldEvent::LandingGraded { town: town.clone(), at: at.clone(), grade: 3, on_time: true })).0, "BUTTER *** - ON TIME");
        let (t, _) = s(cues_for(&mut g, WorldEvent::ClearanceGranted { town, pad: at, window_s: 90.0, inspection: false }));
        assert!(t.starts_with("PAD ") && t.ends_with(" - 90 s") && !t.contains("bent_spoon_east"), "the pad by its name: {t}");
    }

    #[test]
    fn a_rank_names_itself_and_an_airspace_its_town() {
        let mut g = Gameplay::load(&Crates::default());
        let s = |cues: Vec<Cue>| cues.into_iter().find_map(|c| if let Cue::Stamp { text, .. } = c { Some(text) } else { None }).expect("a stamp");
        let rank = s(cues_for(&mut g, WorldEvent::RankReached { ladder: crate::smuggling::systems::LADDER.into(), rank: 1 }));
        assert!(rank.starts_with("RANK UP: ") && !rank.ends_with(": 1") && !rank.contains("rank."), "{rank}");
        let air = s(cues_for(&mut g, WorldEvent::AirspaceEntered { town: TownId::new("bent_spoon") }));
        assert!(air.ends_with(" AIRSPACE") && !air.contains("bent_spoon"), "{air}");
        let free = s(cues_for(&mut g, WorldEvent::AirspaceEntered { town: TownId::new("gulch_freeport") }));
        assert!(free.ends_with("FREE PORT"), "{free}");
    }

    #[test]
    fn heat_cues_go_up_and_down_and_the_goal_pulses_only_on_change() {
        let mut g = Gameplay::load(&Crates::default());
        assert_eq!(cues_for(&mut g, WorldEvent::HeatStars { stars: 2 }), vec![Cue::Heat { up: true }]);
        assert_eq!(cues_for(&mut g, WorldEvent::HeatStars { stars: 1 }), vec![Cue::Heat { up: false }]);
        let mut f = Feedback::default();
        f.goal_line(0.0, "Next: a");
        f.goal_line(1.0, "Next: a");
        assert!(f.cues.is_empty(), "the first line and repeats do not pulse");
        f.goal_line(2.0, "Next: b");
        assert_eq!(f.cues, vec![(2.0, Cue::Goal)]);
    }

    #[test]
    fn a_payday_is_a_banner_that_counts_up() {
        let mut g = Gameplay::load(&Crates::default());
        on_world(&mut g, &WorldEvent::ProductOrderSettled { order: gameplay_core::OrderId(7), paid: 328, matched: 1.0 });
        let shown = g.notices.tick(0.1);
        let s = shown.iter().find(|s| s.notice.key.as_str() == "deal.paid").expect("the payday notice");
        assert!(s.banner, "a banner, not a toast");
        assert!(s.notice.args.iter().any(|(k, a)| k == "money" && *a == Arg::Number(328)));
    }
}
