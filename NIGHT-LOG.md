# NIGHT-LOG: round 6, the smuggling run, night of 2026-10-11

Morning report of the unattended run. The plan was agreed with the initiator in the chat: base `round/planets-100km`, backup off, GitHub allowed (epic #225, lane issues #226–#230, playtest issue and PR at the end, the PR not merged). Branch `round/6`. The design proposal is in the concept repo, `docs/SMUGGLE-RUN-PROPOSAL.md`. Every name and number is `TODO(initiator)`. The old D log is in git history.

## Checklist

Kernel and world:
- [x] Kernel contract (4e02e53): round-6 events, `TownId`, `EffectId`, `ProductSpec`, the `effect` record with 10 placeholder effects
- [x] G1 town: the city plan as data, building colliders, pad models, characters, new towns and pads, `--start <place>`

Core lanes (Haiku, reviewed by the orchestrator):
- [x] #226 traffic_core
- [x] #227 heat_core
- [x] #228 lab (production_core)
- [x] #229 buyers (customers_core)
- [x] #230 empire (gameplay_core goals, shops, tallies)

Glue (orchestrator):
- [x] G2 phone and goal line
- [x] G3 hail and landing
- [x] G4 empire and shop
- [x] G5 lab
- [x] G6 heat and scan
- [x] G7 dealing
- [x] G8 the whole loop (`smuggle_run`)

End:
- [x] merge of round/planets-100km, gate, static check
- [x] playtest issue, PR, cleanup

## Log

- 01:45 Phase 0: worktree `../exo-1-r6`, kernel contract pushed, epic and lane issues, five Haiku lanes and two research agents started. Research notes went into the concept repo as `docs/research/smuggle-run-*.md`.
- 02:00 G1 started. The city plan is exported to `content/city/plan.json` by `art/city/export_plan.py`, plain Python with box bounds read from the glbs. New places: `city` (beside Drip Rock), `noodle_town`, `gulch_freeport`, plus two unofficial spots. New pads at Drip Rock and Bent Spoon. Decision for the night: buildings stand on the ground under their centre, with no flattening of the city (no edits in `planet.rs`, which belongs to the planet lanes). TODO(initiator).
- 02:15 All five Haiku lanes are done and merged into round/6 after review. Findings:
  - buyers: a withdrawn order raised `ProductOrderSettled` with 0 paid, which heat would count as a deal and the family's tally as a deal done. `CustomerPoached.by` named the rival instead of the buyer. Both fixed, tests adjusted.
  - traffic: its tests patch the shipped town files by exact strings. Moving the towns to the round-6 pads broke 14 tests; the test strings now follow the new files.
  - empire: the wholesaler's placeholder goods are swapped for the lab's six ingredients plus both bases, the bases behind `lab_access`.
  - Core crates on round/6: 289 tests pass (gameplay 86, jobs 73, customers 39, production 34, heat 38, traffic 19).
- 02:30 Glue written: `smuggling/{systems,glue,hud}.rs`, routed from `gameplay_step`, plus the seven round-6 scenarios in `scenario/smuggle.rs`. The first exo_app build on this machine takes about 40 min (heavy host load), so it compiles now.
- 03:05 All eight round-6 scenarios pass (town, phone, hail, empire, lab, scan, dealing, smuggle_run). Fixes along the way:
  - A key tapped several times in one step counts once: the phone scenarios move one row per step (`select_row`).
  - A crate in reach beats "start the brew", so a loaded kettle doesn't swallow the next ingredient's F.
  - Heat decays while a scenario waits, so `heat_to` aims a few points above the star.
  - At 4 stars a scan may search the hidden hold. That's correct, so the scan scenario hails properly to stay at 3.
- 03:20 Merged round/planets-100km (relief, settlements #223, geomorphing, sky). The new relief put the city on a 44° slope. `content/place/city_ground.json` flattens one rectangle under the spaceport and the city; pads inside lose their own flattens.
- 03:30 First gate on round/6: 12 failures. A baseline run of the same tests on round/planets-100km (detached checkout in the same worktree) fails 7 of them already upstream: collision_t1, crate_carry, crate_unload, crate_budget, first_person_settings, warp, full. Round 6 added courier, map, 3 unit tests (KEYS.md rows, the unbound-key test now that Z counters, a wet pad).
- 03:55 Fixed: every pad is dry and flat at 5 km and 100 km (ignored test `probe_town_spots` checks both and searches dry offsets). The free port moved to 6°N 44°E, Noodle Scrub and Noodle Post moved. The level plot now holds Lint Trap and Noodle Post, because walking off its edge with a parcel wrecked it. exo_app lib 89/89, the 14 gameplay scenarios green.
- 04:08 The container restarted (external, same time as the nights before). The run continued in the morning.
- Morning: stretch goal air patrol (#231, `heat_core::patrol` plus the `patrol` scenario): from 2 stars a patrol may hail you in a town's airspace. Slow below 8 m/s within 12 s for an air scan, or flee for heat and a violation. Screenshots of a window run are on the orphan branch `shots/round-6`. Proposal updated in the concept repo. Lane worktrees and targets removed.
- Final gate on 79aa6f5 (round/planets-100km merged, nothing new upstream since): `scripts/gate --no-fail-fast --test-threads 3` ran 786 tests: 786 passed, 8 skipped. The 7 baseline failures from 03:30 passed this time too, so they look load-dependent (the 03:30 run shared the CPU with other jobs). `cargo check --workspace --all-targets` OK.
- 11:15 Merged round/planets-100km again (#232 HUD altitude, #238 dev_8): conflict in `Tap::ALL` (35 upstream + 11 round-6 = 46). Gate on ed37469: 785 passed, 1 failed (`keys_md_lists_every_action`: #238 added `dev_8` without a KEYS.md row, so it fails upstream too). Row added; the controls tests pass 21/21. `cargo check --workspace --all-targets` OK.
- 11:20 Round 6b started (the initiator's choice: research and a feedback and HUD layer on round/6 now, fun and depth as a proposal after the playtest #233). Three Haiku research agents: feedback inventory of the code, game-feel references, the local extracts.
- 12:10 Round 6b research done (scratchpad notes, synthesis in the concept repo `docs/research/smuggle-run-feedback.md`). Diagnosis: 43 actions, almost all answer with a text line only. The payday notice had no amount, and fines, bribes and confiscations were silent.
- 12:40 Slice 1 (17d3f37): `feedback`, headless cues per world event, the payday count-up banner, refusals said, toasts for orders, referrals and poaching. 5 unit tests; `smuggle_run` checks 9 felt beats. `deliver` failed once in a parallel run (a crate missed the pad under load) and passes alone.
- 13:30 Slice 2 (aa7f300): `feedback_view` and `ui_theme`, the HUD rewritten in one look. Screenshot review found the middle-dot glyph missing (ASCII now), the stamp queue flooding on repeated violations (dedup, at most 3), a vignette that was too strong, and the patrol craft off-frame. All fixed and checked on new pictures.
- 14:00 Slices 3 and 4 (626d3be, 431add8, 032e6b4): the stopped needle holds in its tier colour, rank-up names the rank, airspace stamps, a live speed warning in the tower's line (hail scenario checks it), credits count towards a new total, themed banner and toasts. 12 screenshots, 7 new ones on `shots/round-6` under `round-6b/`; playtest issue #233 has section 8 and the pictures.
- 14:45 Merged round/planets-100km (#236). Gate on 534ded8: 793 passed, 0 failed, 8 skipped; `cargo check --workspace --all-targets` OK. `deliver` failed twice under load (dropped crates settle slowly), so it now waits for the job instead of a fixed 4 s. Phase C candidates are in the proposal in the concept repo, for after the playtest.
- 15:10 Final review on 054a894. The five core crates' diffs since the base: no `||` in asserts. Three `return;`, all in game code (`traffic_core/src/state.rs:233` hold answer, `customers_core/src/state.rs:431/440` referral guards), none in tests. Nine `return;` in `production_core/tests/production.rs` come from the base (not round 6): "skip if no recipes" guards, never taken because the fixtures hold two recipes. Result: nothing to fix. The proposal's TODO list was regenerated at this head: 57 of 57 code lines listed by path:line, and 75 of 75 content files listed under their folder.
