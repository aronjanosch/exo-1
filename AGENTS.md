# AGENTS.md — EXO-1

Project rules for agents, single source for this repo and the concept repo. Read `docs/VISION.md` first (currently in the concept repo); settled choices live in `docs/DECISIONS.md` there, with the initiator's words.

## What this project is

A co-op crew game in Rust with Bevy: friends run one walkable ship together, each in a role, through a strange galaxy with real flight and real planets. Built by the initiator and maybe a few friends. AI makes implementation cheap, so the scarce things are ideas and taste. You are here to amplify the initiator's thinking, not to replace it.

## Your role

- Sparring partner first, implementer second.
- The initiator decides design, balancing and system design. When a design gap appears, ask, or list the assumption and get a yes.
- Build in small slices, one branch per feature.
- Other games are source material. Read their code and data, port or reimplement their mechanics, talents and values, mix them with our own ideas, then tune by playtest. The result is a new work. Add a short line for everything taken over to `docs/BORROWED.md` in the concept repo.

**Spikes**: throwaway code on a `spike/<name>` branch, driven by a spike brief, never merged into `main`. Findings go into the docs; the code stays on the branch and a `spike/<n>-<name>` tag.

**Git state** (tags, merges, deleting branches): read the concept repo's `docs/LEARNINGS.md`, section "Git and spike states", before you change it.

## Architecture

- One Cargo workspace at the repo root, every crate in `crates/`, content in `content/` (the Bevy asset root).
- Simulation lives in `*_core` crates without Bevy types, in `f64`, tested with plain `cargo test`. Physics-facing logic talks to the world through a small trait (pattern: `walker_core::World` with `sweep` and `depenetrate`).
- The Bevy crate is glue: rendering, input, camera, HUD, physics bodies, scenarios.
- Physics runs in `f64` world space (Avian with `f64`); only the render origin shifts.
- Models are Blender Python scripts in `art/`, exported into `content/`; model work follows `art/AGENTS.md` (start the session in `art/` for the live Blender MCP).

## Agent loop

- Tests run in three sizes; the initiator wants the long run only when a lot comes together:
  - **A feature** is done when its own tests pass: `cargo test -p <crate>` for every crate it touches, and its scenario, where it has one, as `cargo ts scenario_<name>` (dynamic; `cargo test -p exo_app` builds static Bevy, 900 MB per binary).
  - **Merging a few features** (into a round branch or `main`) reruns the tests of those features on the merged branch, no full run.
  - **A big round** (a whole milestone or most of it) runs the full gate before it reaches `main`: `scripts/gate` (Linux and macOS; on Windows `cargo t` directly) runs `cargo t` (cargo-nextest, `mise install`, every workspace test and every headless scenario, the whole CPU), one at a time on the machine. Seven 100 km planet tests are `#[ignore]` in that run (about 300 CPU seconds); before `main`, add `--run-ignored all`: `scripts/gate --run-ignored all`. With it go the timing test `cargo t -P perf` on a quiet machine and `cargo check --workspace --all-targets` for the static build (GitHub CI is off, runner budget).
  Commands and scenario names: `README.md`, section "Run".
- The app's integration tests are one binary (`crates/exo_app/tests/app/`): a new scenario test is a module there. Each extra file in `tests/` links Bevy again.
- Several sessions at once (a lane each) or a playtest round across them: skill `exo-orchestrate`.
- Test what breaks the game: a crash, a desync, a NaN, lost data (save, items), a broken rule (authority, reach, state machine), physics that blows up. A scripted scenario that drives the game through the `Controls` resource covers what crosses crates, without a window. Feel, tuning values, layout and config get no test: the initiator judges them in play, and a test that goes red on a change that plays better is deleted.
- Bevy and Avian change their API in every release. Look up names and signatures in `~/.cargo/registry/src/*/` (`bevy*-<version>/`, `avian3d-<version>/`) and the Bevy examples there before you write code; trust the source over memory of older versions.

## Ask first

New dependencies, CI, `.cargo/`, `build.rs`, `unsafe`, networking, spawning processes, file access outside the game's own directories.

## Style

- Simple look, best runtime performance. Assets we ship are original or CC0 (fonts may also be SIL OFL, with their `OFL.txt` beside them); foreign test material is listed in the concept repo's `docs/BORROWED.md`.
- Comments give the reason a reader cannot see in the code (why, units, source). History lives in git and issues.
- Invented names in content, tests and issues; no personal data.
- This repo is public: issues, PRs and commits describe mechanics in our own words and point to the private concept repo for material from other games (configs, number tables, screenshots).
- Goofy tone is a feature. The setting is a strange galaxy.

## Skills

Project skills live in `.agents/skills/`. New contributors start with `exo-onboarding`, which also lists the external skills to install (`.agents/skills/exo-onboarding/SKILLS.md`). Before editing a skill, `AGENTS.md` or `CLAUDE.md`, load `writing-for-agents`.

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues; use `gh`. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context documentation rules. See `docs/agents/domain.md`.

## Local environment

If a `WORKSPACE.md` exists in the repo root, read it before running anything. Each person writes their own to describe their machine, for example a shared `CARGO_TARGET_DIR`. It is gitignored. Follow it over the defaults here.

## When unsure

Say what you do not know, propose the smallest next step, ask one question at a time.
