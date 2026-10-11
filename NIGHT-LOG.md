# NIGHT-LOG: round 6, the smuggling run, night of 2026-10-11

Morning report of the unattended run. The plan was agreed with the initiator in the chat: base `round/planets-100km`, backup off, GitHub allowed (epic #225, lane issues #226–#230, playtest issue and PR at the end, the PR not merged). Branch `round/6`. The design proposal is in the concept repo, `docs/SMUGGLE-RUN-PROPOSAL.md`. Every name and number is `TODO(initiator)`. The old D log is in git history.

## Checklist

Kernel and world:
- [x] Kernel contract (4e02e53): round-6 events, `TownId`, `EffectId`, `ProductSpec`, the `effect` record with 10 placeholder effects
- [ ] G1 town: the city plan as data, building colliders, pad models, characters, new towns and pads, `--start <place>`

Core lanes (Haiku, reviewed by the orchestrator):
- [x] #226 traffic_core
- [x] #227 heat_core
- [x] #228 lab (production_core)
- [x] #229 buyers (customers_core)
- [x] #230 empire (gameplay_core goals, shops, tallies)

Glue (orchestrator):
- [ ] G2 phone and goal line
- [ ] G3 hail and landing
- [ ] G4 empire and shop
- [ ] G5 lab
- [ ] G6 heat and scan
- [ ] G7 dealing
- [ ] G8 the whole loop (`smuggle_run`)

End:
- [ ] merge of round/planets-100km, gate, static check
- [ ] playtest issue, PR, cleanup

## Log

- 01:45 Phase 0: worktree `../exo-1-r6`, kernel contract pushed, epic and lane issues, five Haiku lanes and two research agents started. Research notes went into the concept repo as `docs/research/smuggle-run-*.md`.
- 02:00 G1 started. The city plan is exported to `content/city/plan.json` by `art/city/export_plan.py`, plain Python with box bounds read from the glbs. New places: `city` (beside Drip Rock), `noodle_town`, `gulch_freeport`, plus two unofficial spots. New pads at Drip Rock and Bent Spoon. Decision for the night: buildings stand on the ground under their centre, with no flattening of the city (no edits in `planet.rs`, which belongs to the planet lanes). TODO(initiator).
- 02:15 All five Haiku lanes are done and merged into round/6 after review. Findings:
  - buyers: a withdrawn order raised `ProductOrderSettled` with 0 paid, which heat would count as a deal and the family's tally as a deal done. `CustomerPoached.by` named the rival instead of the buyer. Both fixed, tests adjusted.
  - traffic: its tests patch the shipped town files by exact strings. Moving the towns to the round-6 pads broke 14 tests; the test strings now follow the new files.
  - empire: the wholesaler's placeholder goods are swapped for the lab's six ingredients plus both bases, the bases behind `lab_access`.
  - Core crates on round/6: 289 tests pass (gameplay 86, jobs 73, customers 39, production 34, heat 38, traffic 19).
- 02:30 Glue written: `smuggling/{systems,glue,hud}.rs`, routed from `gameplay_step`, plus the seven round-6 scenarios in `scenario/smuggle.rs`. The first exo_app build on this machine takes about 40 min (heavy host load), so it compiles now.
