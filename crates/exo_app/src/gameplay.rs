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
use gameplay_core::save::{Envelope, KernelState, SaveError};
use customers_core::{CustomerContent, Customers};
use jobs_core::{Briefing, GiverId, JobContent, JobEvent, JobId, JobState, Jobs, Outcome};

use crate::cargo::{crate_bundle, Crate, Crates};
use crate::env::{from_v3, PlanetRes};
use crate::grab::Grab;

/// The game's own section of the save (#135): its seed.
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

/// Window only: pad rings and the job line.
pub fn window_plugin(app: &mut App) {
    app.init_resource::<PadRings>();
    app.add_systems(Startup, (spawn_job_line, spawn_panel));
    app.add_systems(Update, update_panel.in_set(crate::phases::Frame::Hud));
    app.add_systems(Update, update_pad_rings.in_set(crate::phases::Frame::World));
    app.add_systems(Update, update_job_line.in_set(crate::phases::Frame::Hud));
}

/// The host's own sender id in events until the network sends each client's (#134); the folder's
/// id is `savefile::LocalClient` (#135).
pub const HOST: ClientId = ClientId(1);
/// Seed of a new game's customer orders (a loaded game continues the saved draws).
const CUSTOMER_SEED: u64 = 0xC057_0001;
/// Seed of a new game: the text picks (#135 keeps it in the save). TODO(initiator): a new game
/// draws its own; fixed keeps the scenarios repeatable.
const NEW_GAME_SEED: u64 = 0x5EED_0001;

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
    /// Named buyers: their orders become offers of the wholesaler (#168).
    pub customers: Customers,
    pub customer_content: CustomerContent,
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
    /// The job whose next stop is the pointer and the map's target (#166).
    pub tracked: Option<JobId>,
    /// The counter the player has open: a giver's briefing with accept and decline (#167).
    pub panel: Option<Panel>,
    /// Briefings already assembled, by offer and the giver's mood then: an offer reads the same
    /// each time it is opened.
    briefings: std::collections::BTreeMap<JobId, (jobs_core::Mood, Briefing)>,
    /// This game's seed (#135, in the save).
    pub seed: u64,
    /// Something happened to a job since the last save: the autosave writes soon (#135).
    pub save_wanted: bool,
}

/// A giver's counter, opened.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub giver: GiverId,
    /// Which of the giver's offers is shown.
    pub page: usize,
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
        let customer_content = CustomerContent::load(&files, &kernel).unwrap_or_else(|e| fail(e));
        let mut errors = jobs_content.check_texts(&text);
        errors.extend(customer_content.check_texts(&text));
        errors.extend(check_prices(&kernel, &jobs_content, &customer_content));
        if !errors.is_empty() {
            fail(errors);
        }
        let customers = Customers::new(&customer_content, CUSTOMER_SEED);
        let progress = Progress::new(&kernel);
        let jobs = Jobs::default();
        let mut g = Gameplay { kernel, jobs_content, text, picker: Picker::new(NEW_GAME_SEED), notices: NoticeQueue::default(), shown: Vec::new(), progress, jobs, customers, customer_content, dedup: Dedup::default(), seq: 0, next_crate: 1, pads: Vec::new(), pads_for: None, queue: Vec::new(), log: Vec::new(), readout: String::new(), clock: 0.0, panel: None, tracked: None, briefings: Default::default(), seed: NEW_GAME_SEED, save_wanted: false };
        g.refresh_offers();
        g
    }

    /// The gameplay sections of a save (#135): kernel, jobs, customers and the game's seed. The
    /// world (crates, ship) is `savefile::snapshot`'s.
    pub fn save_to(&self, env: &mut Envelope) {
        KernelState { progress: self.progress.clone(), dedup: self.dedup.clone() }.save(env);
        self.jobs.save(env);
        self.customers.save(env);
        GameSave { seed: self.seed }.save(env);
    }

    /// The next crate id to hand out.
    pub fn next_crate(&self) -> u64 {
        self.next_crate
    }

    /// A restart from a save (#135): progress, jobs, customers and seed from `env`; notices, the
    /// panel and anything queued are gone. Content and pads stay (same build, same planet).
    pub fn restore(&mut self, env: &Envelope, next_crate: u64) -> Result<(), SaveError> {
        let kernel = KernelState::load(env)?.ok_or_else(|| SaveError::Parse("no kernel section".into()))?;
        self.jobs = Jobs::load(env)?.unwrap_or_default();
        self.customers = Customers::load(env)?.unwrap_or_else(|| Customers::new(&self.customer_content, CUSTOMER_SEED));
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
        self.save_wanted = false;
        self.note("loaded the save".into());
        self.refresh_offers();
        Ok(())
    }

    /// One above every crate id the jobs know (a save without a world section).
    fn next_crate_in_jobs(&self) -> u64 {
        self.jobs.all().flat_map(|j| j.legs.iter().flat_map(|l| l.crates.keys().map(|c| c.0 + 1))).max().unwrap_or(1)
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

    /// Makes sure every giver's fixed job is on offer: one offer per template while it is not
    /// active, and none for a once-only job that was completed. (No board yet, #126.)
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

    /// The giver whose counter is at the pad under `p`.
    pub fn counter_at(&self, p: DVec3) -> Option<GiverId> {
        let pad = self.pad_at(p)?;
        self.jobs_content.givers.values().find(|g| g.record.location == pad.location).map(|g| g.record.id.clone())
    }

    /// The giver's offers the crew may take now: open giver, template condition holds.
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

    /// The offer the open panel shows.
    pub fn panel_offer(&self) -> Option<JobId> {
        let p = self.panel.as_ref()?;
        let offers = self.offers_of(&p.giver);
        (!offers.is_empty()).then(|| offers[p.page % offers.len()])
    }

    /// The briefing of an offer, assembled once per mood of its giver.
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

    /// The counter opens: the giver's greeting and the first offer.
    pub fn open_counter(&mut self, giver: GiverId) {
        self.panel = Some(Panel { giver, page: 0 });
    }

    /// Whether the player may take the seat: with a licence that allows piloting, or while taking
    /// its exam (the exam lends the ship). Without any such licence in the data, always (#169).
    pub fn may_pilot(&self, who: ClientId) -> bool {
        let needed = self.jobs_content.licences_allowing("pilot_ship");
        needed.is_empty()
            || needed.iter().any(|l| {
                self.progress.value(&self.kernel, &l.track, Some(who)).is_some_and(|v| v >= 1) || self.jobs.active().any(|j| j.accepted_by == Some(who) && j.template == l.exam)
            })
    }

    /// Where to take the exam, for the prompt and the refusal.
    pub fn licence_hint(&self) -> String {
        let exam = self.jobs_content.licences_allowing("pilot_ship").first().map(|l| l.exam.clone());
        let school = exam.and_then(|e| self.jobs_content.templates.get(&e)).and_then(|t| t.record.giver.clone()).and_then(|g| self.jobs_content.givers.get(&g)).map(|g| self.text(&g.record.name));
        format!("needs the flight licence{}", school.map(|s| format!(": exam at {s}")).unwrap_or_default())
    }

    /// Keeps the tracked job an active one: a job that ended is replaced by the next active job.
    pub fn refresh_tracking(&mut self) {
        if self.tracked.and_then(|j| self.jobs.get(j)).is_some_and(|j| j.state == JobState::Active) {
            return;
        }
        self.tracked = self.jobs.active().next().map(|j| j.id);
    }

    /// Tracks the next active job after the tracked one (T).
    pub fn track_next(&mut self) {
        let active: Vec<JobId> = self.jobs.active().map(|j| j.id).collect();
        if active.is_empty() {
            self.tracked = None;
            return;
        }
        let at = self.tracked.and_then(|t| active.iter().position(|j| *j == t));
        self.tracked = Some(active[at.map_or(0, |i| (i + 1) % active.len())]);
    }

    /// Declines: the panel closes, the offer stays.
    pub fn close_counter(&mut self) {
        self.panel = None;
    }

    /// The panel's text: giver, briefing, pay and the keys; empty when no counter is open.
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

    /// "First haul (300 credits)".
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
            jobs_core::Refusal::CannotAfford { .. } => "notice.refused.cannot_afford",
            _ => return,
        };
        self.notify(Notice::new(NoticeKind::Warning, key));
    }
}

/// Startup system of the scenarios: every licence is earned for the host's client.
pub fn grant_starting_licences(mut gp: ResMut<Gameplay>) {
    let tracks: Vec<_> = gp.jobs_content.licences.values().map(|l| l.record.track.clone()).collect();
    for track in tracks {
        gp.push_world(HOST, WorldEvent::TrackChanged { track, delta: 1, player: Some(HOST) });
    }
}

/// The one price table against the records it mirrors: customers pay the commodity's base price,
/// the courier rewards are the templates' (#168). Errors for what differs.
fn check_prices(k: &Content, jc: &JobContent, cc: &CustomerContent) -> Vec<String> {
    let p = &cc.prices;
    let mut e = Vec::new();
    for (c, price) in &p.record.customer {
        if let Some(com) = k.commodities.get(c)
            && com.record.base_price != *price
        {
            e.push(format!("{}: customer: '{c}' is {price} here but the commodity's base_price is {}", p.path, com.record.base_price));
        }
    }
    for (t, price) in &p.record.courier {
        match jc.templates.get(&jobs_core::TemplateId::new(t.clone())) {
            None => e.push(format!("{}: courier: no job template '{t}'", p.path)),
            Some(tpl) if tpl.record.reward != *price => e.push(format!("{}: courier: '{t}' is {price} here but the template pays {}", p.path, tpl.record.reward)),
            Some(_) => {}
        }
    }
    for t in jc.templates.values().filter(|t| t.record.id.as_str().starts_with("courier_")) {
        if !p.record.courier.contains_key(t.record.id.as_str()) {
            e.push(format!("{}: courier: no price for the template '{}'", p.path, t.record.id));
        }
    }
    e
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
                    // Customers read time and settled orders; their orders go back in as events.
                    for o in g.customers.apply_world(&g.customer_content, &g.kernel, &ev) {
                        match o {
                            customers_core::Outcome::Emit(e) => g.push_world(HOST, e),
                            customers_core::Outcome::Notice(n) => g.notify(n),
                        }
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
            // A job changed: the autosave writes soon (#135).
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
        // Where to next (only for the tracked job, T switches): the pickup while crates wait, else the dropoff.
        if gp.tracked == Some(j.id)
            && let Some((loc, stop)) = jobs_core::map::next_stop(j)
            && let Some(p) = point(&gp, &loc)
        {
            s += &format!("\n{} {p}", if stop == jobs_core::map::Stop::Pickup { "pick up:" } else { "deliver to:" });
        }
    }
    // With nothing active: where a giver's counter with jobs on offer is.
    if gp.jobs.active().next().is_none()
        && let Some(g) = gp.jobs_content.givers.values().map(|g| &g.record).find(|g| !gp.offers_of(&g.id).is_empty() && gp.pad_of(&g.location).is_some())
        && let Some(p) = point(&gp, &g.location)
    {
        s += &format!("\njobs at the counter: {p}");
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

#[derive(Component)]
struct PanelText;

/// The giver's counter: a panel on the left with the briefing. It takes no input itself (the
/// interact, next and decline taps do), so nobody is blocked by it.
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
    use crate::interact::key_label;
    let keys = format!("[{}] take the job   [{}] next   [{}] decline", key_label(&bindings, Tap::Interact), key_label(&bindings, Tap::NextOffer), key_label(&bindings, Tap::Decline));
    let s = gp.panel_text(&keys);
    if let Ok((mut t, mut n)) = q.single_mut() {
        n.display = if s.is_empty() { Display::None } else { Display::Flex };
        if **t != s {
            **t = s;
        }
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

    /// #170: the courier drops lie 150 to 400 m from the start pad (the Drip Rock counter). They
    /// are placed in metres from it, so a bigger planet keeps the distances (`planet_core`'s
    /// places tests check that at several radii).
    #[test]
    fn courier_drops_are_a_walk_from_drip_rock() {
        let sys = warp_core::System::from_json(crate::warp::SYSTEM).unwrap();
        let home = warp_core::PlanetId(0);
        let gp = Gameplay::load(&Crates::default());
        let drops: Vec<_> = gp.kernel.locations.values().filter(|l| l.record.tags.iter().any(|t| t.as_str() == "courier_drop")).collect();
        assert_eq!(drops.len(), 3);
        let planet = PlanetRes::load(home, sys.planet(home));
        let start = planet.pgen.pad("drip_rock", "main").unwrap().centre;
        for l in &drops {
            let pad = planet.pgen.pad(&l.record.place, &l.record.pad).unwrap();
            let d = planet.radius * start.normalized().dot(pad.centre.normalized()).clamp(-1.0, 1.0).acos();
            assert!((150.0..=400.0).contains(&d), "{}: {d:.0} m from Drip Rock", l.path);
        }
    }

    /// #167: the family is a giver of the cast but offers nothing in D; the courier office does.
    #[test]
    fn the_family_offers_nothing_yet_and_the_courier_office_does() {
        let gp = Gameplay::load(&Crates::default());
        assert!(gp.offers_of(&GiverId::new("small_family")).is_empty());
        assert!(!gp.offers_of(&GiverId::new("courier_office")).is_empty());
        assert_eq!(gp.jobs_content.givers.len(), 4, "courier office, small family, wholesaler, flight school");
        assert_eq!(gp.customer_content.customers.len(), 3);
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
