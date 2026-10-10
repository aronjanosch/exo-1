//! Another player's figure, in front of the walker and in the cabin, and the menus.
use super::*;

pub(super) fn figure_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    use crate::menu::{Back, Menu, Screen, SettingsTab};
    let menu_shot = |screen: Screen, tag: &'static str, shot_step: &dyn Fn(&'static str) -> Step| -> Vec<Step> {
        vec![
            Box::new(move |w: &mut World, _: &mut Ctx| {
                if let Some(mut m) = w.get_resource_mut::<Menu>() {
                    m.screen = screen;
                }
                true
            }),
            wait(0.3),
            shot_step(tag),
            wait(0.3),
        ]
    };
    for (screen, tag) in [(Screen::Main, "menu-main"), (Screen::Join, "menu-join")] {
        s.extend(menu_shot(screen, tag, shot_step));
    }
    for (tab, tag) in [(SettingsTab::Sound, "settings-sound"), (SettingsTab::Display, "settings-display"), (SettingsTab::Controls, "settings-controls"), (SettingsTab::Keybinds, "settings-keybinds")] {
        s.push(Box::new(move |w, _| {
            if let Some(mut m) = w.get_resource_mut::<Menu>() { m.settings_tab = tab; }
            true
        }));
        s.extend(menu_shot(Screen::Settings(Back::Main), tag, shot_step));
    }
    s.extend(menu_shot(Screen::Paused, "menu-paused", shot_step));
    s.extend(menu_shot(Screen::None, "menu-closed", shot_step));
    // Another player's figure 4 m in front, facing the walker (test hook: a remote walker without
    // a network).
    s.push(Box::new(|w, _| {
        let p = player_world(w);
        let own = ship_frame_of(w).origin;
        face_towards(w, p + (p - own));
        let (fwd, up) = with_player(w, |pl| (pl.w.forward, pl.up));
        let at = p + fwd * 4.0;
        w.spawn((crate::net_live::RemoteWalker { owner: 2 }, crate::origin::WorldPose { pos: at, rot: walker_core::look_rot(-fwd, up) }, Transform::default(), Visibility::default()));
        true
    }));
    s.push(wait(0.5));
    s.push(shot_step("figure-outside"));
    // A screenshot is taken a few frames later; keep the scene until then.
    s.push(wait(0.5));
    // In the cabin: the walker stands at the seat, the figure near the back, both looking at it.
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        let f = ship_frame_of(w);
        let at = f.to_world(DVec3::new(0.0, 0.32, 2.0));
        let up = f.rot * DVec3::Y;
        let mut q = w.query_filtered::<&mut crate::origin::WorldPose, With<crate::net_live::RemoteWalker>>();
        for mut pose in q.iter_mut(w) {
            pose.pos = at;
            pose.rot = walker_core::look_rot(f.rot * DVec3::NEG_Z, up);
        }
        face_towards(w, at);
        true
    }));
    s.push(wait(0.6));
    s.push(shot_step("figure-cabin"));
    s.push(wait(1.0));
}
