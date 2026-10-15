use super::*;

fn console(n: u32, lights: bool) -> Panel {
    Panel::new(
        400.0,
        300.0,
        column(
            10.0,
            vec![
                title("TEST").fixed(40.0),
                row(10.0, vec![button("-", "dec").enabled(n > 0), label(format!("{n}")), button("+", "inc")]).fixed(50.0),
                bar(n as f64 / 10.0, "LOAD").fixed(30.0),
                toggle(lights, "LIGHTS", "lights").fixed(40.0),
                slider(0.5, "TRIM", "trim").fixed(30.0),
                space(),
            ],
        ),
    )
}

fn centre_of(p: &Panel, action: &str) -> DVec2 {
    p.find(action).unwrap().rect.centre()
}

#[test]
fn too_much_fixed_room_leaves_flex_items_empty_not_negative() {
    let p = Panel::new(100.0, 100.0, column(10.0, vec![label("a").fixed(80.0), label("b")]));
    assert_eq!(p.layout()[1].rect.size.y, 0.0);
}

#[test]
fn hover_names_the_usable_widget_under_the_pointer() {
    let p = console(0, false);
    let mut ptr = Pointer::default();
    ptr.step(&p, Some(centre_of(&p, "inc")), false);
    assert_eq!(ptr.hover.as_deref(), Some("inc"));
    ptr.step(&p, Some(DVec2::new(1.0, 1.0)), false);
    assert_eq!(ptr.hover, None, "the padding is no widget");
    ptr.step(&p, Some(centre_of(&p, "dec")), false);
    assert_eq!(ptr.hover, None, "a disabled button has no hover");
    ptr.step(&p, None, false);
    assert_eq!(ptr.hover, None);
}

#[test]
fn press_and_release_over_the_same_button_clicks_once() {
    let p = console(0, false);
    let mut ptr = Pointer::default();
    let at = Some(centre_of(&p, "inc"));
    assert!(ptr.step(&p, at, false).is_empty());
    assert!(ptr.step(&p, at, true).is_empty(), "no click on the press");
    assert_eq!(ptr.pressed.as_deref(), Some("inc"));
    assert!(ptr.step(&p, at, true).is_empty(), "no click while held");
    assert_eq!(ptr.step(&p, at, false), vec![Click { action: "inc".into(), value: None }]);
    assert!(ptr.step(&p, at, false).is_empty(), "one click per press");
    assert_eq!(ptr.pressed, None);
}

#[test]
fn release_elsewhere_or_off_the_panel_is_no_click() {
    let p = console(0, false);
    let mut ptr = Pointer::default();
    ptr.step(&p, Some(centre_of(&p, "inc")), true);
    assert!(ptr.step(&p, Some(centre_of(&p, "lights")), false).is_empty(), "released over another widget");
    ptr.step(&p, Some(centre_of(&p, "inc")), true);
    assert!(ptr.step(&p, None, false).is_empty(), "released off the panel");
    ptr.step(&p, Some(DVec2::new(1.0, 1.0)), true);
    assert!(ptr.step(&p, Some(centre_of(&p, "inc")), false).is_empty());
}

#[test]
fn a_disabled_button_does_not_click_and_an_enabled_one_does() {
    let mut ptr = Pointer::default();
    let p0 = console(0, false);
    let at = Some(centre_of(&p0, "dec"));
    ptr.step(&p0, at, true);
    assert!(ptr.step(&p0, at, false).is_empty());
    let p1 = console(1, false);
    ptr.step(&p1, at, true);
    assert_eq!(ptr.step(&p1, at, false), vec![Click { action: "dec".into(), value: None }]);
}

#[test]
fn a_toggle_reports_its_new_state() {
    let mut ptr = Pointer::default();
    for (on, want) in [(false, 1.0), (true, 0.0)] {
        let p = console(0, on);
        let at = Some(centre_of(&p, "lights"));
        ptr.step(&p, at, true);
        assert_eq!(ptr.step(&p, at, false), vec![Click { action: "lights".into(), value: Some(want) }]);
    }
}

#[test]
fn a_slider_reports_its_value_on_press_and_while_dragged() {
    let p = console(0, false);
    let r = p.find("trim").unwrap().rect;
    let at = |t: f64| Some(DVec2::new(r.min.x + r.size.x * t, r.centre().y));
    let mut ptr = Pointer::default();
    let c = ptr.step(&p, at(0.25), true);
    assert_eq!(c.len(), 1);
    assert!((c[0].value.unwrap() - 0.25).abs() < 1e-9 && c[0].action == "trim");
    assert!(ptr.step(&p, at(0.25), true).is_empty(), "no repeat without a change");
    let c = ptr.step(&p, at(0.75), true);
    assert!((c[0].value.unwrap() - 0.75).abs() < 1e-9);
    let c = ptr.step(&p, Some(DVec2::new(r.max().x + 50.0, 10.0)), true);
    assert_eq!(c[0].value, Some(1.0));
    assert!(ptr.step(&p, None, true).is_empty());
    assert!(ptr.step(&p, at(0.5), false).is_empty(), "the release adds nothing");
    assert!(ptr.step(&p, at(0.1), false).is_empty(), "not held: no change");
}

#[test]
fn a_ray_hit_becomes_a_panel_point_and_back() {
    let p = Panel::screen(Tier::M, "Reactor 2", "RX-02", column(0.0, vec![label("a")]));
    assert_eq!(p.from_uv(DVec2::new(0.5, 0.5)), p.size * 0.5);
    assert_eq!(p.from_uv(DVec2::new(1.0, 0.0)), DVec2::new(p.size.x, 0.0));
    let at = DVec2::new(123.0, 45.0);
    assert!((p.from_uv(p.to_uv(at)) - at).length() < 1e-9);
}

#[test]
fn a_ray_meets_the_front_of_a_turned_panel() {
    let rot = DQuat::from_rotation_y(0.7) * DQuat::from_rotation_x(-0.3);
    let centre = DVec3::new(10.0, 2.0, -5.0);
    let size = DVec2::new(1.2, 0.8);
    for uv in [DVec2::new(0.5, 0.5), DVec2::new(0.1, 0.9), DVec2::new(0.8, 0.2)] {
        let target = panel_point(centre, rot, size, uv);
        let origin = centre + rot * DVec3::new(0.3, 0.2, 2.0);
        let hit = ray_to_panel(origin, (target - origin) * 3.0, centre, rot, size).expect("hits the front");
        assert!((hit.uv - uv).length() < 1e-9, "{uv} -> {}", hit.uv);
        assert!((hit.distance - (target - origin).length()).abs() < 1e-9);
    }
    let front = centre + rot * DVec3::new(0.0, 0.0, 2.0);
    assert!(ray_to_panel(front, centre - front, centre, rot, size).is_some());
    let back = centre + rot * DVec3::new(0.0, 0.0, -2.0);
    assert!(ray_to_panel(back, centre - back, centre, rot, size).is_none());
    assert!(ray_to_panel(front, front - centre, centre, rot, size).is_none());
    assert!(ray_to_panel(front, rot * DVec3::X, centre, rot, size).is_none());
    let beside = panel_point(centre, rot, size, DVec2::new(1.2, 0.5));
    assert!(ray_to_panel(front, beside - front, centre, rot, size).is_none());
    let hit = ray_to_panel_plane(front, beside - front, centre, rot, size).expect("the plane is there");
    assert!((hit.uv - DVec2::new(1.2, 0.5)).length() < 1e-9);
    assert!(ray_to_panel_plane(back, centre - back, centre, rot, size).is_none());
}

#[test]
fn the_pointer_ray_and_the_window_point_are_inverses() {
    let rot = DQuat::from_rotation_y(0.4) * DQuat::from_rotation_x(-0.2);
    let (pos, window, fov) = (DVec3::new(1.0, 2.0, 3.0), DVec2::new(1600.0, 900.0), 1.1);
    let (o, d) = pointer_ray(pos, rot, fov, window, window * 0.5);
    assert!(o == pos && (d - rot * DVec3::NEG_Z).length() < 1e-9);
    for p in [DVec2::new(10.0, 20.0), DVec2::new(800.0, 450.0), DVec2::new(1500.0, 700.0)] {
        let (o, d) = pointer_ray(pos, rot, fov, window, p);
        let back = window_point(pos, rot, fov, window, o + d * 2.7).expect("in front");
        assert!((back - p).length() < 1e-6, "{p} -> {back}");
    }
    assert!(window_point(pos, rot, fov, window, pos + rot * DVec3::Z).is_none(), "behind the camera");
}

#[test]
fn status_tones_carry_a_glyph() {
    assert_eq!(Tone::Good.glyph(), Some(Glyph::Circle));
    assert_eq!(Tone::Warn.glyph(), Some(Glyph::Triangle));
    assert_eq!(Tone::Bad.glyph(), Some(Glyph::Square));
    assert_eq!(Tone::Normal.glyph(), None);
    assert_eq!(Level::Crit.tone(), Tone::Bad);
}

#[test]
fn a_disabled_toggle_or_slider_is_no_target() {
    let p = Panel::new(200.0, 200.0, column(0.0, vec![toggle(true, "", "t").enabled(false).labels("MAN", "AUTO"), slider(0.3, "", "s").enabled(false)]));
    let mut ptr = Pointer::default();
    for a in ["t", "s"] {
        let at = Some(p.find(a).unwrap().rect.centre());
        ptr.step(&p, at, false);
        assert_eq!(ptr.hover, None, "{a} disabled: no hover");
        assert!(ptr.step(&p, at, true).is_empty() && ptr.step(&p, at, false).is_empty(), "{a} disabled: no click");
    }
    assert!(matches!(&p.find("t").unwrap().widget, Widget::Toggle { labels, .. } if labels[1] == "AUTO"));
}

#[test]
fn a_list_click_gives_the_row_index() {
    let rows = vec![list_row("Hull", "92%", Tone::Good), list_row("Reactor", "HOT", Tone::Warn), list_row("Door", "", Tone::Dim)];
    let p = Panel::new(232.0, 200.0, column(0.0, vec![list(rows).select(Some(0), Some("pick")).fixed(160.0), space()]));
    let r = p.find("pick").unwrap().rect;
    let at = Some(DVec2::new(r.centre().x, r.min.y + 40.0 * 1.5));
    let mut ptr = Pointer::default();
    ptr.step(&p, at, true);
    assert_eq!(ptr.step(&p, at, false), vec![Click { action: "pick".into(), value: Some(1.0) }]);
    assert_eq!(list_row_at(&r, 40.0, 3, DVec2::new(r.centre().x, r.min.y + 1.0)), Some(0));
    assert_eq!(list_row_at(&r, 40.0, 3, DVec2::new(r.centre().x, r.max().y - 1.0)), None, "below the last row");
    let q = Panel::new(232.0, 200.0, list(vec![list_row("a", "", Tone::Good)]));
    assert!(q.hit(q.content().centre()).is_none());
}

#[test]
fn the_shipped_theme_parses_and_bad_ones_are_refused() {
    let src = include_str!("../../../content/ui/displays.json");
    let t = Theme::from_json(src).unwrap();
    assert_eq!(t.colours.accent, Rgb::from_hex("#6fb8ff").unwrap());
    assert_eq!(t.tone(Tone::Bad), t.colours.bad);
    assert!(Rgb::from_hex("6fb8ff").is_err() && Rgb::from_hex("#6fb8f").is_err() && Rgb::from_hex("#gggggg").is_err());
    let bad = src.replace("\"texture_scale\": 2.0", "\"texture_scale\": 9.0");
    assert!(Theme::from_json(&bad).is_err());
    let unknown = src.replacen("\"source\"", "\"surprise\": 1, \"source\"", 1);
    assert!(Theme::from_json(&unknown).is_err());
}

#[test]
fn a_canvas_with_an_action_is_a_button_that_clicks_with_no_value() {
    use crate::{canvas, Ink, Shape};
    let mut p = Pointer::default();
    let panel = Panel::screen(Tier::S, "T", "X", canvas(100.0, 50.0, vec![Shape::Circle { cx: 50.0, cy: 25.0, r: 20.0, fill: Some(Ink::Hud), stroke: None }], Some("lever")).flex(1.0));
    let at = panel.find("lever").map(|w| w.rect.centre());
    assert!(at.is_some());
    assert!(p.step(&panel, at, true).is_empty());
    let out = p.step(&panel, at, false);
    assert_eq!(out, vec![Click { action: "lever".into(), value: None }]);
}
