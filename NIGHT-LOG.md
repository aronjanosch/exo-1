# NIGHT-LOG: round 6, the smuggling run, night of 2026-10-11

Morning report of the unattended run. The plan was agreed with the initiator in the chat: base `round/planets-100km`, backup off, GitHub allowed (epic #225, lane issues #226–#230, playtest issue and PR at the end, the PR not merged). Branch `round/6`. The design proposal is in the concept repo, `docs/SMUGGLE-RUN-PROPOSAL.md`. Every name and number is `TODO(initiator)`. The old D log is in git history.

## Checklist

Kernel and world:
- [x] Kernel contract (4e02e53): round-6 events, `TownId`, `EffectId`, `ProductSpec`, the `effect` record with 10 placeholder effects
- [ ] G1 town: the city plan as data, building colliders, pad models, characters, new towns and pads, `--start <place>`

Core lanes (Haiku, reviewed by the orchestrator):
- [ ] #226 traffic_core
- [ ] #227 heat_core
- [ ] #228 lab (production_core)
- [ ] #229 buyers (customers_core)
- [ ] #230 empire (gameplay_core goals, shops, tallies)

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
