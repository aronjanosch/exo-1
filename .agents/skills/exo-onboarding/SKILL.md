---
name: exo-onboarding
description: Walk a new contributor through setting up EXO-1, step by step, explaining each step. Type it by hand on your first day.
disable-model-invocation: true
---

# exo-onboarding

You guide a new contributor from a fresh clone to a first working PR. Talk in the contributor's language. Explain each step in one or two sentences (what and why), then do it. **Ask before every command that installs something or writes outside this repo.** This includes Cargo downloading crates, installing skills, and cloning the sibling concept repo. The contributor may be on Windows, Linux or macOS, and may use any coding agent.

In OpenCode, invoke this user-invoked skill with `/exo-onboarding` (or select it from slash-command suggestions); do not assume a generic `/skill <name>` command exists. In other agents, use that agent's native skill invocation. If slash invocation is unavailable, the contributor can ask the agent to load `exo-onboarding` by name.

Each step is done when its check passes. Say the result, then go on. Ask one question at a time and wait where a step requires the contributor's understanding, choice, or permission. If the contributor asks something or steers to a side task, handle it, then resume at the unfinished onboarding step. Do not report onboarding complete until every step's completion criterion is met.

## Steps

1. **Orient.** Read `AGENTS.md` and tell the contributor in five lines what the project is and what the roles are: the initiator decides design, the agent is a sparring partner first and implements in small slices. Pause for their confirmation or questions before continuing.
2. **Machine.** Detect the OS. Check `git --version`, `rustc --version`, `cargo --version`, `node --version`. Name what is missing and offer to install it (`mise` if present, else the platform's usual way). Ask before the install command. Done when all four print a version.
3. **Windows only.** On Linux and macOS, say that this step does not apply and go on. On Windows, check `git config --get core.symlinks`. If it is not `true`, explain that Git symlinks require Windows Developer Mode and `git config core.symlinks true`; `.agents/skills/` works without symlinks. Ask whether to enable it or leave it off; only set it after the contributor chooses enable. Done when the setting is true or the contributor chooses to leave it off.
4. **Local notes.** Read `WORKSPACE.md` if it exists. If it is missing, explain that it is a gitignored, local note and ask whether to create it. Ask separately for a shared build-cache path and display setup. Record where the concept repo lives (see step 5) as a line in `WORKSPACE.md`. Be precise: `WORKSPACE.md` documents `CARGO_TARGET_DIR`; it does not set the environment variable. Cargo already uses the project's `target/` directory by default. If the contributor wants that default, document it and create `target/` only if requested; if they want a custom path, ask before adding any Cargo config. Done when the note exists or they declined.
5. **Concept repo.** Check for `../exo-1-concept` relative to this repo. If it exists, `git pull` it so it is current. If missing, ask for access and permission before cloning it as a sibling (outside this repo). Either way, make sure `WORKSPACE.md` names its path. Read `docs/VISION.md` there and summarize the vision together; point to `docs/DECISIONS.md` for explicit decisions and `docs/LEARNINGS.md`, which must be read before running the game. Done when `docs/VISION.md` is readable, the repo is current and `WORKSPACE.md` names its path.
6. **Skills.** Read [`SKILLS.md`](SKILLS.md), identify the contributor's agent, and install the listed skills as described there. Ask before running the installer. The agent-skill configuration (`docs/agents/`) is already committed; do not run `setup-matt-pocock-skills`. If the contributor already has some of the skills installed globally, do not link duplicates. Done when every listed skill is available to the agent.
7. **First run.** Re-check for root `Cargo.toml` at this point. If it exists, read `README.md`, section "Run". Export the `CARGO_TARGET_DIR` named in `WORKSPACE.md`, if any. Ask before the first build if Cargo may need to download crates (the first build takes minutes); then run `mise install` (it brings cargo-nextest) and the gate command. Explain that a change is done when it passes. If no root `Cargo.toml` exists, say the code arrives with the next milestone and skip. Done when the gate exits zero or the skip is explicitly stated.
8. **First PR.** Explain the flow: fork (or branch, if the contributor has write access), one branch per feature, small slices, PR against `main`; `main` takes changes only through PRs. Point at `scope-gate`, `implement-slice`, and `exo-review` in `../exo-1-concept/.agents/skills/` for when work starts. Ask for the name of their first branch and wait for an answer; if they defer or steer to another task, leave onboarding incomplete and return to this step later.

Only after all completion criteria are met, finish with a short summary: what is installed, what was skipped, the first branch name, and what comes next.
