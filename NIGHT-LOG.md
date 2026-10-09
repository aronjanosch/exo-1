# NIGHT-LOG: milestone D night run, 2026-10-10

Morning report of the unattended run (brief: concept repo `docs/RUN-D-NIGHT-BRIEF.md`). Phase 1 on `night/d-foundation`, phase 2 (extras) on `night/d-extras`. Nothing merged, no PR, no issue closed. The old C log is in git history.

## Checklist

Phase 1, `night/d-foundation`:

- [x] #128 save model (kernel envelope, kernel/jobs/world sections)
- [x] #165 feedback beats
- [ ] #167 givers
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
