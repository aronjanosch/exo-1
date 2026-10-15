//! What the lab's panels show: pure functions of the state (560 px wide; heights in `DEVICES`).
use super::*;
use display_core::{bar, button, column, label, row, slider, space, title, Item as Ui, Panel};

/// What a panel needs to draw: the lab, the hands, the skill and the text lookups.
pub struct View<'a> {
    pub s: &'a ScienceLab,
    pub hands: &'a Hands,
    pub medical: f64,
    pub names: &'a dyn Fn(&CommodityId) -> String,
    pub effects: &'a dyn Fn(&Batch) -> String,
}

const GAP: f64 = 6.0;

/// One held or stored item in a line.
pub fn item_label(item: &Item<LabItem>, names: &dyn Fn(&CommodityId) -> String) -> String {
    match &item.payload {
        Some(LabItem::Ingredient(id)) => names(id),
        Some(LabItem::Batch(b)) => format!("batch {} q{:.2} ({:.1})", names(&b.base), b.quality, b.volume),
        Some(LabItem::Syringe(y)) => format!("syringe {} q{:.2} ({})", names(&y.product.base), y.product.quality, y.doses),
        Some(LabItem::Key) => "ID card".into(),
        Some(LabItem::FirstAid(kind)) => kind.clone(),
        None => item.kind.to_string(),
    }
}

/// A short summary of the hands.
pub fn hands_line(h: &Hands) -> String {
    let (mut batches, mut syringes, mut goods) = (0, 0, 0);
    for i in 0..h.0.len() {
        if let Some(s) = h.0.slot(i) {
            match s.item.payload {
                Some(LabItem::Batch(_)) => batches += s.count,
                Some(LabItem::Syringe(_)) => syringes += s.count,
                Some(LabItem::Ingredient(_)) => goods += s.count,
                _ => {}
            }
        }
    }
    let used = h.0.capacity().used_slots;
    format!("HANDS {batches} batch  {syringes} syringe  {goods} goods  {used}/{}", h.0.len())
}

fn message(v: &View, id: &PanelId) -> Ui {
    let (text, tone) = v.s.lines.get(id).cloned().unwrap_or_default();
    label(text).tone(tone).fixed(22.0)
}

fn footer(v: &View, id: &PanelId) -> Vec<Ui> {
    vec![message(v, id), label(hands_line(v.hands)).tone(Tone::Dim).fixed(20.0), space()]
}

fn batch_line(v: &View, b: Option<&Batch>) -> String {
    match b {
        None => "empty: put a batch in".into(),
        Some(b) => format!("{} q {:.2}, {:.1} doses, {:.0} C{}", (v.names)(&b.base), b.quality, b.volume, b.temp_c, if b.burnt { ", burnt" } else { "" }),
    }
}

fn put_take(has: bool, hands_batch: bool) -> Ui {
    row(8.0, vec![button("put batch", "put").enabled(!has && hands_batch), button("take batch", "take").enabled(has)]).fixed(40.0)
}

/// A marker under a 0..1 bar: `text` around the middle of `from..to` (at least 16 % wide, so the
/// text does not wrap).
fn marker(from: f64, to: f64, text: &str) -> Ui {
    let (mid, half) = ((from + to) * 0.5, ((to - from) * 0.5).max(0.08));
    let (a, b) = ((mid - half).clamp(0.0, 1.0), (mid + half).clamp(0.0, 1.0));
    row(0.0, vec![space().flex(a), label(text).tone(Tone::Good).flex((b - a).max(0.01)), space().flex((1.0 - b).max(0.0))]).fixed(18.0)
}

fn speed(v: &View) -> f64 {
    skill::speed(v.medical, v.s.tuning.skill.required, &v.s.tuning.skill)
}

/// The mixer: base and ingredients from the hands, start, the needle and its release, the batch.
pub fn mixer_panel(v: &View) -> Panel {
    let s = v.s;
    let k = s.kettle();
    let status = match &k {
        Kettle::Empty => "empty: a base, then up to 3 goods".to_string(),
        Kettle::Loaded { base, ingredients } => {
            let mut parts = vec![(v.names)(base)];
            parts.extend(ingredients.iter().map(|i| (v.names)(i)));
            format!("in it: {}", parts.join(" + "))
        }
        Kettle::Brewing { remaining_s, .. } => format!("brewing, {remaining_s:.1} s left"),
        Kettle::Done { .. } => "done: release at the sweet spot".into(),
        Kettle::Ready { product, .. } => format!("ready: quality {:.2}, take the batch", product.quality),
    };
    let can_add = matches!(k, Kettle::Empty | Kettle::Loaded { .. });
    let mut items = vec![
        title("MIXER").fixed(36.0),
        label(status).fixed(22.0),
        label(format!("medical skill {:.1}   brew {:.1} s", v.medical, s.lc.tuning.brew_s / speed(v))).tone(Tone::Dim).fixed(20.0),
        label("FROM YOUR HANDS").tone(Tone::Dim).fixed(20.0),
    ];
    let goods: Vec<usize> = (0..v.hands.0.len()).filter(|&i| matches!(v.hands.0.slot(i).and_then(|x| x.item.payload.as_ref()), Some(LabItem::Ingredient(_)))).take(5).collect();
    if goods.is_empty() {
        items.push(label("nothing to add: take goods from the ingredient shelf").tone(Tone::Dim).fixed(30.0));
    }
    for i in goods {
        let st = v.hands.0.slot(i).unwrap();
        items.push(row(8.0, vec![label(format!("{} x{}", item_label(&st.item, v.names), st.count)), button("add", format!("add:{i}")).enabled(can_add).fixed(100.0)]).fixed(30.0));
    }
    let needle = if matches!(k, Kettle::Done { .. }) { production_core::needle(s.needle_t, s.lc.tuning.needle_period_s) } else { 0.0 };
    let (sweet, w) = (s.lc.tuning.sweet_spot, s.lc.tuning.sweet_width * 0.4);
    items.extend([
        button("start brew", "start").enabled(matches!(k, Kettle::Loaded { .. })).fixed(40.0),
        bar(needle, "NEEDLE").fixed(30.0),
        marker(sweet - w, sweet + w, "^sweet"),
        row(8.0, vec![button("release", "release").enabled(matches!(k, Kettle::Done { .. })), button("take batch", "take").enabled(matches!(k, Kettle::Ready { .. }))]).fixed(40.0),
    ]);
    items.extend(footer(v, &MIXER));
    Panel::new(PANEL_W, 560.0, column(GAP, items))
}

/// The heater: setpoint, the batch's temperature against the band, the hold, the burn.
pub fn heater_panel(v: &View) -> Panel {
    let s = v.s;
    let (h, t) = (&s.heater, &s.tuning.heater);
    let (lo, hi) = (s.tuning.ambient_c, t.max_setpoint_c);
    let b = h.batch();
    let temp = h.temperature().unwrap_or(s.tuning.ambient_c);
    let need = t.hold_s / speed(v);
    let heat = b.map(|b| b.heat).unwrap_or(0.0);
    let (state, tone) = match b {
        None => ("".to_string(), Tone::Dim),
        Some(b) if b.burnt => ("BURNT".into(), Tone::Bad),
        Some(b) if b.heat >= 1.0 => ("heated through".into(), Tone::Good),
        Some(_) if temp > t.band_high_c => ("too hot: over the band!".into(), Tone::Warn),
        Some(_) if temp >= t.band_low_c => ("in the band: hold it".into(), Tone::Good),
        Some(_) => ("warming".into(), Tone::Normal),
    };
    let mut items = vec![
        title("HEATER").fixed(36.0),
        label(batch_line(v, b)).fixed(22.0),
        slider((h.setpoint() - lo) / (hi - lo), format!("SETPOINT {:.0} C", h.setpoint()), "setpoint").fixed(36.0),
        label(format!("element {:.0} C  band {:.0}-{:.0}  burns >{:.0}", h.element(), t.band_low_c, t.band_high_c, t.burn_c)).tone(Tone::Dim).fixed(20.0),
        bar(temp / hi, format!("BATCH {temp:.1} C")).thresholds(Some(t.band_high_c / hi), Some(t.burn_c / hi)).fixed(30.0),
        marker(t.band_low_c / hi, t.band_high_c / hi, "^band"),
        bar(heat, format!("HOLD {:.1} / {:.1} s", heat * need, need)).fixed(30.0),
        label(state).tone(tone).fixed(22.0),
        put_take(b.is_some(), v.hands.find(|x| matches!(x, LabItem::Batch(_))).is_some()),
    ];
    items.extend(footer(v, &HEATER));
    Panel::new(PANEL_W, 520.0, column(GAP, items))
}

/// The centrifuge: speed and time, start and stop, separation and wobble.
pub fn centrifuge_panel(v: &View) -> Panel {
    let s = v.s;
    let (c, t) = (&s.centrifuge, &s.tuning.centrifuge);
    let b = c.batch();
    let spinning = c.state() == Spin::Spinning;
    let (state, tone) = match c.state() {
        Spin::Idle => ("idle".to_string(), Tone::Dim),
        Spin::Spinning => (format!("spinning, {:.1} s left", c.left_s()), Tone::Normal),
        Spin::Done => ("stopped".into(), Tone::Normal),
        Spin::Tripped => ("TRIPPED: the drum wobbled too hard".into(), Tone::Bad),
    };
    let rpm = if spinning { c.rpm() } else { s.spin_rpm };
    let mut items = vec![
        title("CENTRIFUGE").fixed(36.0),
        label(batch_line(v, b)).fixed(22.0),
        slider(rpm / t.max_rpm, format!("SPEED {rpm:.0} rpm"), "rpm").fixed(36.0),
        label(format!("sweet {:.0}-{:.0}  wobble >{:.0}  sep {:.1} s", t.sweet_low_rpm, t.sweet_high_rpm, t.wobble_rpm, t.separate_s / speed(v))).tone(Tone::Dim).fixed(20.0),
        slider(s.spin_s / MAX_SPIN_S, format!("TIME {:.1} s", s.spin_s), "time").fixed(36.0),
        row(8.0, vec![button("start", "start").enabled(b.is_some() && !spinning), button("stop", "stop").enabled(spinning)]).fixed(40.0),
        bar(b.map(|b| b.separation).unwrap_or(0.0), "SEPARATED").fixed(30.0),
        bar(c.wobble(), "WOBBLE").thresholds(Some(0.5), Some(0.8)).fixed(30.0),
        label(state).tone(tone).fixed(22.0),
        put_take(b.is_some() && !spinning, v.hands.find(|x| matches!(x, LabItem::Batch(_))).is_some()),
    ];
    items.extend(footer(v, &CENTRIFUGE));
    Panel::new(PANEL_W, 560.0, column(GAP, items))
}

/// The syringe filler: the batch, its quality and doses, fill.
pub fn filler_panel(v: &View) -> Panel {
    let s = v.s;
    let b = s.filler.as_ref();
    let fx = b.map(|b| (v.effects)(b)).unwrap_or_default();
    let cap = s.tuning.syringe.capacity;
    let mut items = vec![
        title("SYRINGE FILLER").fixed(36.0),
        label(batch_line(v, b)).fixed(22.0),
        label(if b.is_some() { format!("does: {fx}") } else { String::new() }).tone(Tone::Dim).fixed(20.0),
        button(format!("fill a syringe ({cap} doses)"), "fill").enabled(b.is_some_and(|b| b.volume >= 1.0) && v.hands.has_free_slot()).fixed(40.0),
        put_take(b.is_some(), v.hands.find(|x| matches!(x, LabItem::Batch(_))).is_some()),
        button("pour out the rest", "discard").enabled(b.is_some()).fixed(36.0),
    ];
    items.extend(footer(v, &FILLER));
    Panel::new(PANEL_W, 420.0, column(GAP, items))
}

/// The test dummy: inject a syringe from the hands, the last dose.
pub fn dummy_panel(v: &View) -> Panel {
    let has = v.hands.find(|x| matches!(x, LabItem::Syringe(y) if y.doses > 0)).is_some();
    let mut items = vec![
        title("TEST DUMMY").fixed(36.0),
        label("a stand-in: the medbay's bed takes real doses").tone(Tone::Dim).fixed(20.0),
        button("inject a syringe", "inject").enabled(has).fixed(40.0),
        label(format!("medical skill {:.1}", v.medical)).tone(Tone::Dim).fixed(20.0),
    ];
    items.extend(footer(v, &DUMMY));
    Panel::new(PANEL_W, 360.0, column(GAP, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab() -> ScienceLab {
        let files = crate::content_files("gameplay");
        let kernel = gameplay_core::Content::load(&files, &["small", "medium", "large"]).unwrap();
        ScienceLab::new(LabContent::load(&files, &kernel).unwrap(), DeviceTuning::load(&files, &kernel).unwrap())
    }

    #[test]
    fn the_mixer_offers_held_goods_and_buttons_follow_the_kettle() {
        let s = lab();
        let mut hands = Hands::default();
        hands.give(ingredient_item(&CommodityId::new("dream_goo")));
        let names = |id: &CommodityId| id.to_string();
        let fx = |_: &Batch| String::new();
        let v = View { s: &s, hands: &hands, medical: 50.0, names: &names, effects: &fx };
        let p = mixer_panel(&v);
        assert!(matches!(p.find("add:0").unwrap().widget, display_core::Widget::Button { enabled: true, .. }));
        assert!(matches!(p.find("start").unwrap().widget, display_core::Widget::Button { enabled: false, .. }));
        assert!(matches!(p.find("release").unwrap().widget, display_core::Widget::Button { enabled: false, .. }));
    }
}
