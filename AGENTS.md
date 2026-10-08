# AGENTS.md — EXO-1

Project rules for agents, single source for this repo and the concept repo. Read `docs/VISION.md` first (currently in the concept repo); settled choices live in `docs/DECISIONS.md` there, with the initiator's words.

## What this project is

A game in Rust with Bevy, built by the initiator and maybe a few friends. AI makes implementation cheap, so the scarce things are ideas and taste. You are here to amplify the initiator's thinking, not to replace it.

## Your role

- Sparring partner first, implementer second.
- The initiator decides design, balancing and system design. When a design gap appears, ask, or list the assumption and get a yes.
- Build in small slices, one branch per feature.
- Other games are inspiration: learn from their mechanics and write-ups, then build our own implementation with our own code, assets, data, names and texts.

**Spikes**: throwaway code on a `spike/<name>` branch, driven by a spike brief, never merged into `main`. Findings go into the docs; the code stays on the branch and a `spike/<n>-<name>` tag.

## Architecture

- One Cargo workspace at the repo root, every crate in `crates/`, content in `content/` (the Bevy asset root).
- Simulation lives in `*_core` crates without Bevy types, in `f64`, tested with plain `cargo test`. Physics-facing logic talks to the world through a small trait (pattern: `walker_core::World` with `sweep` and `depenetrate`).
- The Bevy crate is glue: rendering, input, camera, HUD, physics bodies, scenarios.
- Physics runs in `f64` world space (Avian with `f64`); only the render origin shifts.

## Agent loop

- A change is done when `cargo t` (workspace tests) and `cargo scenario` (headless full scenario) both pass; the scenario exits non-zero on any failed check. Both link Bevy dynamically for fast rebuilds; CI on main runs the static build. Commands and scenario names: `README.md`, section "Run".
- Every feature gets a scripted scenario that drives it through the `Controls` resource, so it can be tested without a window.
- Bevy and Avian change their API in every release. Look up names and signatures in `~/.cargo/registry/src/*/` (`bevy*-<version>/`, `avian3d-<version>/`) and the Bevy examples there before you write code; trust the source over memory of older versions.

## Ask first

New dependencies, CI, `.cargo/`, `build.rs`, `unsafe`, networking, spawning processes, file access outside the game's own directories.

## Style

- Simple look, best runtime performance. Assets original or CC0.
- Invented names in content, tests and issues; no personal data.
- Goofy tone is a feature. The setting is a strange galaxy.
- For Blender modeling and visual refinement, follow `art/README.md` (live MCP workflow).

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
