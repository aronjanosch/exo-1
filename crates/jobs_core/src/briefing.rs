//! Briefings from parts (#167): title formula, the giver's greeting (by mood) and intro, a
//! paragraph for the shape of the job, a reason keyed to cargo or route, the giver's sign-off.
//! The picker decides every pick, so the same picker state gives the same briefing, and a picker
//! kept across briefings never repeats a line twice in a row.
use gameplay_core::notice::Arg;
use gameplay_core::text::{Picker, TextTable};
use gameplay_core::{Content, TextKey};

use crate::giver::GiverHistory;
use crate::job::Leg;
use crate::template::JobTemplate;
use crate::JobContent;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Briefing {
    /// The giver's name.
    pub giver: String,
    pub title: String,
    pub greeting: String,
    pub intro: String,
    pub paragraph: String,
    pub reason: String,
    pub sign_off: String,
}

impl Briefing {
    /// All parts, one paragraph each, for a panel.
    pub fn text(&self) -> String {
        [&self.greeting, &self.intro, &self.paragraph, &self.reason, &self.sign_off].iter().filter(|s| !s.is_empty()).map(|s| s.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// The shape of a job picks its paragraph: timed, big (4 crates or more) or few.
fn shape(t: &JobTemplate, asked: u32) -> &'static str {
    if t.deadline_s.is_some() {
        "briefing.shape.timed"
    } else if asked >= 4 {
        "briefing.shape.many"
    } else {
        "briefing.shape.few"
    }
}

pub fn briefing(jc: &JobContent, kernel: &Content, table: &TextTable, picker: &mut Picker, t: &JobTemplate, legs: &[Leg], history: &GiverHistory) -> Briefing {
    let giver = t.giver.as_ref().and_then(|g| jc.givers.get(g)).map(|g| &g.record);
    let mut out = Briefing::default();
    let asked: u32 = legs.iter().map(|l| l.amount).sum();
    let first = legs.first();
    let name_of = |p: &mut Picker, k: &TextKey| p.pick(table, k.as_str());
    let fill = |p: &mut Picker, key: &str, args: &[(&str, String)]| {
        let mut s = p.pick(table, key);
        for (n, v) in args {
            s = s.replace(&format!("{{{n}}}"), v);
        }
        s
    };
    out.giver = giver.map(|g| name_of(picker, &g.name)).unwrap_or_default();
    let title = name_of(picker, &t.title);
    out.title = if table.has("briefing.title") && giver.is_some() { fill(picker, "briefing.title", &[("title", title), ("giver", out.giver.clone())]) } else { title };
    if let Some(l) = first {
        let commodity = kernel.commodities.get(&l.commodity).map(|c| name_of(picker, &c.record.name)).unwrap_or_else(|| l.commodity.to_string());
        let place = |p: &mut Picker, id: &gameplay_core::LocationId| kernel.locations.get(id).map(|x| name_of(p, &x.record.name)).unwrap_or_else(|| id.to_string());
        let (from, to) = (place(picker, &l.from), place(picker, &l.to));
        out.paragraph = fill(picker, shape(t, asked), &[("amount", asked.to_string()), ("commodity", commodity), ("from", from), ("to", to)]);
    }
    if let Some(g) = giver {
        out.greeting = name_of(picker, g.voice.greeting.key(history.mood()));
        out.intro = name_of(picker, &g.voice.intro);
        out.sign_off = name_of(picker, &g.voice.sign_off);
        // The reason: the cargo's own, else the destination's route tag, else the giver's.
        let mut reason = None;
        if let Some(l) = first {
            let by_cargo = format!("briefing.reason.commodity.{}", l.commodity);
            if table.has(&by_cargo) {
                reason = Some(by_cargo);
            } else if let Some(dest) = kernel.locations.get(&l.to) {
                reason = dest.record.tags.iter().map(|tag| format!("briefing.reason.route.{tag}")).find(|k| table.has(k));
            }
        }
        out.reason = match reason {
            Some(k) => picker.pick(table, &k),
            None => name_of(picker, &g.voice.reason),
        };
    }
    out
}

/// A text key as an argument (the helper the notices use).
pub fn key_arg(k: &TextKey) -> Arg {
    Arg::Key(k.clone())
}
