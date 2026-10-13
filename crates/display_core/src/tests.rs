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
fn a_column_stacks_fixed_and_flexible_items_with_gaps() {
    let p = Panel::new(200.0, 100.0, column(10.0, vec![label("a").fixed(20.0), label("b"), label("c").flex(3.0)]));
    let l = p.layout();
    assert_eq!(l.len(), 3);
    // Inside the 16 px padding: 168 x 68; 20 fixed, two gaps, 28 left for flex 1 : 3.
    assert_eq!(l[0].rect, Rect::new(16.0, 16.0, 168.0, 20.0));
    assert_eq!(l[1].rect, Rect::new(16.0, 46.0, 168.0, 7.0));
    assert_eq!(l[2].rect, Rect::new(16.0, 63.0, 168.0, 21.0));
    assert!((l[2].rect.max().y - (100.0 - 16.0)).abs() < 1e-9, "the column ends at the padding");
}

#[test]
fn a_row_splits_its_width_and_space_takes_room_without_a_widget() {
    let p = Panel::new(232.0, 82.0, row(0.0, vec![button("x", "x").fixed(50.0), space(), button("y", "y")]));
    let l = p.layout();
    assert_eq!(l.len(), 2, "space draws nothing");
    assert_eq!(l[0].rect, Rect::new(16.0, 16.0, 50.0, 50.0));
    assert_eq!(l[1].rect, Rect::new(16.0 + 50.0 + 75.0, 16.0, 75.0, 50.0));
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
    // Pressed elsewhere, released over the button: no click either.
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
    // Beside the slider while held: clamped to its end; off the panel: no change.
    let c = ptr.step(&p, Some(DVec2::new(r.max().x + 50.0, 10.0)), true);
    assert_eq!(c[0].value, Some(1.0));
    assert!(ptr.step(&p, None, true).is_empty());
    assert!(ptr.step(&p, at(0.5), false).is_empty(), "the release adds nothing");
    assert!(ptr.step(&p, at(0.1), false).is_empty(), "not held: no change");
}

#[test]
fn bar_levels_for_high_and_low_bad_values() {
    assert_eq!(level(0.5, None, None), Level::Normal);
    // Heat: warn 0.7, crit 0.9.
    assert_eq!(level(0.6, Some(0.7), Some(0.9)), Level::Normal);
    assert_eq!(level(0.7, Some(0.7), Some(0.9)), Level::Warn);
    assert_eq!(level(0.95, Some(0.7), Some(0.9)), Level::Crit);
    // Fuel: warn 0.3, crit 0.1.
    assert_eq!(level(0.5, Some(0.3), Some(0.1)), Level::Normal);
    assert_eq!(level(0.2, Some(0.3), Some(0.1)), Level::Warn);
    assert_eq!(level(0.05, Some(0.3), Some(0.1)), Level::Crit);
    assert_eq!(level(0.95, None, Some(0.9)), Level::Crit);
    let b = bar(0.5, "x").thresholds(Some(0.4), None);
    assert_eq!(b.widget, Widget::Bar { value: 0.5, label: "x".into(), warn: Some(0.4), crit: None });
}

#[test]
fn a_panel_fits_the_screen_centred_and_maps_both_ways() {
    let f = Fit::centred(DVec2::new(1600.0, 900.0), DVec2::new(400.0, 300.0), 0.7);
    assert!((f.rect.size.y - 630.0).abs() < 1e-9 && (f.rect.size.x - 840.0).abs() < 1e-9);
    assert!((f.rect.centre() - DVec2::new(800.0, 450.0)).length() < 1e-9);
    let p = DVec2::new(123.0, 45.0);
    assert!((f.to_panel(f.to_screen(p)) - p).length() < 1e-9);
    // A narrow screen: the width limits.
    let n = Fit::centred(DVec2::new(500.0, 900.0), DVec2::new(400.0, 300.0), 0.7);
    assert!((n.rect.size.x - 475.0).abs() < 1e-9 && n.rect.size.y < 630.0);
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
    // From behind, pointing away, parallel, or beside the quad: no hit.
    let back = centre + rot * DVec3::new(0.0, 0.0, -2.0);
    assert!(ray_to_panel(back, centre - back, centre, rot, size).is_none());
    assert!(ray_to_panel(front, front - centre, centre, rot, size).is_none());
    assert!(ray_to_panel(front, rot * DVec3::X, centre, rot, size).is_none());
    let beside = panel_point(centre, rot, size, DVec2::new(1.2, 0.5));
    assert!(ray_to_panel(front, beside - front, centre, rot, size).is_none());
}

#[test]
fn panel_ids_are_const_and_compare_by_text() {
    const ID: PanelId = PanelId::new("test_console");
    assert_eq!(ID, PanelId::from("test_console".to_string()));
    assert_eq!(ID.as_str(), "test_console");
}

#[test]
fn a_screen_puts_its_widgets_in_the_well_under_the_header() {
    let p = Panel::screen(Tier::M, "Reactor 2", "RX-02", column(0.0, vec![label("a")])).status("STABLE", Tone::Good);
    assert_eq!(p.size, DVec2::new(640.0, 400.0));
    assert_eq!(p.tier(), Tier::M);
    let h = p.header().unwrap();
    let w = p.well().unwrap();
    assert_eq!(h, Rect::new(2.0, 2.0, 636.0, 32.0));
    assert_eq!(w, Rect::new(6.0, 34.0, 628.0, 360.0));
    let c = p.content();
    assert_eq!(c, Rect::new(6.0 + 14.0, 34.0 + 14.0, 628.0 - 28.0, 360.0 - 28.0));
    assert_eq!(p.layout()[0].rect, c, "the one label fills the content");
    assert_eq!(p.plate.as_ref().unwrap().status, Some(("STABLE".into(), Tone::Good)));
    assert!(Panel::new(10.0, 10.0, space()).status("x", Tone::Bad).plate.is_none(), "no plate, no status");
}

#[test]
fn tiers_have_the_design_canvases_and_the_nearest_is_found() {
    assert_eq!(Tier::S.canvas(), DVec2::new(320.0, 240.0));
    assert_eq!(Tier::L.canvas(), DVec2::new(1280.0, 720.0));
    assert_eq!(Tier::nearest(DVec2::new(480.0, 360.0)), Tier::M);
    assert_eq!(Tier::nearest(DVec2::new(300.0, 200.0)), Tier::S);
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
    // Without an action the list is only a readout.
    let q = Panel::new(232.0, 200.0, list(vec![list_row("a", "", Tone::Good)]));
    assert!(q.hit(q.content().centre()).is_none());
}

#[test]
fn thresholds_and_tones_reach_the_new_widgets() {
    assert!(matches!(dial(0.5, "50%", "Output").thresholds(Some(0.8), Some(0.95)).widget, Widget::Dial { warn: Some(_), crit: Some(_), .. }));
    assert!(matches!(trend(vec![0.1, 0.2]).thresholds(Some(0.7), None).widget, Widget::Trend { warn: Some(_), crit: None, .. }));
    assert!(matches!(button("SCRAM", "scram").tone(Tone::Bad).widget, Widget::Button { tone: Tone::Bad, .. }));
    assert!(matches!(readout("412 °C").widget, Widget::Label { mono: true, size, .. } if size == font::VALUE));
    assert!(matches!(micro("Mode").widget, Widget::Label { tone: Tone::Micro, .. }));
}

#[test]
fn the_shipped_theme_parses_and_bad_ones_are_refused() {
    let src = include_str!("../../../content/ui/displays.json");
    let t = Theme::from_json(src).unwrap();
    assert_eq!(t.colours.accent, Rgb::from_hex("#6fb8ff").unwrap());
    assert!(t.share(Tier::S) < t.share(Tier::M) && t.share(Tier::M) < t.share(Tier::L), "bigger screens open bigger");
    assert_eq!(t.tone(Tone::Bad), t.colours.bad);
    assert!(Rgb::from_hex("6fb8ff").is_err() && Rgb::from_hex("#6fb8f").is_err() && Rgb::from_hex("#gggggg").is_err());
    let bad = src.replace("\"s\": 0.42", "\"s\": 1.5");
    assert!(Theme::from_json(&bad).is_err());
    let unknown = src.replacen("\"source\"", "\"surprise\": 1, \"source\"", 1);
    assert!(Theme::from_json(&unknown).is_err());
}
