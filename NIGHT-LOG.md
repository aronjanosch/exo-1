# NIGHT-LOG: milestone D night run, 2026-10-10

Morning report of the unattended run (brief: concept repo `docs/RUN-D-NIGHT-BRIEF.md`). Phase 1 on `night/d-foundation`, phase 2 (extras) on `night/d-extras`. Nothing merged, no PR, no issue closed. The old C log is in git history.

## Checklist

Phase 1, `night/d-foundation`:

- [x] #128 save model (kernel envelope, kernel/jobs/world sections)
- [x] #165 feedback beats
- [x] #167 givers
- [x] #170 courier jobs
- [x] #168 customers
- [x] #169 flight licence
- [x] #166 map (screenshot: `night-shots/map.png` in the worktree, not committed)
- [x] #135 save file (done in the day session of 2026-10-10, see "Day session")

Phase 2, `night/d-extras` (day session, from the tip of `night/d-foundation`):

- [x] #126 board generator in `jobs_core` (built and tested, not wired into the game: the counters stay, as decided)
- [x] #131 text keys checked against `en.json`, fuller pools
- [x] #136 HUD target arrow (money and XP were on the job line already)
- [x] #132 reduced by the initiator to an abandon key on the tracked job (no board menu: "the board stays later")
- [x] `production_core` sketch (recipe record, station state machine, save section)
- [ ] better synthesized sounds: not done
- [ ] other backlog items: not done

## How the run ended

The session died at about 04:08 on 2026-10-10 while the gate for #166 was starting; it was resumed at 10:26, after the 08:30 stop time. Cause (from the host's `appdata.backup` log, told by the initiator): not a crash and not out of memory. The Appdata Backup plugin stops the `roost` container on purpose every day at 03:00 (stop, back up, start): it stopped it at 04:08:11, the backup and its verify took about 95 minutes, and it started again at 05:43:18. All herdr and Claude processes in the container ended with it, and nothing started them again afterwards. (My first guess in this log, an out-of-memory kill, was wrong. The 6 GB limit and the 4.9 GB peak of a gate run are real, from an older incident on 2026-10-08, so `CARGO_BUILD_JOBS` at most 4 and nextest `--test-threads` 3 to 4 stay good advice.) The earlier night-C stop at 03:57 may be the same backup. The #166 gate was rerun after the resume with 3 jobs and 3 test threads. Not done: #135, phase 2.

## #128 save model

Built: `gameplay_core::save`: `Envelope` (version + named sections, each with its own version; text in/out, no file I/O), `KernelState` (progress + dedup, `apply` ignores ids it has seen, also after a load), `WorldSave` (crates of active jobs with condition and place in the planet frame or the ship frame, ship pose, next crate id). `Dedup::next_seq` lets a restarted client continue its numbering above the saved ids. `jobs_core::Jobs::save` / `load` is the jobs section. A wrong envelope or section version is `SaveError`, never a guess; a missing section loads as none.

Checks: 7 tests in `gameplay_core/tests/save.rs`, 1 in `jobs_core/tests/jobs.rs` (half-played job saved, loaded into a second host, both finish with equal state). Gate: `cargo t` 358 passed, `cargo scenario` 0 failures.

TODO(initiator): none. Open: board offers "with seed and lifetime" have nothing to save until the board exists (#126); every later system brings its own section (each with a round-trip test).

## #165 feedback beats

Built: kernel `gameplay_core::notice` (`Notice` with kind, text key, args, weight; `NoticeQueue`: one notice at a time with a gap, never two banners at once, rank and unlock held until the objectives were quiet for 4 s, `skip` runs the rest fast), `text` (`TextTable` where a key is one line or a pool, `Picker` that never gives the same line twice in a row, seeded), `rng` (splitmix64), `Progress::apply_with_notices` (unlock bought, track level reached). `jobs_core` returns `Outcome::Notice` for accepted, picked up, delivered, crate lost, abandoned, completed or expired, and the itemised payout (base, share, condition, hazard: the lines add up to the pay; then XP). Glue: `gameplay.rs` no longer writes note lines to the player, it queues notices and renders them from `content/gameplay/text/en.json` (pools now); `exo_app::notices` draws one banner (a payout counts up) and a toast stack in the window; `audio.rs` has five new synthesized sounds (ping, accept, coin, fanfare, buzz) picked by the notice kind; new tap `skip_notices` (Enter, in `bindings.json`, old binding files still load).

Checks: 22 new tests (kernel 11, jobs 4, exo_app 3 unit), scenario `deliver` extended: beats in order, three delivered beats and one completed, payout in the banner, never two banners at once, no line twice in a row, queue empty after the ritual, skip empties the queue within 4 s (23 notices). Gate: `cargo t` 376 passed, `cargo scenario` 0 failures. Window drawing and sounds are not run headless; checked by the screenshot of the map run (#166) at the end.

TODO(initiator): all pacing times (gap 0.6 s, banner 3 s, toast 2.5 s, calm 4 s), the skip key, sound volumes and shapes, every text in `en.json` (placeholders), the count-up time (1.2 s).
Open: the XP line names the track by the convention `track.<id>.name`; the standing line of the ritual comes with #167.

## #167 givers

Built: `jobs_core::giver` (record `giver`: kind legal or family, counter location, voice pools with greetings by mood, standing track, gain and loss, optional `available` condition), `jobs_core::briefing` (title formula, greeting by mood, intro, paragraph by shape of the job timed/many/few, reason from the cargo, else the destination's route tag, else the giver's pool, sign-off; all picks by seed through the `Picker`), template field `giver`, `Jobs` history per giver (completed count, failure streak, saved with the jobs section), standing: a completed job raises the giver's crew track by `gain`, a failed one (expired, or nothing delivered) lowers it by `loss`; new track field `min` (floor 0) so standing never locks a giver for good and work brings it back; abandoning costs nothing. A closed giver (`available` flag) offers nothing (the family: flag `family_open`, never raised in D). Rank thresholds of the standing track unlock templates through the existing `available` condition. `check_texts` reports missing voice pools and pools with fewer than 3 lines (so no line repeats twice in a row by lack of choice). The ritual names standing gained or lost.
Glue: counters at the giver's pad: prompt "talk to <giver>", F opens a panel on the left (giver, briefing, pay; Tab next offer, Backspace declines, F takes the job, walking away closes it; it never takes input). The fixed jobs are offered per template again after a completion (no board yet). Content: givers `courier_office` ("Dinglepost Couriers", counter at Drip Rock) and `small_family` ("The Gribbles", counter at Bent Spoon for now, moves in #170), standing tracks, a family placeholder job, about 60 new text lines.

Checks: 14 tests in `jobs_core/tests/givers.rs` (loader errors with file and field, missing and thin pools, deterministic briefing, reason order, shape, mood, no repeat over 100 briefings, standing up/down/floor/recovery, rank unlocks a template, closed giver, history in the save, standing lines in the ritual), 2 kernel tests for the floor, 1 exo_app test. Scenario `deliver` now goes through the counter: prompt, briefing, same text when asked again, decline keeps the offer, take, standing +10 and mood regular after the job. Gate: `cargo t` 393 passed, `cargo scenario` 0 failures.

TODO(initiator): giver names and every voice, briefing and reason line (placeholders, tone silly), gain 10 and loss 15, standing ranks 30/100/250, the family's counter place, panel layout and keys (Tab, Backspace).
Open: no board yet, so one fixed offer per template; the panel is plain text.

## #170 courier jobs

Built: new optional place field `near: { place, east_m, north_m }` (`planet_core::Place`): a place given in metres east and north of an absolute place (east and north as on the ground there), resolved by `Planet::set_places` on the planet's radius, so walking distances stay the same when the radius changes (#177). Either `lat_deg` and `lon_deg`, or `near`; a reference that is itself near, or unknown, is an error naming the place. Three new places `lint_trap` (about 180 m from Drip Rock), `noodle_post` (about 280 m), `pebble_kiosk` (about 345 m), each with a pad and a flatten edit, three locations tagged `courier_drop`, a new commodity `parcel` (small crate, price 20), three courier templates from the courier office counter: one parcel to the Lint Trap (30), two to the Noodle Post (45), three to the Pebble Kiosk with a 240 s deadline (60), repeatable (offered again at once). Texts for them, a README line.

Checks: 4 new tests in `planet_core/tests/places.rs` (distance and direction at radii 3000, 5000, 6500 and 20000 without bake, the baked pad at 5000, flatten and sites, the error cases), `exo_app` test that the drops are 150 to 400 m from Drip Rock. New scenario `courier` (and `scenario_courier.rs` in the gate): at the counter, Tab to the offer, take it, grab the parcel, the walker walks the whole way (nothing moved by test hooks): 180 m in 36 s to the Lint Trap, set down, paid 29 of 30 (a hand-drop costs a hair of condition), standing +10, the beats of #165 in order; then the timed job: 342 m in 70 s to the Pebble Kiosk, one of three parcels delivered, the job stays open. Gate: `cargo t` 398 passed, `cargo scenario` 0 failures.

TODO(initiator): place names and positions (placeholders until the city, E), prices and rewards (30, 45, 60), the deadline 240 s, the parcel good, all texts. Open question: the decision says a courier job takes about 2 to 3 minutes on foot; at the walking speed measured here one way is 36 to 70 s. For 2 to 3 minutes the drops would have to be farther (400 m is the limit of the decision) or a job needs a return leg; I kept the 150 to 400 m rule.
Note: the first haul (`first_haul`) stays as it was, 300 credits to Bent Spoon; the flight licence (#169) will gate it.

## #168 customers

Built: new crate `customers_core` (no Bevy; its only dev-dependency is `jobs_core`, used by the test host the way the glue wires them). Record `customer` (name, location, taste tags over commodities, condition standard, order rhythm, amount, patience, start relationship, voice pools order/thanks/grumble) and the single record `price_table`, checked by the loader (every good a customer may order has a wholesale and a customer price, the customer pays more than the wholesaler asks, a wholesaler location exists). `Customers`: seeded orders by rhythm (scaled by relationship: strangers wait twice as long, friends a quarter), one open order per customer, goods of their taste from the wholesale location to their pad, reward = crates × price × (1 + 4 % per relationship point); satisfaction from share, condition against the customer's standard and time (about -0.9 to +0.7) moves the relationship (0 to 5); neglect wears it down by 0.1 per 600 s without a pleasing delivery, to a floor of 1.0 and no further; they answer in their own voice (thanks or grumble pool). Section `customers` in the save, with a round-trip test.
Events: kernel `WorldEvent::OrderPlaced` and `OrderSettled` (plain data, `OrderId`). `jobs_core` turns `OrderPlaced` into an offer of the template `customer_order` with the order's places, goods, reward and deadline (`Job::order`, `OrderTerms`), pays the order's reward, and raises `OrderSettled` when the job ends (delivered, expired or abandoned). The customer's title and words show in the offer and the briefing.
Glue and content: wholesaler place `slosh_wholesale` (about 290 m from Drip Rock) with a giver counter showing the orders, three goods (`fizzy_mud` wet, `sock_dust` dry, `grumble_jelly` sticky), three customers at existing places (`mabel_snood` at Noodle Post, `captain_pip` at Pebble Kiosk, `moss_committee` at Lint Trap), the price table `price_table/start.json` (wholesale, customer and courier prices in one place; the glue checks customer prices against the commodities' base prices and the courier prices against the templates, so one number cannot drift), texts.

Checks: 17 tests in `customers_core/tests/customers.rs` (loader errors, pools, determinism for a seed, the first order within the rhythm, route and goods, notices, satisfaction, relationship bounds, regulars order more and pay a little more, neglect floor and winning back, repeats, the chain order → offer → delivery → satisfaction and the expiry case with the jobs system, save round trip), 7 in `jobs_core/tests/orders.rs`, 1 in the kernel. Scenario `customers` (in the gate): Mabel's order appears as an offer, the counter shows it in her words, delivery pays the order's reward (32), relationship 2.00 to 2.70, her thanks, next order due within her rhythm; a second order dropped from 30 m wrecks the goods: relationship 2.70 to 2.30, her grumble. Gate: `cargo t` 424 passed, `cargo scenario` 0 failures.

TODO(initiator): every name and text (customers, goods, wholesaler), prices (one table), rhythms, standards, patience, the relationship formulas (+4 % pay, rhythm factor 2.0 to 0.25, satisfaction weights, neglect 0.1 per 600 s to a floor of 1.0), the wholesaler's gain/loss.
Open: an order nobody accepts stays on the board and the customer waits (no withdrawal in D, so a customer with an unserved order does not order again; the relationship still decays to the floor); the customers sit at the places of the courier drops until the city is placed (E).

## #169 flight licence

Built: kernel events `TookOff`, `PadReached { at }`, `Landed { at, speed }` (plain data; the exam reads only these, never how the ship flies). `jobs_core`: new objective kinds `take_off`, `reach_pad`, `land` (a touchdown faster than `max_mps` is a crash) next to `deliver`; a job carries `checks` (done in order, only from the player who took the job, the clock starts at take-off); template block `exam` (fee, retry fee for every later try of the same player, the personal track it grants, minimum crate condition, optional honours with a touchdown limit, a time and a standing bonus with a giver); record `licence` (id, name, personal track, exam template, what it allows, here `pilot_ship`); a new job state `Failed` (crash, a crate too damaged); fee charged on accepting, refused when the crew cannot pay (`CannotAfford`); exam XP only when passed. Passing sets the licence track to 1 for the examinee only (per player), flag `exam_honours:<exam>` and the giver's standing bonus for honours; failing tells the retry fee. Retry after a failure costs half; passed is passed (the exam closes to holders through the template's `available` condition).
Glue: the seat refuses without a licence that allows piloting (prompt "(needs the flight licence: exam at Skyhook Flight School)", a notice on the tap), open while the player's exam is active (the exam lends the ship); riding along and carrying never need one. `exo_app::flight_events::FlightWatch` (plain state machine with unit tests) turns the ship's state into the three events: take-off when the ship leaves the ground under the pilot, pad reached once per visit within the pad's radius and 120 m above it, landing with the largest speed towards the ground of the last 10 steps. Scenarios other than `licence` start with the licence earned. Content: flight school place `skyhook_school` (about 70 m from the start, opposite Drip Rock), giver `flight_school`, exam template `flight_exam` (take off, reach and land on the Drip Rock pad, set the school's parcel down; fee 150, retry 75, crash above 6 m/s, honours at 2.5 m/s within 150 s, deadline 300 s), licence `flight`, track `licence_flight`; `first_haul` (Bent Spoon) now needs the licence; texts (exam notices, refusal).

Checks: 10 tests in `jobs_core/tests/exam.rs` (loader errors, fee and per-player retry price, pass, honours, a slow plain pass, order and sender of the checks, crash, damaged crate, clock out, cannot afford, closed to holders, save with attempts), 5 unit tests of `FlightWatch`, kernel test for the events. Scenario `licence` (in the gate): a new player has no licence, the seat refuses with the pointer and a notice, riding along works, exam taken at the counter (fee 150 paid, the parcel waits on the school pad, three checks), the seat is open during the exam, the three events by test hook, parcel set down on Drip Rock, exam completed with honours, licence 1, standing +10, exam closed, the seat prompt plain. Gate: `cargo t` 440 passed, `cargo scenario` 0 failures.

TODO(initiator): fee 150, retry 75, crash limit 6 m/s, honours limits (2.5 m/s, 150 s), deadline 300 s, honours standing 10, overflight height 120 m for pad reached, the school's place and all its texts.
Open: the take-off, pad and landing in the real game are detected by `FlightWatch` but only the state machine is tested, the scenario sets the events by hook as the brief allows (no real hop under the flight model that is being reworked); a new game starts with 100 credits, so the fee needs three or four courier jobs first as decided.

## #166 map

Built: `planet_core::look`: `Planet::local_map` (a north-up RGB picture of the ground within a radius of a point: water by depth, land in biome colour shaded by height and slope), `local_map_dir` and `local_offset` (pixel to direction and back, in metres on the surface). A whole-planet equirectangular map is useless here because the start lies at the pole, so the map is local around the player. `jobs_core::map` (no Bevy): `pins` (places the crew may use on this planet, the tracked job's target at its next stop, the players, in that order), `next_stop` (pickup while crates wait, dropoff when all are carried, back to the pickup when one is set down elsewhere). Glue: taps `map` (M) and `track_job` (T, in `bindings.json`), `Gameplay::tracked` (kept on an active job, T cycles), the job line's pointer follows the tracked job's next stop, `exo_app::map`: M opens a 640 px overlay with the picture (made on a worker thread, about 0.9 s on this machine) and pins (yellow places, red pickup or green dropoff target, blue you), labels placed left or right so they do not overlap; it takes no input.

Checks: 3 tests in `planet_core/tests/look.rs` (size, north up and east right, metres, offset inverse, water blue), 6 in `jobs_core/tests/map.rs` (open and foreign places, the pin moving from pickup to dropoff, back to the pickup, offered or foreign targets, players, order), 1 unit test, scenario `map` (in the gate): 7 places pinned, all but far Bent Spoon on the picture, the tracked pin at the pickup then the dropoff, the job line agrees, M opens and closes, the picture is made. Windowed under `xvfb-run` the scenario shot the map: `night-shots/map.png`.

TODO(initiator): map radius (1000 m), picture size, colours, keys M and T.
Open: pin labels are plain text; no fog of war or scanning; the map is only for the planet the player is on.

## Day session (2026-10-10, with the initiator)

The rest of the brief was finished in a day session: the initiator planned it with the coordinator (Opus) and the coordinator split it into lanes for Haiku subagents. The container's 6 GB allow one Bevy build at a time, so only lanes in the serde-only `*_core` crates ran side by side, each in its own worktree and target dir (`lane/d-board`, `lane/d-production`, `lane/d-texts`). The coordinator reviewed each lane and sent back or fixed what fell short. The Haiku lane for #135 committed without its gate and left four Bevy builds running at once; its commit was undone and #135 was done by the coordinator, as the initiator decided. Glue lanes (#136, #132) go to the coordinator as well.

## #135 save file

Built: `exo_app::savefile`: the #128 envelope as JSON in `saves/autosave.json` in the game's own folder (scripted and headless runs: `<out_dir>/saves`, and they start new), written through a temporary file and a rename. Sections: kernel, jobs, customers, `game` (the seed) and world (the crates of active jobs with condition and place, planet frame or ship frame; the next crate id). A load is a restart: the goods crates go, the saved state replaces progress, jobs and customers (the host's event numbers continue above the saved ones, `Dedup::next_seq`), the crates come back where they were with their condition and pad. The save is loaded once at start, when the pads are known. Autosave on the game clock: 2 s after a job event (a burst writes once), else every 60 s. `saves/client_id` is made once per game folder (`LocalClient`); events still go out as `HOST` until the network sends the id (#134). `/saves/` is in `.gitignore`.

Checks: 3 unit tests in `savefile.rs` (client id made once and kept, slot written whole and read back, autosave pacing). New scenario `savefile` (and `scenario_savefile.rs` in the gate): take the first haul, deliver one of three crates, wear another down to 0.8, autosave seen, save to the file, restart from a fresh gameplay state with no crates, load the file: wallet, tracks, flags, unlocks (the whole progress), the active job and the crates with their condition and pad are as saved; the two crates left are carried and the job pays 290 of 300 (the worn crate). Gate: `cargo t` 455 passed (1 skipped: perf), `cargo scenario` 0 failures.

TODO(initiator): slot name and slots, the autosave times (2 s, 60 s), where the save lives for an installed game, a new game's seed (fixed now, so scenarios repeat).
Open: the ship's pose is not saved (the ship spawns at the start, a crate in its cabin comes back into the cabin, not mag-locked); crates on another planet than the current one are left out with a log line; no load menu; the join and send part is #134.

## #126 board generator (`lane/d-board`, Haiku, reviewed and fixed)

Built: `jobs_core::board` and `Jobs::generate_board_at` / `tick_board`: per location 3 to 5 offers from the templates whose condition holds and whose giver's counter is there (templates without a giver at any board), no template twice, places by tag search (pickup never the dropoff, locked places never), commodity and amount from the template's pool and range, seeded; unaccepted offers rotate after 600 s with a seed derived from the old one, accepted ones stay; exam templates and `customer_order` are never board offers; a once-only template is gone after its job is done; a follow-up waits for its job and then comes first on every board. The board state is part of the jobs save section.
Review: the first Haiku pass only aged offers, ignored the location and had a test that asserted nothing; the second pass reported follow-ups as done while nothing read the field and the test returned early. The coordinator implemented follow-ups and made the once-only test check both sides.
Checks: 13 tests in `jobs_core/tests/board.rs`.
TODO(initiator): offer lifetime 600 s, 3 to 5 offers, the rule for templates without a giver.
Open: not wired into the game, by the decision "Where jobs come from (D)": the counters still offer one fixed offer per template.

## #131 text keys (`lane/d-texts`, Haiku, reviewed and fixed)

Built: the loaders' text checks cover templates, givers, customers, licences and the keys the glue uses (`jobs_core::texts::GLUE_KEYS`, 16 keys); a test loads the shipped content and `en.json` and asserts no missing key and at least 3 lines in every pool a player sees often; 14 pools filled to 3 or 4 lines. The coordinator completed the glue key list (Haiku had 5 of 15) and replaced a refusal line that said one job at a time (the limit is two).
Checks: 2 tests in `customers_core/tests/texts.rs`.
TODO(initiator): every text (placeholders). Open: the loop UI does not yet show text only through keys (pointer words, panel hints and the HUD still have English in code).

## `production_core` sketch (`lane/d-production`, Haiku, reviewed and fixed)

Built: new crate `production_core` (no Bevy): record `recipe` (station kind, inputs, outputs, time) with loader errors naming file and field, `Station` state machine (idle, loading, running, done, blocked; insert, step, take outputs; wrong input refused), section `production` in the save. Two placeholder recipes in `content/gameplay/recipe/`. The coordinator fixed the shipped recipes (they named a good that does not exist; only fixtures were tested) and added a test that loads them against the shipped goods.
Checks: 16 tests in `production_core/tests/production.rs`.
TODO(initiator): every recipe, station kind and time. Open: no glue, no events yet; G decides how stations are placed and fed.

## #136 HUD target arrow

Built: `Gameplay::arrow` (target, pickup or dropoff, bearing from the look direction, distance), filled every step from the tracked job's next stop (`jobs_core::map::next_stop`, tested in #166); `gameplay::bearing` (also used by the job line's pointer words). Window: an arrow at the top centre, turned towards the target, red to a pickup and green to a dropoff as the map's pins, the distance under it; hidden without a tracked job. Money and freight XP were already on the job line.
Checks: 1 unit test (bearing: left positive, ahead 0, behind 180, height ignored); scenario `courier` checks the arrow at the pickup while parcels wait, at the dropoff after the pickup (distance within 5 m of the walk), and the job line's money after the payout with no arrow left. The window drawing is not run headless (no screenshot this time). Gate: `cargo t` 487 passed (1 skipped: perf), `cargo scenario` 0 failures.
TODO(initiator): look, size, place and colours of the arrow.

## #132 abandon (reduced)

Decided with the initiator in the day session: no board menu (the counters stay); only the missing abandon. Built: tap `abandon_job` (Y, in `bindings.json`, old binding files still load): the first press names the tracked job and asks ("press again"), a second press within 3 s drops it (`JobAbandoned`: crates become plain, nothing is charged, the giver offers it again). New pool `notice.job.abandon_ask`.
Checks: scenario `courier` starts with it: take the Lint Trap job, one press asks and the job stays, the second drops it, its parcel is a plain crate, the job is on offer again.
TODO(initiator): the key, the confirm time 3 s, the texts.

