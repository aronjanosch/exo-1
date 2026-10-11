//! Round 6, the smuggling run: the glue for traffic (towers, clearance), heat (scans), the lab
//! (kettles), buyers' product orders and the family ladder with its shops. The `*_core` crates
//! hold the rules; this module feeds them the world (where the ship is, what lands where, which
//! crates are aboard) and turns their outcomes into crates, radio lines and the HUD.
//!
//! - `systems`: the round-6 states and contents, routed from `gameplay::gameplay_step`.
//! - `hud`: the window part (goal line, heat, radio, phone, scan prompt, needle).
pub mod glue;
pub mod hud;
pub mod systems;

use bevy::prelude::*;

/// A line on the radio: who speaks and what, at which game time.
#[derive(Clone, Debug, PartialEq)]
pub struct RadioLine {
    pub who: String,
    pub text: String,
    pub at: f64,
}

/// The phone's tabs, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneTab {
    Goals,
    Orders,
    Contacts,
    Recipes,
    Heat,
    Shop,
}

impl PhoneTab {
    pub const ALL: [PhoneTab; 6] = [PhoneTab::Goals, PhoneTab::Orders, PhoneTab::Contacts, PhoneTab::Recipes, PhoneTab::Heat, PhoneTab::Shop];
    pub fn title(self) -> &'static str {
        match self {
            PhoneTab::Goals => "Goals",
            PhoneTab::Orders => "Orders",
            PhoneTab::Contacts => "Contacts",
            PhoneTab::Recipes => "Recipes",
            PhoneTab::Heat => "Heat",
            PhoneTab::Shop => "Shop",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).expect("listed")
    }
}

/// The phone as shown: open or not, the tab, its rows and the selected one, a key hint.
#[derive(Clone, Debug, PartialEq)]
pub struct PhoneView {
    pub open: bool,
    pub tab: PhoneTab,
    pub rows: Vec<String>,
    pub selected: usize,
    pub hint: String,
}

impl Default for PhoneView {
    fn default() -> Self {
        PhoneView { open: false, tab: PhoneTab::Goals, rows: Vec::new(), selected: 0, hint: String::new() }
    }
}

/// The kettle's timing moment as shown: needle position, sweet spot and half width (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrewView {
    pub needle: f64,
    pub sweet: f64,
    pub width: f64,
}

/// Everything the round-6 HUD shows; the fixed step fills it (headless too, for scenarios).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct SmuggleHud {
    /// "Next: ..." from the family ladder.
    pub goal_line: String,
    pub heat_stars: u8,
    pub heat_points: f64,
    /// Every radio line so far, oldest first (the window shows the recent ones).
    pub radio: Vec<RadioLine>,
    /// Game clock now (s), for the radio's fade.
    pub clock: f64,
    pub phone: PhoneView,
    /// The scan prompt while a scan waits for an answer; empty otherwise.
    pub scan: String,
    pub brew: Option<BrewView>,
    /// The tower's state for the pilot: airspace, calling, holding, cleared pad and time left.
    pub clearance: String,
    /// The countdown under it (round 6b): share of the time left (0..1), and whose it is.
    pub bar: Option<HudBar>,
}

/// A countdown shown as a shrinking bar: the clearance window or a patrol's halt order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudBar {
    pub left: f64,
    pub tone: crate::feedback::Tone,
}

/// How long a radio line stays up (s). TODO(initiator).
pub const RADIO_S: f64 = 9.0;

impl SmuggleHud {
    pub fn say(&mut self, who: impl Into<String>, text: impl Into<String>) {
        let at = self.clock;
        self.radio.push(RadioLine { who: who.into(), text: text.into(), at });
    }

    /// The last radio lines still on screen, at most three.
    pub fn radio_text(&self) -> String {
        let recent: Vec<&RadioLine> = self.radio.iter().rev().take_while(|l| self.clock - l.at < RADIO_S).take(3).collect();
        recent.iter().rev().map(|l| format!("[{}] {}", l.who, l.text)).collect::<Vec<_>>().join("\n")
    }

    pub fn phone_text(&self) -> String {
        let p = &self.phone;
        if !p.open {
            return String::new();
        }
        let tabs: Vec<String> = PhoneTab::ALL.iter().map(|t| if *t == p.tab { format!("[{}]", t.title()) } else { t.title().to_string() }).collect();
        let mut s = format!("{}\n\n", tabs.join("  "));
        if p.rows.is_empty() {
            s.push_str("(nothing here yet)\n");
        }
        for (i, r) in p.rows.iter().enumerate() {
            s.push_str(if i == p.selected { "> " } else { "  " });
            s.push_str(r);
            s.push('\n');
        }
        if !p.hint.is_empty() {
            s.push('\n');
            s.push_str(&p.hint);
        }
        s
    }
}
