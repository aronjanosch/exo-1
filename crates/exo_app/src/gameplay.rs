//! Gameplay glue (#129, #130, #133): the kernel (`gameplay_core`) and the jobs system
//! (`jobs_core`) in the game. This module holds their state, turns crates on pads into domain
//! events, applies every event once (host), and carries out the outcomes: spawn the crates of an
//! accepted job, release crates, route payouts and flags back in as events.
//!
//! Content: `content/gameplay/` (records), `content/place/` (where the pads are). One client for
//! now: this game is the host and the only sender (network: #134, client id and save: #135).
//!
//! A crate is on a pad when it rests outside any cabin, not held, within the pad's radius. Leaving
//! a pad is a pickup, coming to rest on one a delivery (the jobs system decides whether it counts).

use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use gameplay_core::notice::{Arg, Notice, NoticeKind, NoticeQueue};
use gameplay_core::text::{Picker, TextTable};
use gameplay_core::{ClientId, CommodityId, Content, CrateId, Dedup, Event, LocationId, Progress, TextKey, TrackId, WorldEvent};
use jobs_core::{JobContent, JobEvent, JobId, JobState, Jobs, Outcome};

use crate::cargo::{crate_bundle, Crate, Crates};
use crate::env::{from_v3, PlanetRes};
use crate::grab::Grab;

pub fn plugin(app: &mut App) {
    let crates = Crates::default();
    app.insert_resource(Gameplay::load(&crates));
    app.add_systems(FixedUpdate, (update_pads, watch_pads, gameplay_step, update_readout).chain().after(crate::cargo::budget_step).in_set(crate::phases::Fx::Cargo));
}

/// Window only: pad rings and the job line.
pub fn window_plugin(app: &mut App) {
    app.init_resource::<PadRings>();
    app.add_systems(Startup, spawn_job_line);
    app.add_systems(Update, update_pad_rings.in_set(crate::phases::Frame::World));
    app.add_systems(Update, update_job_line.in_set(crate::phases::Frame::Hud));
}

/// The host's own client id until each client keeps one (#135).
pub const HOST: ClientId = ClientId(1);
/// Seed of the text picks (TODO(initiator): later the save's seed).
const TEXT_SEED: u64 = 0x5EED_0001;

/// A crate that carries goods for a job.
#[derive(Component, Clone, Debug)]
pub struct Goods {
    pub id: CrateId,
    pub commodity: CommodityId,
    pub job: JobId,
    /// The pad it rests on, as last reported.
    pub on_pad: Option<LocationId>,
}

/// A location's pad on the current planet, world space.
#[derive(Clone, Debug)]
pub struct Pad {
    pub location: LocationId,
    pub centre: DVec3,
    pub up: DVec3,
    pub radius: f64,
}

/// A crate on a pad stands at most this high over its centre plane (m): pads are flattened.
const PAD_HEIGHT: f64 = 3.0;

impl Pad {
    pub fn contains(&self, p: DVec3) -> bool {
        let d = p - self.centre;
        let h = d.dot(self.up);
        (d - self.up * h).length() <= self.radius && (-1.0..=PAD_HEIGHT).contains(&h)
    }
}

enum Queued {
    World(Event<WorldEvent>),
    Job(Event<JobEvent>),
}

#[derive(Resource)]
pub struct Gameplay {
    pub kernel: Content,
    pub jobs_content: JobContent,
    text: TextTable,
    picker: Picker,
    /// What the players are told: queued by the systems, released with pauses (#165).
    pub notices: NoticeQueue,
    /// Every notice shown so far, oldest first (the window draws them, scenario checks read them).
    pub shown: Vec<ShownLine>,
    pub progress: Progress,
    pub jobs: Jobs,
    dedup: Dedup,
    seq: u64,
    next_crate: u64,
    pub pads: Vec<Pad>,
    pads_for: Option<warp_core::PlanetId>,
    queue: Vec<Queued>,
    /// What happened, oldest first (scenario checks).
    pub log: Vec<String>,
    /// The job line as shown: money, the active job.
    pub readout: String,
    clock: f64,
}

/// A notice as the player sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownLine {
    pub kind: NoticeKind,
    pub key: String,
    pub text: String,
    pub banner: bool,
    /// How long it stays on screen (s).
    pub seconds: f64,
    /// A payout to count up in the banner (`{money}` in its text).
    pub money: Option<i64>,
    /// Game clock when it was released (s).
    pub at: f64,
}

impl Gameplay {
    /// Loads and checks `content/gameplay/` and the places the locations name; panics with every
    /// error, like the planet recipes. Offers the fixed jobs (#133: no board yet).
    pub fn load(crates: &Crates) -> Gameplay {
        let files = crate::content_files("gameplay");
        let sizes: Vec<&str> = crates.0.sizes.iter().map(|s| s.name.as_str()).collect();
        let fail = |e: Vec<String>| -> ! { panic!("content/gameplay:\n  {}", e.join("\n  ")) };
        let kernel = Content::load(&files, &sizes).unwrap_or_else(|e| fail(e));
        let jobs_content = JobContent::load(&files, &kernel).unwrap_or_else(|e| fail(e));
        let places: Vec<planet_core::Place> = crate::content_files("place").iter().map(|f| content_core::parse_strict(&f.path, &f.text).unwrap_or_else(|e| panic!("content/place/{e}"))).collect();
        let mut e = Vec::new();
        for l in kernel.locations.values() {
            match places.iter().find(|p| p.id == l.record.place) {
                None => e.push(format!("{}: place: no content/place/{}.json", l.path, l.record.place)),
                Some(p) if !p.pads.iter().any(|pad| pad.id == l.record.pad) => e.push(format!("{}: pad: place '{}' has no pad '{}'", l.path, p.id, l.record.pad)),
                Some(_) => {}
            }
        }
        if !e.is_empty() {
            fail(e);
        }
        let text = files
            .iter()
            .find(|f| f.path == "text/en.json")
            .map(|f| TextTable::from_json("content/gameplay/text/en.json", &f.text).unwrap_or_else(|e| panic!("{e}")))
            .unwrap_or_default();
        let progress = Progress::new(&kernel);
        let mut jobs = Jobs::default();
        for t in jobs_content.templates.values() {
            jobs.offer_fixed(&t.record);
        }
        Gameplay { kernel, jobs_content, text, picker: Picker::new(TEXT_SEED), notices: NoticeQueue::default(), shown: Vec::new(), progress, jobs, dedup: Dedup::default(), seq: 0, next_crate: 1, pads: Vec::new(), pads_for: None, queue: Vec::new(), log: Vec::new(), readout: String::new(), clock: 0.0 }
    }

    /// English text for a key (the first line of a pool); the key itself when it has none (#131
    /// adds checked tables).
    pub fn text(&self, key: &TextKey) -> String {
        self.text.lines(key.as_str()).and_then(|l| l.first().cloned()).unwrap_or_else(|| key.to_string())
    }

    pub fn push_world(&mut self, from: ClientId, e: WorldEvent) {
        self.seq += 1;
        self.queue.push(Queued::World(Event::new(from, self.seq, e)));
    }

    pub fn push_job(&mut self, from: ClientId, e: JobEvent) {
        self.seq += 1;
        self.queue.push(Queued::Job(Event::new(from, self.seq, e)));
    }

    /// The pad at a world point, if any.
    pub fn pad_at(&self, p: DVec3) -> Option<&Pad> {
        self.pads.iter().find(|pad| pad.contains(p))
    }

    pub fn pad_of(&self, l: &LocationId) -> Option<&Pad> {
        self.pads.iter().find(|p| &p.location == l)
    }

    /// An offered job whose first pickup is the pad at `p` (the prompt offers it there).
    pub fn offer_at(&self, p: DVec3) -> Option<JobId> {
        let pad = self.pad_at(p)?;
        self.jobs.all().find(|j| j.state == JobState::Offered && j.legs.first().is_some_and(|l| l.from == pad.location)).map(|j| j.id)
    }

    /// "First haul (300 credits)".
    pub fn offer_label(&self, job: JobId) -> String {
        let Some(j) = self.jobs.get(job) else { return String::new() };
        let Some(t) = self.jobs_content.templates.get(&j.template) else { return String::new() };
        format!("{} ({} {})", self.text(&t.record.title), t.record.reward, self.text(&TextKey::new("track.wallet.name")))
    }

    /// A line for the developer log (scenario checks, the terminal); the players are told through
    /// notices.
    fn note(&mut self, s: String) {
        println!("gameplay: {s}");
        self.log.push(s);
    }

    /// Queues a notice for the players.
    pub fn notify(&mut self, n: Notice) {
        self.notices.push(n);
    }

    /// Releases what the queue allows and renders it (a line of the pool, never the same twice).
    fn pace_notices(&mut self, dt: f64) {
        for s in self.notices.tick(dt) {
            let text = self.picker.render(&self.text, &s.notice);
            let money = s.notice.args.iter().find_map(|(k, a)| if let ("money", Arg::Number(n)) = (k.as_str(), a) { Some(*n) } else { None });
            println!("gameplay: [{}] {text}", if s.banner { "banner" } else { "toast" });
            self.shown.push(ShownLine { kind: s.notice.kind, key: s.notice.key.to_string(), text, banner: s.banner, seconds: s.seconds, money, at: self.clock });
        }
    }

    /// Tells the players a request was refused, where they can act on it.
    fn refused(&mut self, r: &jobs_core::Refusal) {
        let key = match r {
            jobs_core::Refusal::TooManyActive => "notice.refused.too_many",
            jobs_core::Refusal::NotAvailable => "notice.refused.not_available",
            _ => return,
        };
        self.notify(Notice::new(NoticeKind::Warning, key));
    }
}

/// Pads of the current planet, when it changes (#129).
pub fn update_pads(planet: Res<PlanetRes>, mut gp: ResMut<Gameplay>) {
    if gp.pads_for == Some(planet.id) {
        return;
    }
    let pads: Vec<Pad> = gp
        .kernel
        .locations
        .values()
        .filter_map(|l| {
            let p = planet.pgen.pad(&l.record.place, &l.record.pad)?;
            Some(Pad { location: l.record.id.clone(), centre: planet.centre + from_v3(p.centre), up: from_v3(p.up), radius: p.radius_m })
        })
        .collect();
    gp.pads = pads;
    gp.pads_for = Some(planet.id);
}

/// Crates with goods leaving or coming to rest on pads become pickups and deliveries.
pub fn watch_pads(mut gp: ResMut<Gameplay>, grab: Res<Grab>, mut crates: Query<(Entity, &Crate, &mut Goods)>) {
    let held = grab.held.as_ref().map(|h| h.crate_e);
    let mut events = Vec::new();
    for (e, c, mut g) in &mut crates {
        let now = if c.ship.is_some() || held == Some(e) {
            None
        } else if c.body.asleep || c.body.grounded {
            gp.pad_at(c.body.pos).map(|p| p.location.clone())
        } else {
            // Sliding or in the air: undecided until it rests or someone holds it.
            continue;
        };
        if now == g.on_pad {
            continue;
        }
        if let Some(a) = g.on_pad.take() {
            events.push(WorldEvent::CratePickedUp { crate_id: g.id, at: a });
        }
        if let Some(b) = &now {
            events.push(WorldEvent::CrateDelivered { crate_id: g.id, at: b.clone(), condition: c.condition });
        }
        g.on_pad = now;
    }
    for e in events {
        gp.push_world(HOST, e);
    }
}

/// Applies the queued events once each and carries out the outcomes.
pub fn gameplay_step(mut commands: Commands, time: Res<Time>, table: Res<Crates>, mut actions: ResMut<crate::controls::Actions>, mut gp: ResMut<Gameplay>, goods: Query<(Entity, &Goods)>) {
    gp.clock += time.delta_secs_f64();
    let dt = time.delta_secs_f64();
    if actions.take_tap(crate::controls::Tap::SkipNotices) {
        gp.notices.skip();
    }
    gp.push_world(HOST, WorldEvent::TimePassed { dt });
    let mut rounds = 0;
    while !gp.queue.is_empty() {
        rounds += 1;
        assert!(rounds < 100, "gameplay: events keep making events");
        let queue = std::mem::take(&mut gp.queue);
        for q in queue {
            let g = &mut *gp;
            let outcomes = match q {
                Queued::World(ev) => {
                    if !g.dedup.first_time(ev.id) {
                        continue;
                    }
                    match g.progress.apply_with_notices(&g.kernel, &ev) {
                        Ok(ns) => ns.into_iter().for_each(|n| g.notify(n)),
                        Err(r) => g.note(format!("refused {:?}: {r:?}", ev.payload)),
                    }
                    g.jobs.apply_world(&g.jobs_content, &ev)
                }
                Queued::Job(ev) => {
                    if !g.dedup.first_time(ev.id) {
                        continue;
                    }
                    match g.jobs.apply_job(&g.jobs_content, &g.kernel, &g.progress, &ev) {
                        Ok(o) => o,
                        Err(r) => {
                            g.note(format!("refused {:?}: {r:?}", ev.payload));
                            g.refused(&r);
                            Vec::new()
                        }
                    }
                }
            };
            for o in outcomes {
                carry_out(&mut commands, &table, g, &goods, o);
            }
        }
    }
    gp.pace_notices(dt);
}

fn carry_out(commands: &mut Commands, table: &Crates, gp: &mut Gameplay, goods: &Query<(Entity, &Goods)>, o: Outcome) {
    match o {
        Outcome::SpawnCrates { job, leg, commodity, count, at } => {
            let Some(pad) = gp.pad_of(&at).cloned() else {
                gp.note(format!("no pad for {at} on this planet: the crates of job {} wait", job.0));
                return;
            };
            let size = gp.kernel.commodities[&commodity].record.crate_size.clone();
            let half = table.0.get(&size).expect("checked at load").extents[1] * 0.5;
            // A row across the pad's middle, a little above the ground.
            let east = pad.up.any_orthonormal_vector();
            let north = pad.up.cross(east);
            let mut ids = Vec::new();
            for i in 0..count {
                let id = CrateId(gp.next_crate);
                gp.next_crate += 1;
                let off = (i as f64 - (count - 1) as f64 * 0.5) * 1.5;
                let pos = pad.centre + pad.up * (half + 0.05) + east * off + north * (pad.radius * 0.4);
                let fwd = DQuat::from_rotation_arc(DVec3::Y, pad.up) * DVec3::NEG_Z;
                commands.spawn((crate_bundle(&table.0, &size, None, pos, fwd), Goods { id, commodity: commodity.clone(), job, on_pad: Some(at.clone()) }));
                ids.push(id);
            }
            gp.push_job(HOST, JobEvent::CratesSpawned { job, leg, crates: ids });
        }
        Outcome::ReleaseCrates { crates } => {
            for (e, g) in goods {
                if crates.contains(&g.id) {
                    commands.entity(e).remove::<Goods>();
                }
            }
        }
        Outcome::Ended { job, .. } => {
            // Delivered crates stay where they are, as plain crates.
            for (e, g) in goods {
                if g.job == job {
                    commands.entity(e).remove::<Goods>();
                }
            }
        }
        Outcome::Emit(e) => gp.push_world(HOST, e),
        Outcome::Notice(n) => gp.notify(n),
    }
}

/// "Bent Spoon 1.9 km, left": distance and rough side of a pad from where the player looks. A
/// stand-in until the target arrow (#136).
pub fn pointer(name: &str, from: DVec3, look: DVec3, to: DVec3, up: DVec3) -> String {
    let d = to - from;
    let flat = |v: DVec3| (v - up * v.dot(up)).normalize_or_zero();
    let (f, t) = (flat(look), flat(d));
    let dist = d.length();
    let side = if dist < 40.0 {
        "here"
    } else {
        let ang = f.cross(t).dot(up).atan2(f.dot(t)).to_degrees();
        match ang {
            a if a.abs() <= 30.0 => "ahead",
            a if a.abs() >= 150.0 => "behind",
            a if a > 0.0 => "left",
            _ => "right",
        }
    };
    let dist = if dist >= 1000.0 { format!("{:.1} km", dist / 1000.0) } else { format!("{dist:.0} m") };
    format!("{name} {dist}, {side}")
}

/// The job line: money, the active job's progress and where to, a recent notice.
pub fn update_readout(
    mut gp: ResMut<Gameplay>,
    players: Query<&crate::walker::Player>,
    ships: Query<(&avian3d::prelude::Position, &avian3d::prelude::Rotation), With<crate::ship::Ship>>,
    planet: Res<PlanetRes>,
) {
    // Where the player is and looks (in the seat the walker's look is the view).
    let view = players.single().ok().zip(ships.iter().next()).map(|(pl, (p, r))| {
        let f = crate::walker::ship_frame(p, r);
        (pl.world_pos(f), pl.world_look(f))
    });
    let point = |gp: &Gameplay, l: &LocationId| -> Option<String> {
        let (pos, look) = view?;
        let pad = gp.pad_of(l)?;
        let name = gp.kernel.locations.get(l).map(|r| gp.text(&r.record.name))?;
        Some(pointer(&name, pos, look, pad.centre, planet.up(pos)))
    };
    let money = gp.progress.wallet();
    let mut s = format!("{money} {}", gp.text(&TextKey::new("track.wallet.name")));
    let xp = gp.progress.value(&gp.kernel, &TrackId::new("freight_xp"), Some(HOST));
    if let Some(xp) = xp {
        s += &format!("   {xp} {}", gp.text(&TextKey::new("track.freight_xp.name")));
    }
    for j in gp.jobs.active() {
        let Some(t) = gp.jobs_content.templates.get(&j.template) else { continue };
        let to = j.legs.first().map(|l| gp.kernel.locations.get(&l.to).map(|r| gp.text(&r.record.name)).unwrap_or_default()).unwrap_or_default();
        s += &format!("\n{}: {}/{} delivered to {to}", gp.text(&t.record.title), j.delivered(), j.asked());
        if let Some(left) = j.time_left(&t.record) {
            s += &format!(", {left:.0} s left");
        }
        // Where to next: the pickup while crates wait there, else the dropoff.
        if let Some(l) = j.legs.first() {
            let waiting = l.crates.values().any(|m| *m == jobs_core::CrateMark::Waiting);
            if let Some(p) = point(&gp, if waiting { &l.from } else { &l.to }) {
                s += &format!("\n{} {p}", if waiting { "pick up:" } else { "deliver to:" });
            }
        }
    }
    if gp.jobs.active().next().is_none()
        && let Some(l) = gp.jobs.all().find(|j| j.state == JobState::Offered).and_then(|j| j.legs.first())
        && let Some(p) = point(&gp, &l.from)
    {
        s += &format!("\njob on offer: {p}");
    }
    gp.readout = s;
}

#[derive(Component)]
struct JobLine;

fn spawn_job_line(mut commands: Commands) {
    commands.spawn((
        JobLine,
        Text::new(""),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::srgb(1.0, 0.92, 0.55)),
        Node { position_type: PositionType::Absolute, top: px(8), right: px(12), ..default() },
        TextLayout::justify(Justify::Right),
    ));
}

fn update_job_line(gp: Res<Gameplay>, mut q: Query<&mut Text, With<JobLine>>) {
    if let Ok(mut t) = q.single_mut()
        && **t != gp.readout
    {
        **t = gp.readout.clone();
    }
}

/// Which planet the rings were spawned for.
#[derive(Resource, Default)]
pub struct PadRings(Option<warp_core::PlanetId>);

/// A plain glowing ring on every pad until places have models (DECISIONS.md, "Places").
fn update_pad_rings(mut commands: Commands, planet: Res<PlanetRes>, gp: Res<Gameplay>, mut rings: ResMut<PadRings>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if rings.0 == Some(planet.id) || gp.pads_for != Some(planet.id) {
        return;
    }
    rings.0 = Some(planet.id);
    let mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.85, 0.3), emissive: LinearRgba::rgb(4.0, 2.6, 0.6), ..default() });
    for p in &gp.pads {
        let mesh = meshes.add(Torus { minor_radius: 0.12, major_radius: p.radius as f32 });
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat.clone()),
            Transform { rotation: Quat::from_rotation_arc(Vec3::Y, p.up.as_vec3()), ..default() },
            crate::origin::WorldPos(p.centre + p.up * 0.08),
            crate::terrain::PlanetScene(planet.id),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #129: the shipped content loads, and every location on Hearth has its pad on the ground.
    #[test]
    fn every_location_finds_its_pad_on_the_planet() {
        let gp = Gameplay::load(&Crates::default());
        let sys = warp_core::System::from_json(crate::warp::SYSTEM).unwrap();
        let home = warp_core::PlanetId(0);
        let planet = PlanetRes::load(home, sys.planet(home));
        let places = crate::env::places_for(&sys.planet(home).recipe);
        for l in gp.kernel.locations.values() {
            if !places.iter().any(|p| p.id == l.record.place) {
                continue;
            }
            let pad = planet.pgen.pad(&l.record.place, &l.record.pad).unwrap_or_else(|| panic!("{}: no pad on the planet", l.path));
            let s = planet.pgen.sample(pad.up);
            assert!(s.water_depth == 0.0 && s.slope_deg < 3.0, "{}: pad in water or on a slope ({:.1}°)", l.path, s.slope_deg);
        }
        assert!(gp.jobs.all().any(|j| j.state == JobState::Offered), "the fixed job is offered at the start");
    }

    #[test]
    fn pointer_names_distance_and_side() {
        let (up, look) = (DVec3::Y, DVec3::NEG_Z);
        assert_eq!(pointer("A", DVec3::ZERO, look, DVec3::new(0.0, 0.0, -1900.0), up), "A 1.9 km, ahead");
        assert_eq!(pointer("A", DVec3::ZERO, look, DVec3::new(-300.0, 5.0, 0.0), up), "A 300 m, left");
        assert_eq!(pointer("A", DVec3::ZERO, look, DVec3::new(300.0, 0.0, 0.0), up), "A 300 m, right");
        assert_eq!(pointer("A", DVec3::ZERO, look, DVec3::new(0.0, 0.0, 300.0), up), "A 300 m, behind");
        assert_eq!(pointer("A", DVec3::ZERO, look, DVec3::new(10.0, 0.0, 0.0), up), "A 10 m, here");
    }
}
