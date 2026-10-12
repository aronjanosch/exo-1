//! The comms display (placeholder): a hail list and one canned dialogue per contact with
//! choices. The captain makes the final choice (PULSAR: the crew may vote; later). Paying the
//! freighter's toll spends from the captain's available budget; refusing it sets the threat flag
//! that raises the automatic yellow alert.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::gameplay::HOST;
use display_core::{button, column, label, row, space, title, Panel, Tone};

pub const ID: PanelId = PanelId::new("captain_comms");
pub const NAME: &str = "Comms";

/// The toll the freighter asks (credits). TODO(initiator): a placeholder.
pub const TOLL: i64 = 200;

/// Contacts in range: name, what they say, the choices (label, answer). Invented.
pub const CONTACTS: [(&str, &str, &[(&str, &str)]); 3] = [
    ("Hearth Tower", "Hearth Tower here. Pad main is free. Please do not land on the snack stand again.", &[("ACKNOWLEDGE", "Tower: Lovely. Mind the stand.")]),
    (
        "Sticky Wicket (melon freighter)",
        "This is the Sticky Wicket. This lane is ours. Pay 200 credits or eat melon.",
        &[("PAY 200", "Freighter: Pleasure doing business. Enjoy the lane."), ("REFUSE", "Freighter: Then melons it is. Shields up, friends."), ("TELL A JOKE", "Freighter: ...that was terrible. We are leaving out of pity.")],
    ),
    ("Lost Space Llama", "Mmmmh. (It is a llama. In space. It looks lost.)", &[("WAVE", "The llama waves back. Probably.")]),
];

#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct CommsUi {
    /// The contact on the line.
    pub open: Option<usize>,
    /// The last answer.
    pub answer: Option<String>,
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(console(ID, NAME, 7.0, [0.3, 0.26, 0.45]));
    app.init_resource::<CommsUi>();
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects).after(super::overview::read_clicks));
}

pub fn panel(ui: &CommsUi, hostile: bool) -> Panel {
    let mut left = vec![title("COMMS").fixed(40.0), label("IN RANGE").tone(Tone::Dim).fixed(22.0)];
    for (i, (name, ..)) in CONTACTS.iter().enumerate() {
        left.push(button(pick(&format!("HAIL {name}"), ui.open == Some(i)), format!("hail:{i}")).fixed(44.0));
    }
    left.push(space());
    if hostile {
        left.push(label("A contact is hostile.").tone(Tone::Bad).fixed(26.0));
    }
    let mut right = vec![];
    match ui.open {
        None => right.push(label("Hail a contact.").tone(Tone::Dim).fixed(30.0)),
        Some(i) => {
            let (name, says, choices) = CONTACTS[i];
            right.push(title(name).fixed(40.0));
            right.push(label(says).fixed(60.0));
            right.push(label("The captain answers (crew votes come later):").tone(Tone::Dim).fixed(24.0));
            for (k, (choice, _)) in choices.iter().enumerate() {
                right.push(button(*choice, format!("say:{i}:{k}")).fixed(44.0));
            }
            if let Some(a) = &ui.answer {
                right.push(label(a.clone()).tone(Tone::Good).fixed(30.0));
            }
        }
    }
    right.push(space());
    right.push(button("CLOSE TRANSMISSION", "close").fixed(44.0));
    Panel::new(PANEL_PX[0], PANEL_PX[1], row(28.0, vec![column(8.0, left).flex(1.0), column(8.0, right).flex(1.4)]))
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut ui: ResMut<CommsUi>, mut crew: ResMut<CrewState>, mut gp: ResMut<Gameplay>, role: Res<DevRole>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        let parts: Vec<&str> = c.action.split(':').collect();
        match parts.as_slice() {
            ["hail", i] => {
                ui.open = i.parse::<usize>().ok().filter(|i| *i < CONTACTS.len());
                ui.answer = None;
            }
            ["close"] => *ui = CommsUi::default(),
            ["say", i, k] => {
                let (Ok(i), Ok(k)) = (i.parse::<usize>(), k.parse::<usize>()) else { continue };
                let Some((choice, answer)) = CONTACTS.get(i).and_then(|c| c.2.get(k)) else { continue };
                if role.0 != Role::Captain {
                    refuse(&mut gp, &Refusal::NotCaptain);
                    continue;
                }
                match *choice {
                    "PAY 200" => match crew.budget.try_spend(Role::Captain, TOLL, gp.progress.wallet()) {
                        Ok(e) => {
                            gp.push_world(HOST, e);
                            crew.hostile = false;
                        }
                        Err(r) => {
                            refuse(&mut gp, &r);
                            continue;
                        }
                    },
                    "REFUSE" => crew.hostile = true,
                    "TELL A JOKE" => crew.hostile = false,
                    _ => {}
                }
                ui.answer = Some(answer.to_string());
            }
            _ => warn!("comms: unknown action {c:?}"),
        }
    }
}

pub fn write_panel(ui: Res<CommsUi>, crew: Res<CrewState>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&ui, crew.hostile));
}
