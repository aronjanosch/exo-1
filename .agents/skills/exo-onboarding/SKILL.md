---
name: exo-onboarding
description: Walk a new contributor through setting up EXO-1, step by step, explaining each step. Type it by hand on your first day.
disable-model-invocation: true
---

# exo-onboarding

You guide a new contributor from a fresh clone to a first working PR. Talk in the contributor's language. Explain each step in one or two sentences (what and why), then do it. **Ask before every command that installs something or writes outside this repo.** The contributor may be on Windows, Linux or macOS, and may use any coding agent.

Each step is done when its check passes. Say the result, then go on.

## Steps

1. **Orient.** Read `AGENTS.md` and tell the contributor in five lines what the project is and what the roles are: the initiator decides design, the agent is a sparring partner first and implements in small slices. Done when the contributor says they understood, or asks questions.
2. **Machine.** Detect the OS. Check `git --version`, `rustc --version`, `cargo --version`, `node --version`. Name what is missing and offer to install it (`mise` if present, else the platform's usual way). Done when all four print a version.
3. **Windows only.** Check `git config core.symlinks`. If it is not `true`, explain that Developer Mode plus `git config core.symlinks true` is needed for symlinks and that `.agents/skills/` works without them. Done when the contributor chose one.
4. **Local notes.** If `WORKSPACE.md` is missing, offer to create it from questions about their machine (shared `CARGO_TARGET_DIR`, display setup). It is gitignored. Done when the file exists or the contributor declined.
5. **Concept repo.** Look for `../exo-1-concept`. If missing, ask for access and clone it as a sibling folder. Read `docs/VISION.md` there together, then point at `docs/DECISIONS.md` (the initiator's words) and `docs/LEARNINGS.md` (read before running the game). Done when `docs/VISION.md` is readable.
6. **Skills.** Read [`SKILLS.md`](SKILLS.md) and install what it lists, for the contributor's agent. Done when every listed skill shows up in the agent's skill list.
7. **First run.** If `Cargo.toml` exists, run `cargo test --workspace` and the headless full scenario from `README.md`, section "Run". Explain that a change is done when both pass. If there is no `Cargo.toml` yet, say that the code arrives with the next milestone and skip. Done when both exit zero, or the skip is stated.
8. **First PR.** Explain the flow: fork (or branch, if the contributor has write access), one branch per feature, small slices, PR against `main`. `main` takes changes only through PRs. Point at the project skills `scope-gate`, `implement-slice`, `exo-review` in `../exo-1-concept/.agents/skills/` for when work starts. Done when the contributor can say what their first branch will be called.

Finish with a short summary: what is installed, what was skipped, what comes next.
