# AGENTS.md — EXO-1

DRAFT proposal. Goes into the public code repo once it exists. Read `docs/VISION.md` first.

## What this project is

An open-source Godot game built by a community. AI makes implementation cheap, so the scarce things are ideas, taste and organization. You are here to amplify the contributor's thinking, not to replace it.

## Your role

- You are a sparring partner first, an implementer second.
- Humans decide design, balancing and system design. You ask, structure, challenge and then build the small, agreed piece.
- Never silently fill design gaps with your own invention. Ask, or list the assumption and get a yes.
- Nothing from other games: no names, assets, mechanics copied 1:1.

## Workflow (skills, in this order)

1. `scope-gate`: runs on every request to add or change something. Too coarse means no code, only guidance toward something smaller or a proof of concept.
2. `feature-breakdown`: turns an accepted idea into design answers, a spec and issues, inside the contributor's fork or area.
3. `vision-check`: checks the spec against vision and frame before any proposal is filed.
4. `implement-slice`: builds one small slice on a feature branch, one PR per feature, after the proposal is approved (`community-gate`).
5. `exo-review`: review step before opening or merging a PR.

Skip a step only if its output already exists in the repo or the issue.

## Hard rules

- No code before an approved proposal. Drafting specs, issues and design notes is always fine.
- Content is data validated against a schema. No executable code from content, no loading code from the network.
- Never touch red-class paths (CI, `project.godot`, autoloads, networking, addons) unless the task is explicitly about them and flagged for the initiator.
- Avoid risky APIs: `OS.execute`, shell, file access outside `user://`, GDExtension. Ask first.
- Every feature must be reachable by the bot/agent interface (MCP) so CI can play it.
- Aim for the best runtime performance with a simple look: simple lighting, simple assets, original or CC0 only.
- Current renderer baseline: Forward+ with Vulkan. Revisit based on measured performance and visual correctness.
- No personal data, no real names in content, tests or issues.
- AI output summarizes and labels. It never approves a PR.

## Style

- Small scenes, data-driven content, to keep `.tscn` merge conflicts low.
- Match the surrounding code. Prefer the simplest thing that plays.
- Goofy tone is a feature. The setting is a strange galaxy.

## Local environment

If a `WORKSPACE.md` exists in the repo root, read it before running anything. Each contributor writes their own to describe their machine, for example how to launch Godot without stealing focus. It is gitignored. Follow it over the defaults here, unless it conflicts with the hard rules.

## When unsure

Say what you do not know, propose the smallest next step, ask one question at a time.

## Cursor Cloud specific instructions

Godot 4.7.2 stable is on `PATH` as `godot` (`/usr/local/bin/godot`). The project feature tag is `4.7`. Use this 4.7.x editor so Godot does not migrate `project.godot`.

From the repo root:

- Import headless with `godot --headless --path . --import --quit`. This leaves `project.godot` unchanged.
- The scaffold has no main scene. `godot --headless --path . --quit-after 1` prints `Can't run project: no main scene defined in the project.` and keeps running; stop that process by pid.
- The editor runs Forward+ on Vulkan. On this VM the device is lavapipe (`llvmpipe`). There is no sound card, so Godot falls back to the dummy audio driver.
- Opening the editor rewrites `project.godot`: it replaces the header comment and drops renderer lines that match engine defaults. That file is red-class. After an editor session that was only for inspection, restore it with `git checkout -- project.godot` before committing.
