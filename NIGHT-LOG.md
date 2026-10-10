# NIGHT-LOG: milestone D night run, 2026-10-10

Morning report of the unattended run (brief: concept repo `docs/RUN-D-NIGHT-BRIEF.md`). Phase 1 on `night/d-foundation`, phase 2 (extras) on `night/d-extras`. Nothing merged, no PR, no issue closed. The old C log is in git history.

## Checklist

Phase 1, `night/d-foundation`:

- [x] #128 save model (kernel envelope, kernel/jobs/world sections)
- [x] #165 feedback beats
- [x] #167 givers
- [ ] #170 courier jobs
- [ ] #168 customers
- [ ] #169 flight licence
- [ ] #166 map
- [ ] #135 save file

Phase 2, `night/d-extras`: not started.

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
