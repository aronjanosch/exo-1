//! A crate is on a pad when it rests outside any cabin, not held, within the pad's radius. Leaving
//! a pad is a pickup, coming to rest on one a delivery (the jobs system decides whether it counts).

use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use gameplay_core::notice::{Arg, Notice, NoticeKind, NoticeQueue};
use gameplay_core::text::{Picker, TextTable};
use gameplay_core::{ClientId, CommodityId, Content, CrateId, Dedup, Event, LocationId, Progress, TextKey, TrackId, WorldEvent};
use gameplay_core::save::{Envelope, KernelState, SaveError};
use jobs_core::{Briefing, GiverId, JobContent, JobEvent, JobId, JobState, Jobs, Outcome};

use crate::cargo::{crate_bundle, Crate, Crates};
use crate::env::{from_v3, PlanetRes};
use crate::grab::Grab;

#[derive(Clone, Debug, PartialEq)]
pub struct GameSave {
    pub seed: u64,
}

impl GameSave {
    pub const SECTION: &str = "game";
    pub const VERSION: u32 = 1;

    pub fn save(&self, env: &mut Envelope) {
        env.put(Self::SECTION, Self::VERSION, &self.seed);
    }

    pub fn load(env: &Envelope) -> Result<Option<GameSave>, SaveError> {
        Ok(env.get::<u64>(Self::SECTION, Self::VERSION)?.map(|seed| GameSave { seed }))
    }
}

pub fn plugin(app: &mut App) {
    let crates = Crates::default();
    app.insert_resource(Gameplay::load(&crates));
    app.add_systems(FixedUpdate, (update_pads, watch_pads, gameplay_step, update_readout).chain().after(crate::cargo::budget_step).in_set(crate::phases::Fx::Cargo));
}

pub fn window_plugin(app: &mut App) {
    app.init_resource::<PadRings>();
    app.add_systems(Startup, spawn_panel);
    app.add_systems(Update, update_panel.in_set(crate::phases::Frame::Hud));
    app.add_systems(Update, update_pad_rings.in_set(crate::phases::Frame::World));
}

/// The host's own sender id in events until the network sends each client's; the folder's
/// id is `savefile::LocalClient`.
pub const HOST: ClientId = ClientId(1);
/// Seed of a new game: the text picks (the save keeps it). TODO(initiator): a new game
/// draws its own; fixed keeps the scenarios repeatable.
const NEW_GAME_SEED: u64 = 0x5EED_0001;

#[derive(Component, Clone, Debug)]
pub struct Goods {
    pub id: CrateId,
    pub commodity: CommodityId,
    pub job: JobId,
    pub on_pad: Option<LocationId>,
}

#[derive(Clone, Debug)]
pub struct Pad {
    pub location: LocationId,
    pub centre: DVec3,
    pub up: DVec3,
    pub radius: f64,
}

/// The second abandon press must come within this time (s). TODO(initiator).
const ABANDON_CONFIRM_S: f64 = 3.0;

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
    pub notices: NoticeQueue,
    pub shown: Vec<ShownLine>,
    pub progress: Progress,
    pub jobs: Jobs,
    dedup: Dedup,
    seq: u64,
    next_crate: u64,
    pub pads: Vec<Pad>,
    pub(crate) pads_for: Option<warp_core::PlanetId>,
    queue: Vec<Queued>,
    pub log: Vec<String>,
    pub readout: String,
    clock: f64,
    pub tracked: Option<JobId>,
    pub panel: Option<Panel>,
    /// Briefings already assembled, by offer and the giver's mood then: an offer reads the same
    /// each time it is opened.
    briefings: std::collections::BTreeMap<JobId, (jobs_core::Mood, Briefing)>,
    abandon_asked: Option<(JobId, f64)>,
    pub arrow: Option<Arrow>,
    pub seed: u64,
    pub save_wanted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub giver: GiverId,
    pub page: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arrow {
    pub target: LocationId,
    pub stop: jobs_core::map::Stop,
    /// Turn from where the player looks to the target, degrees, + to the left, on the ground.
    pub bearing_deg: f64,
    pub dist_m: f64,
}

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
    /// error, like the planet recipes. Offers the fixed jobs (no board yet).
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
        let errors = jobs_content.check_texts(&text);
        if !errors.is_empty() {
            fail(errors);
        }
        let progress = Progress::new(&kernel);
        let jobs = Jobs::default();
        let mut g = Gameplay { kernel, jobs_content, text, picker: Picker::new(NEW_GAME_SEED), notices: NoticeQueue::default(), shown: Vec::new(), progress, jobs, dedup: Dedup::default(), seq: 0, next_crate: 1, pads: Vec::new(), pads_for: None, queue: Vec::new(), log: Vec::new(), readout: String::new(), clock: 0.0, panel: None, tracked: None, briefings: Default::default(), abandon_asked: None, arrow: None, seed: NEW_GAME_SEED, save_wanted: false };
        g.refresh_offers();
        g
    }

    pub fn save_to(&self, env: &mut Envelope) {
        KernelState { progress: self.progress.clone(), dedup: self.dedup.clone() }.save(env);
        self.jobs.save(env);
        GameSave { seed: self.seed }.save(env);
    }

    pub fn next_crate(&self) -> u64 {
        self.next_crate
    }

    /// panel and anything queued are gone. Content and pads stay (same build, same planet).
    pub fn restore(&mut self, env: &Envelope, next_crate: u64) -> Result<(), SaveError> {
        let kernel = KernelState::load(env)?.ok_or_else(|| SaveError::Parse("no kernel section".into()))?;
        self.jobs = Jobs::load(env)?.unwrap_or_default();
        self.seed = GameSave::load(env)?.map_or(NEW_GAME_SEED, |g| g.seed);
        // The host's next events continue above the saved ones, so none is taken for a repeat.
        self.seq = kernel.dedup.next_seq(HOST).saturating_sub(1);
        self.picker = Picker::new(self.seed ^ self.seq);
        self.progress = kernel.progress;
        self.dedup = kernel.dedup;
        self.next_crate = next_crate.max(self.next_crate_in_jobs());
        self.notices = NoticeQueue::default();
        self.queue.clear();
        self.panel = None;
        self.tracked = None;
        self.briefings.clear();
        self.abandon_asked = None;
        self.save_wanted = false;
        self.note("loaded the save".into());
        self.refresh_offers();
        Ok(())
    }

    /// One above every crate id the jobs know (a save without a world section).
    fn next_crate_in_jobs(&self) -> u64 {
        self.jobs.all().flat_map(|j| j.legs.iter().flat_map(|l| l.crates.keys().map(|c| c.0 + 1))).max().unwrap_or(1)
    }

    /// English text for a key (the first line of a pool); the key itself when it has none (the tables are not checked yet).
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

    pub fn pad_at(&self, p: DVec3) -> Option<&Pad> {
        self.pads.iter().find(|pad| pad.contains(p))
    }

    pub fn pad_of(&self, l: &LocationId) -> Option<&Pad> {
        self.pads.iter().find(|p| &p.location == l)
    }

    /// active, and none for a once-only job that was completed. (No board yet.)
    pub fn refresh_offers(&mut self) {
        let ids: Vec<_> = self.jobs_content.templates.values().filter(|t| t.record.giver.is_some()).map(|t| t.record.id.clone()).collect();
        for id in ids {
            let t = &self.jobs_content.templates[&id].record;
            let open = self.jobs.all().any(|j| j.template == id && matches!(j.state, JobState::Offered | JobState::Active));
            let done = t.once_only && self.progress.has_flag(&gameplay_core::Flag::new(format!("job_completed:{id}")));
            if !open && !done {
                let t = t.clone();
                self.jobs.offer_fixed(&t);
            }
        }
    }

    pub fn counter_at(&self, p: DVec3) -> Option<GiverId> {
        let pad = self.pad_at(p)?;
        self.jobs_content.givers.values().find(|g| g.record.location == pad.location).map(|g| g.record.id.clone())
    }

    pub fn offers_of(&self, giver: &GiverId) -> Vec<JobId> {
        let Some(g) = self.jobs_content.givers.get(giver) else { return Vec::new() };
        if g.record.available.as_ref().is_some_and(|c| !c.holds(&self.kernel, &self.progress, Some(HOST))) {
            return Vec::new();
        }
        self.jobs
            .all()
            .filter(|j| j.state == JobState::Offered)
            .filter(|j| self.jobs_content.templates.get(&j.template).is_some_and(|t| t.record.giver.as_ref() == Some(giver) && t.record.available.as_ref().is_none_or(|c| c.holds(&self.kernel, &self.progress, Some(HOST)))))
            .map(|j| j.id)
            .collect()
    }

    pub fn panel_offer(&self) -> Option<JobId> {
        let p = self.panel.as_ref()?;
        let offers = self.offers_of(&p.giver);
        (!offers.is_empty()).then(|| offers[p.page % offers.len()])
    }

    pub fn briefing_of(&mut self, job: JobId) -> Option<Briefing> {
        let j = self.jobs.get(job)?;
        let t = &self.jobs_content.templates.get(&j.template)?.record;
        let mood = t.giver.as_ref().map(|g| self.jobs.history(g).mood())?;
        if let Some((m, b)) = self.briefings.get(&job)
            && *m == mood
        {
            return Some(b.clone());
        }
        let history = self.jobs.history(t.giver.as_ref()?);
        // A customer's order has its own title and the customer's own words as the reason.
        let title = j.title_key(t);
        let line = j.order.as_ref().map(|o| TextKey::new(format!("customer.{}.order", o.by)));
        let b = jobs_core::briefing_with(&self.jobs_content, &self.kernel, &self.text, &mut self.picker, t, &j.legs, &history, &title, line.as_ref());
        self.briefings.insert(job, (mood, b.clone()));
        Some(b)
    }

    pub fn open_counter(&mut self, giver: GiverId) {
        self.panel = Some(Panel { giver, page: 0 });
    }

    /// its exam (the exam lends the ship). Without any such licence in the data, always.
    pub fn may_pilot(&self, who: ClientId) -> bool {
        let needed = self.jobs_content.licences_allowing("pilot_ship");
        needed.is_empty()
            || needed.iter().any(|l| {
                self.progress.value(&self.kernel, &l.track, Some(who)).is_some_and(|v| v >= 1) || self.jobs.active().any(|j| j.accepted_by == Some(who) && j.template == l.exam)
            })
    }

    pub fn licence_hint(&self) -> String {
        let exam = self.jobs_content.licences_allowing("pilot_ship").first().map(|l| l.exam.clone());
        let school = exam.and_then(|e| self.jobs_content.templates.get(&e)).and_then(|t| t.record.giver.clone()).and_then(|g| self.jobs_content.givers.get(&g)).map(|g| self.text(&g.record.name));
        format!("needs the flight licence{}", school.map(|s| format!(": exam at {s}")).unwrap_or_default())
    }

    pub fn refresh_tracking(&mut self) {
        if self.tracked.and_then(|j| self.jobs.get(j)).is_some_and(|j| j.state == JobState::Active) {
            return;
        }
        self.tracked = self.jobs.active().next().map(|j| j.id);
    }

    pub fn track_next(&mut self) {
        let active: Vec<JobId> = self.jobs.active().map(|j| j.id).collect();
        if active.is_empty() {
            self.tracked = None;
            return;
        }
        let at = self.tracked.and_then(|t| active.iter().position(|j| *j == t));
        self.tracked = Some(active[at.map_or(0, |i| (i + 1) % active.len())]);
    }

    /// within `ABANDON_CONFIRM_S` drops it. Nothing tracked, nothing happens.
    pub fn press_abandon(&mut self) {
        let Some(job) = self.tracked.filter(|j| self.jobs.get(*j).is_some_and(|j| j.state == JobState::Active)) else { return };
        match self.abandon_asked {
            Some((asked, at)) if asked == job && self.clock - at <= ABANDON_CONFIRM_S => {
                self.abandon_asked = None;
                self.push_job(HOST, JobEvent::JobAbandoned { job });
            }
            _ => {
                self.abandon_asked = Some((job, self.clock));
                let j = &self.jobs.get(job).unwrap();
                let title = j.title_key(&self.jobs_content.templates[&j.template].record);
                self.notify(Notice::new(NoticeKind::Warning, "notice.job.abandon_ask").arg("title", Arg::Key(title)));
            }
        }
    }

    pub fn close_counter(&mut self) {
        self.panel = None;
    }

    pub fn panel_text(&mut self, keys: &str) -> String {
        let Some(p) = self.panel.clone() else { return String::new() };
        let Some(job) = self.panel_offer() else {
            let name = self.jobs_content.givers.get(&p.giver).map(|g| self.text(&g.record.name)).unwrap_or_default();
            return format!("{name}\nNothing for you right now.");
        };
        let Some(b) = self.briefing_of(job) else { return String::new() };
        let pay = self.offer_pay(job);
        let n = self.offers_of(&p.giver).len();
        // An exam tells what it asks in its own words (it is the tutorial); other jobs say where from and to.
        let body = match self.jobs.get(job).and_then(|j| self.jobs_content.templates.get(&j.template)).filter(|t| t.record.exam.is_some()) {
            Some(t) => self.text(&t.record.brief),
            None => b.paragraph.clone(),
        };
        format!("{}\n\n{}\n{}\n{}\n\n{}  ({pay})\n{keys}{}", b.title, b.greeting, b.intro, body, b.reason, if n > 1 { format!("   ({} of {n})", p.page % n + 1) } else { String::new() })
    }

    fn offer_pay(&self, job: JobId) -> String {
        // An exam costs a fee, half of it after the first try.
        if let Some(j) = self.jobs.get(job)
            && let Some(x) = self.jobs_content.templates.get(&j.template).and_then(|t| t.record.exam.as_ref())
        {
            let fee = if self.jobs.attempts(HOST, &j.template) == 0 { x.fee } else { x.retry_fee };
            return format!("fee {fee} {}", self.text(&TextKey::new("track.wallet.name")));
        }
        let reward = self.jobs.get(job).and_then(|j| self.jobs_content.templates.get(&j.template).map(|t| j.order.as_ref().map_or(t.record.reward, |o| o.reward))).unwrap_or(0);
        format!("{reward} {}", self.text(&TextKey::new("track.wallet.name")))
    }

    pub fn offer_label(&self, job: JobId) -> String {
        let Some(j) = self.jobs.get(job) else { return String::new() };
        let Some(t) = self.jobs_content.templates.get(&j.template) else { return String::new() };
        let reward = j.order.as_ref().map_or(t.record.reward, |o| o.reward);
        if let Some(x) = &t.record.exam {
            let fee = if self.jobs.attempts(HOST, &j.template) == 0 { x.fee } else { x.retry_fee };
            return format!("{} (fee {fee} {})", self.text(&t.record.title), self.text(&TextKey::new("track.wallet.name")));
        }
        format!("{} ({} {})", self.text(&j.title_key(&t.record)), reward, self.text(&TextKey::new("track.wallet.name")))
    }

    fn note(&mut self, s: String) {
        println!("gameplay: {s}");
        self.log.push(s);
    }

    pub fn line(&mut self, pool: &str) -> String {
        self.picker.pick(&self.text, pool)
    }

    /// The game clock (s).
    pub fn clock(&self) -> f64 {
        self.clock
    }

    pub fn notify(&mut self, n: Notice) {
        self.notices.push(n);
    }

    pub fn render(&mut self, n: &Notice) -> String {
        self.picker.render(&self.text, n)
    }

    fn pace_notices(&mut self, dt: f64) {
        for s in self.notices.tick(dt) {
            let text = self.picker.render(&self.text, &s.notice);
            let money = s.notice.args.iter().find_map(|(k, a)| if let ("money", Arg::Number(n)) = (k.as_str(), a) { Some(*n) } else { None });
            println!("gameplay: [{}] {text}", if s.banner { "banner" } else { "toast" });
            self.shown.push(ShownLine { kind: s.notice.kind, key: s.notice.key.to_string(), text, banner: s.banner, seconds: s.seconds, money, at: self.clock });
        }
    }

    fn refused(&mut self, r: &jobs_core::Refusal) {
        let key = match r {
            jobs_core::Refusal::TooManyActive => "notice.refused.too_many",
            jobs_core::Refusal::NotAvailable => "notice.refused.not_available",
            jobs_core::Refusal::CannotAfford { .. } => "notice.refused.cannot_afford",
            _ => return,
        };
        self.notify(Notice::new(NoticeKind::Warning, key));
    }
}

pub fn grant_starting_licences(mut gp: ResMut<Gameplay>) {
    let tracks: Vec<_> = gp.jobs_content.licences.values().map(|l| l.record.track.clone()).collect();
    for track in tracks {
        gp.push_world(HOST, WorldEvent::TrackChanged { track, delta: 1, player: Some(HOST) });
    }
}

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

pub fn gameplay_step(mut commands: Commands, time: Res<Time>, table: Res<Crates>, mut actions: ResMut<crate::controls::Actions>, mut gp: ResMut<Gameplay>, goods: Query<(Entity, &Goods)>) {
    gp.clock += time.delta_secs_f64();
    let dt = time.delta_secs_f64();
    if actions.take_tap(crate::controls::Tap::SkipNotices) {
        gp.notices.skip();
    }
    if actions.take_tap(crate::controls::Tap::AbandonJob) {
        gp.press_abandon();
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
            g.save_wanted |= !outcomes.is_empty();
            for o in outcomes {
                carry_out(&mut commands, &table, g, &goods, o);
            }
        }
    }
    gp.refresh_offers();
    gp.refresh_tracking();
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

pub fn pointer(name: &str, from: DVec3, look: DVec3, to: DVec3, up: DVec3) -> String {
    let (ang, dist) = bearing(from, look, to, up);
    let side = if dist < 40.0 {
        "here"
    } else {
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

/// Turn from `look` to `to` on the ground under `up` (degrees, + to the left) and the distance.
pub fn bearing(from: DVec3, look: DVec3, to: DVec3, up: DVec3) -> (f64, f64) {
    let d = to - from;
    let flat = |v: DVec3| (v - up * v.dot(up)).normalize_or_zero();
    let (f, t) = (flat(look), flat(d));
    (f.cross(t).dot(up).atan2(f.dot(t)).to_degrees(), d.length())
}

pub fn update_readout(
    mut gp: ResMut<Gameplay>,
    players: Query<&crate::walker::Player>,
    ships: Query<(&avian3d::prelude::Position, &avian3d::prelude::Rotation), With<crate::ship::Ship>>,
    planet: Res<PlanetRes>,
) {
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
    let arrow = gp.tracked.and_then(|t| gp.jobs.get(t)).filter(|j| j.state == JobState::Active).and_then(jobs_core::map::next_stop).and_then(|(target, stop)| {
        let (pos, look) = view?;
        let pad = gp.pad_of(&target)?;
        let (bearing_deg, dist_m) = bearing(pos, look, pad.centre, planet.up(pos));
        Some(Arrow { target, stop, bearing_deg, dist_m })
    });
    gp.arrow = arrow;
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
        if gp.tracked == Some(j.id)
            && let Some((loc, stop)) = jobs_core::map::next_stop(j)
            && let Some(p) = point(&gp, &loc)
        {
            s += &format!("\n{} {p}", if stop == jobs_core::map::Stop::Pickup { "pick up:" } else { "deliver to:" });
        }
    }
    if gp.jobs.active().next().is_none()
        && let Some(g) = gp.jobs_content.givers.values().map(|g| &g.record).find(|g| !gp.offers_of(&g.id).is_empty() && gp.pad_of(&g.location).is_some())
        && let Some(p) = point(&gp, &g.location)
    {
        s += &format!("\njobs at the counter: {p}");
    }
    gp.readout = s;
}

#[derive(Component)]
struct PanelText;

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        PanelText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(17.0), ..default() },
        TextColor(Color::srgb(0.95, 0.95, 0.85)),
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.7)),
        Node { position_type: PositionType::Absolute, left: px(16), top: percent(30), max_width: px(420), padding: UiRect::all(px(12)), display: Display::None, ..default() },
    ));
}

fn update_panel(mut gp: ResMut<Gameplay>, bindings: Res<crate::controls::Bindings>, mut q: Query<(&mut Text, &mut Node), With<PanelText>>) {
    use crate::controls::Tap;
    use crate::interact::{key_hint, key_label};
    let keys = format!("[{}] take the job {}{}", key_label(&bindings, Tap::Interact), key_hint(&bindings, Tap::NextOffer, "next"), key_hint(&bindings, Tap::Decline, "decline"));
    let s = gp.panel_text(&keys);
    if let Ok((mut t, mut n)) = q.single_mut() {
        n.display = if s.is_empty() { Display::None } else { Display::Flex };
        if **t != s {
            **t = s;
        }
    }
}

#[derive(Resource, Default)]
pub struct PadRings(Option<warp_core::PlanetId>);

fn update_pad_rings(mut commands: Commands, planet: Res<PlanetRes>, gp: Res<Gameplay>, mut rings: ResMut<PadRings>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, assets: Res<AssetServer>) {
    if rings.0 == Some(planet.id) || gp.pads_for != Some(planet.id) {
        return;
    }
    rings.0 = Some(planet.id);
    let mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.85, 0.3), emissive: LinearRgba::rgb(4.0, 2.6, 0.6), ..default() });
    let pad_model = assets.load(GltfAssetLabel::Scene(0).from_asset("props/landing_pad.glb"));
    for p in &gp.pads {
        // The disc's deck is 0.4 m up; sink it so the deck sits just above the flattened ground.
        commands.spawn((
            WorldAssetRoot(pad_model.clone()),
            Transform { rotation: Quat::from_rotation_arc(Vec3::Y, p.up.as_vec3()), scale: Vec3::splat(p.radius as f32 / 10.0), ..default() },
            crate::origin::WorldPos(p.centre - p.up * 0.3),
            crate::terrain::PlanetScene(planet.id),
        ));
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

    #[test]
    fn bearing_turns_left_positive_and_ignores_height() {
        let (up, look) = (DVec3::Y, DVec3::NEG_Z);
        let b = |to: DVec3| bearing(DVec3::ZERO, look, to, up);
        assert!(b(DVec3::new(0.0, 0.0, -100.0)).0.abs() < 1e-9);
        assert!((b(DVec3::new(-100.0, 0.0, 0.0)).0 - 90.0).abs() < 1e-9);
        assert!((b(DVec3::new(100.0, 0.0, 0.0)).0 + 90.0).abs() < 1e-9);
        assert!((b(DVec3::new(0.0, 0.0, 100.0)).0.abs() - 180.0).abs() < 1e-9);
        let (a, d) = b(DVec3::new(-100.0, 100.0, -100.0));
        assert!((a - 45.0).abs() < 1e-9 && (d - 300f64.sqrt() * 10.0).abs() < 1e-9);
    }
}
