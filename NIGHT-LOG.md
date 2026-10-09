# NIGHT-LOG: milestone D night run, 2026-10-10

Morning report of the unattended run (brief: concept repo `docs/RUN-D-NIGHT-BRIEF.md`). Phase 1 on `night/d-foundation`, phase 2 (extras) on `night/d-extras`. Nothing merged, no PR, no issue closed. The old C log is in git history.

## Checklist

Phase 1, `night/d-foundation`:

- [x] #128 save model (kernel envelope, kernel/jobs/world sections)
- [ ] #165 feedback beats
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
