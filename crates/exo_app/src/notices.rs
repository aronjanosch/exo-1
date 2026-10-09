//! Banners and toasts (#165): what `Gameplay::shown` releases, drawn over the game. One banner at
//! a time in the upper middle (a payout counts up), toasts in a short stack below it. They never
//! take input; Enter (`Tap::SkipNotices`) runs the rest of a ritual fast (in `gameplay_step`).
//! Window only; the logic is in `ScreenNotices` so it can be tested.
use bevy::prelude::*;

use crate::gameplay::{Gameplay, ShownLine};

/// Seconds a payout takes to count up in its banner.
pub const COUNT_UP_S: f64 = 1.2;
/// Toasts on screen at most; older ones go first.
const MAX_TOASTS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Banner {
    pub text: String,
    pub money: Option<i64>,
    pub born: f64,
    pub until: f64,
}

/// What is on screen now.
#[derive(Resource, Default, Debug)]
pub struct ScreenNotices {
    pub banner: Option<Banner>,
    pub toasts: Vec<(String, f64)>,
    /// How many of `Gameplay::shown` were taken in.
    seen: usize,
}

impl ScreenNotices {
    /// Takes in the lines released since last time, at time `now`.
    pub fn absorb(&mut self, shown: &[ShownLine], now: f64) {
        for l in shown.get(self.seen..).unwrap_or_default() {
            if l.banner {
                self.banner = Some(Banner { text: l.text.clone(), money: l.money, born: now, until: now + l.seconds });
            } else {
                self.toasts.push((l.text.clone(), now + l.seconds));
            }
        }
        self.seen = shown.len();
        self.expire(now);
    }

    pub fn expire(&mut self, now: f64) {
        self.toasts.retain(|(_, until)| *until > now);
        if self.toasts.len() > MAX_TOASTS {
            let extra = self.toasts.len() - MAX_TOASTS;
            self.toasts.drain(..extra);
        }
        if self.banner.as_ref().is_some_and(|b| b.until <= now) {
            self.banner = None;
        }
    }

    /// The banner's text at `now`: a payout shows the part of it reached so far.
    pub fn banner_text(&self, now: f64) -> Option<String> {
        let b = self.banner.as_ref()?;
        Some(match b.money {
            Some(m) if m != 0 => {
                let k = ((now - b.born) / COUNT_UP_S).clamp(0.0, 1.0);
                // Ease out: fast at first, slowing into the total.
                let part = (m as f64 * (1.0 - (1.0 - k).powi(2))).round() as i64;
                b.text.replacen(&m.to_string(), &part.to_string(), 1)
            }
            _ => b.text.clone(),
        })
    }

    pub fn toast_text(&self) -> String {
        self.toasts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join("\n")
    }
}

#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct ToastText;

pub fn window_plugin(app: &mut App) {
    app.init_resource::<ScreenNotices>();
    app.add_systems(Startup, spawn);
    app.add_systems(Update, update.in_set(crate::phases::Frame::Hud));
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        BannerText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(34.0), ..default() },
        TextColor(Color::srgb(1.0, 0.9, 0.4)),
        Node { position_type: PositionType::Absolute, top: percent(14), width: percent(100), justify_content: JustifyContent::Center, ..default() },
        TextLayout::justify(Justify::Center),
    ));
    commands.spawn((
        ToastText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(18.0), ..default() },
        TextColor(Color::srgb(0.92, 0.95, 1.0)),
        Node { position_type: PositionType::Absolute, top: percent(24), width: percent(100), justify_content: JustifyContent::Center, ..default() },
        TextLayout::justify(Justify::Center),
    ));
}

fn update(gp: Res<Gameplay>, time: Res<Time>, mut screen: ResMut<ScreenNotices>, mut banner: Query<&mut Text, (With<BannerText>, Without<ToastText>)>, mut toasts: Query<&mut Text, (With<ToastText>, Without<BannerText>)>) {
    let now = time.elapsed_secs_f64();
    screen.absorb(&gp.shown, now);
    if let Ok(mut t) = banner.single_mut() {
        let s = screen.banner_text(now).unwrap_or_default();
        if **t != s {
            **t = s;
        }
    }
    if let Ok(mut t) = toasts.single_mut() {
        let s = screen.toast_text();
        if **t != s {
            **t = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gameplay_core::notice::NoticeKind;

    fn line(text: &str, banner: bool, money: Option<i64>, seconds: f64) -> ShownLine {
        ShownLine { kind: NoticeKind::Completed, key: "k".into(), text: text.into(), banner, seconds, money, at: 0.0 }
    }

    #[test]
    fn a_payout_counts_up_to_the_total_and_the_banner_leaves() {
        let mut s = ScreenNotices::default();
        s.absorb(&[line("JOB DONE   +375", true, Some(375), 3.0)], 10.0);
        assert_eq!(s.banner_text(10.0).unwrap(), "JOB DONE   +0");
        let mid = s.banner_text(10.6).unwrap();
        assert!(mid != "JOB DONE   +0" && mid != "JOB DONE   +375", "{mid}");
        assert_eq!(s.banner_text(11.3).unwrap(), "JOB DONE   +375");
        s.expire(13.1);
        assert!(s.banner.is_none());
    }

    #[test]
    fn toasts_stack_expire_and_are_taken_in_once() {
        let mut s = ScreenNotices::default();
        let shown = vec![line("a", false, None, 2.0), line("b", false, None, 4.0)];
        s.absorb(&shown, 0.0);
        s.absorb(&shown, 1.0);
        assert_eq!(s.toast_text(), "a\nb", "taken in once");
        s.expire(3.0);
        assert_eq!(s.toast_text(), "b");
        let many: Vec<ShownLine> = (0..7).map(|i| line(&format!("t{i}"), false, None, 9.0)).collect();
        let mut s = ScreenNotices::default();
        s.absorb(&many, 0.0);
        assert_eq!(s.toasts.len(), MAX_TOASTS);
        assert!(s.toast_text().ends_with("t6"));
    }
}
