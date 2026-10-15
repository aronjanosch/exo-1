use bevy::prelude::*;

use crate::gameplay::{Gameplay, ShownLine};

pub const COUNT_UP_S: f64 = 1.2;
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
        TextColor(crate::ui_theme::GOLD),
        TextShadow { offset: Vec2::splat(2.5), color: crate::ui_theme::SHADOW },
        Node { position_type: PositionType::Absolute, top: percent(14), width: percent(100), justify_content: JustifyContent::Center, ..default() },
        TextLayout::justify(Justify::Center),
    ));
    commands.spawn((
        ToastText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(18.0), ..default() },
        TextColor(crate::ui_theme::INK),
        TextShadow { offset: Vec2::splat(1.5), color: crate::ui_theme::SHADOW },
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
