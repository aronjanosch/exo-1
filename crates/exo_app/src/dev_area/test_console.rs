//! The test console (#285): a box at the lab zone's front edge, facing the pads, with a display
//! that proves the loop, and the pattern a role module follows. A counter with - and + (the minus is disabled at
//! zero, the plus at the top), a bar that fills with the counter, a toggle and a slider. Its two
//! systems are what a role writes: one builds the panel from the state, one reads the clicks.
use super::*;
use crate::displays::{DisplayAction, Panels};
use display_core::{bar, button, column, label, row, slider, space, title, toggle, Panel, Tone};

pub const ID: PanelId = PanelId::new("test_console");
pub const NAME: &str = "Test console";
/// The counter's top (the bar is full there).
pub const MAX: u32 = 10;
/// Where it stands in the lab zone (m from the zone's centre): at its south edge, facing the walkway
/// and the pads.
pub const AT: [f64; 2] = [0.0, -4.5];

/// The console's state.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TestConsole {
    pub count: u32,
    pub lights: bool,
    pub trim: f64,
}

impl Default for TestConsole {
    fn default() -> Self {
        TestConsole { count: 0, lights: false, trim: 0.5 }
    }
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "test_console".into(),
        zone: LAB,
        local_pos: AT,
        size: [1.6, 1.0, 0.8],
        turn_deg: 0.0,
        colour: [0.32, 0.36, 0.42],
        panel: Some(PanelSpec { id: ID, name: NAME.into(), size_m: [1.0, 0.75], at: [0.0, 1.45, 0.2], tilt_deg: 15.0 }),
    });
    app.init_resource::<TestConsole>();
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects));
}

/// What the console shows (480 x 360 panel pixels).
pub fn panel(s: &TestConsole) -> Panel {
    Panel::new(
        480.0,
        360.0,
        column(
            10.0,
            vec![
                title("TEST CONSOLE").fixed(40.0),
                label("Dev area, lab zone").tone(Tone::Dim).fixed(22.0),
                row(10.0, vec![button("-", "dec").enabled(s.count > 0).fixed(90.0), label(format!("COUNT  {}", s.count)), button("+", "inc").enabled(s.count < MAX).fixed(90.0)]).fixed(56.0),
                bar(s.count as f64 / MAX as f64, "LOAD").thresholds(Some(0.7), Some(0.9)).fixed(34.0),
                toggle(s.lights, "LIGHTS", "lights").fixed(44.0),
                slider(s.trim, "TRIM", "trim").fixed(34.0),
                space(),
            ],
        ),
    )
}

/// The clicks on this panel change the state.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut s: ResMut<TestConsole>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        match (c.action.as_str(), c.value) {
            ("inc", _) => s.count = (s.count + 1).min(MAX),
            ("dec", _) => s.count = s.count.saturating_sub(1),
            ("lights", Some(v)) => s.lights = v > 0.5,
            ("trim", Some(v)) => s.trim = v as f64,
            _ => warn!("test console: unknown action {c:?}"),
        }
    }
}

/// The panel from the state, every tick (`Panels::set` skips an unchanged one).
pub fn write_panel(s: Res<TestConsole>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&s));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_stop_at_the_ends_and_the_bar_follows_the_counter() {
        let p = panel(&TestConsole::default());
        assert!(matches!(p.find("dec").unwrap().widget, display_core::Widget::Button { enabled: false, .. }));
        let p = panel(&TestConsole { count: MAX, ..default() });
        assert!(matches!(p.find("inc").unwrap().widget, display_core::Widget::Button { enabled: false, .. }));
        let p = panel(&TestConsole { count: 3, ..default() });
        let bar = p.layout().into_iter().find_map(|w| if let display_core::Widget::Bar { value, .. } = w.widget { Some(*value) } else { None });
        assert!((bar.unwrap() - 0.3).abs() < 1e-9);
        assert!(p.layout().iter().all(|w| w.rect.max().y <= p.size.y - p.padding + 1e-9), "everything fits");
    }
}
