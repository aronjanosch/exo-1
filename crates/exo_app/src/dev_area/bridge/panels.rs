//! The bridge's screens: the shield SH-01, the helm HM-01 and the jump drive JD-01, and the
//! captain's console CP-01 (`captain_screen`). A press asks the host.
use crate::dev_area::*;
use crate::displays::{DisplayAction, Panels};
use crate::dev_area::screens::*;
use crate::items::Items;
use display_core::{button, canvas, column, font, micro, readout, row, slider, space, Ink, Panel, Shape, Tier, TextFace, Tone};
use item_core::wire::{BotStation, JumpStage, ReactorAsk, RoomAsk, ReactorNews};

use crate::dev_area::bridge::captain_screen;
use crate::dev_area::crew::{bot_tagged, manned};

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

pub const SHIELD_ID: PanelId = PanelId::new("shield_panel");

pub const SHIELD_NAME: &str = "SHIELD";

pub const HELM_ID: PanelId = PanelId::new("helm_panel");

pub const HELM_NAME: &str = "HELM";

pub const JUMP_ID: PanelId = PanelId::new("jump_drive");

pub const JUMP_NAME: &str = "JUMP DRIVE";

/// SH-01 (r40, PULSAR's shield generator): the shield's charge, the quantum shield's word (UP while it
/// blocks boarding; DOWN after a drop below its line, for the offline time) and the regen in % a second
/// (0 inside the recovery delay).
pub fn shield_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let bar_w = 200.0;
    let charge_frac = r.shield.clamp(0.0, 1.0);
    let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if charge_frac > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * charge_frac, h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &format!("{:.0} %", charge_frac * 100.0), 16.0, TextFace::Mono, Ink::Fg2, false));
    let charge = column(4.0, vec![micro("SHIELD").fixed(14.0), canvas(292.0, 18.0, s, None).fixed(18.0)]);
    let (word, tone) = if r.quantum_shield { ("QT SHLD UP", Tone::Good) } else { ("QT SHLD DOWN", Tone::Bad) };
    let quantum = readout(word.to_string()).text_size(font::SMALL).tone(tone);
    let regen = readout(format!("REGEN {:.1} %/s", r.shield_regen)).text_size(font::SMALL).tone(Tone::Normal);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![charge, quantum.fixed(20.0), regen.fixed(20.0)]).fixed(120.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, SHIELD_NAME, "SH-01", body)).with_padding(6.0)
}

/// HM-01 (r38, the helm): the throttle slider (-100..100 %, the engines' target) and what the engines
/// make of it: the thrust (%) and the kW they draw of their demand. A short grid shows here as a
/// thrust below the throttle and a draw below the demand.
pub fn helm_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let throttle = column(4.0, vec![micro("THROTTLE").fixed(14.0), slider((r.throttle + 100.0) / 200.0, "", "throttle").fixed(40.0)]);
    let thrust = readout(format!("THRUST {:.0} %", r.engine_thrust)).text_size(font::SMALL).tone(Tone::Normal);
    let draw = readout(format!("ENGINE {:.0} / {:.0} kW", r.delivered_kw[0], r.demand_kw[0])).text_size(font::SMALL).tone(Tone::Normal);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![throttle, thrust.fixed(20.0), draw.fixed(20.0)]).fixed(120.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, HELM_NAME, "HM-01", body)).with_padding(6.0)
}

/// JD-01 (r40, #252, PULSAR's warp drive screen): the stage, the charge, the EM SPIKE warning while
/// it charges, ARM (starts the charge, from cold only) and JUMP (live at READY only). After a jump the
/// stage reads JUMPED while the drive runs down to cold.
pub fn jump_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let (word, tone) = match r.jump_stage {
        JumpStage::ColdStart => ("COLD START", Tone::Dim),
        JumpStage::Prepping => ("PREPPING", Tone::Normal),
        JumpStage::Paused => ("PAUSED", Tone::Warn),
        JumpStage::Ready => ("READY", Tone::Normal),
        JumpStage::Active => ("JUMPED", Tone::Bad),
    };
    let stage = readout(word).text_size(font::SMALL).tone(tone);
    let bar_w = 200.0;
    let pct = format!("{:.0} %", r.jump_charge.clamp(0.0, 1.0) * 100.0);
    let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if r.jump_charge > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * r.jump_charge.clamp(0.0, 1.0), h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &pct, 16.0, TextFace::Mono, Ink::Fg2, false));
    let charge = column(4.0, vec![micro("CHARGE").fixed(14.0), canvas(292.0, 18.0, s, None).fixed(18.0)]);
    let spike = if r.jump_stage == JumpStage::Prepping { readout("EM SPIKE").text_size(font::SMALL).tone(Tone::Bad) } else { readout("").text_size(font::SMALL).tone(Tone::Dim) };
    let arm = button("ARM", "jump_arm").enabled(r.jump_stage == JumpStage::ColdStart);
    let go = button("JUMP", "jump_go").tone(Tone::Bad).enabled(r.jump_stage == JumpStage::Ready);
    let presses = row(8.0, vec![arm.flex(1.0), go.flex(1.0)]).fixed(28.0);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![stage.fixed(20.0), charge, spike.fixed(20.0), presses]).fixed(160.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, JUMP_NAME, "JD-01", body)).with_padding(6.0)
}

/// The presses on the bridge's screens: the helm's throttle, the jump drive's ARM and JUMP, and
/// CP-01 (the doors are the hull's `RoomAsk`s, the rest the host's asks).
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        let ask = if c.panel == HELM_ID {
            match c.action.as_str() {
                // The slider's 0..1 is -100..100 %.
                "throttle" => c.value.map(|v| ReactorAsk::SetThrottle(v as f64 * 200.0 - 100.0)),
                _ => None,
            }
        } else if c.panel == JUMP_ID {
            match c.action.as_str() {
                "jump_arm" => Some(ReactorAsk::JumpArm),
                "jump_go" => Some(ReactorAsk::JumpGo),
                _ => None,
            }
        } else if c.panel == captain_screen::ID {
            match c.action.as_str() {
                captain_screen::DOOR_ACTION => {
                    let open = !items.room.door_open;
                    items.ask_room(RoomAsk::SetDoor(open));
                    None
                }
                captain_screen::BRIDGE_DOOR_ACTION => {
                    let open = !items.room.bridge_door;
                    items.ask_room(RoomAsk::SetBridgeDoor(open));
                    None
                }
                captain_screen::SEAL_ACTION => Some(ReactorAsk::SealAirlock),
                captain_screen::SILENCE_ACTION => Some(ReactorAsk::SilenceAlarms),
                other => captain_screen::parse_bot_action(other).map(|(s, level)| ReactorAsk::SetBotPriority(s, level)),
            }
        } else {
            None
        };
        if let Some(a) = ask {
            items.ask_reactor(a);
        }
    }
}

/// The bridge's panels from the snapshot, every tick.
pub fn write_panels(items: Res<Items>, mut panels: ResMut<Panels>) {
    let r = items.reactor.as_ref();
    show(&mut panels, r, SHIELD_ID, shield_panel(r));
    show(&mut panels, r, HELM_ID, bot_tagged(helm_panel(r), manned(&items, BotStation::Helm)));
    show(&mut panels, r, JUMP_ID, jump_panel(r));
    show(&mut panels, r, captain_screen::ID, captain_screen::captain_panel(r, items.room.door_open, items.room.bridge_door));
}
