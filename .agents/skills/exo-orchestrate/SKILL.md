---
name: exo-orchestrate
description: Rules for running several agent sessions on EXO-1 at once. Use when starting or working in a parallel session (own worktree and branch), or when coordinating a round of sessions into one playtest and one PR.
---

# exo-orchestrate

Two leading words carry this skill:

- A **lane** is one parallel session: one issue, one branch, one worktree, one target dir, all named `<name>`.
- A **round** is one playtest cycle: lanes start from `main`, finish into the round branch `round/<n>`, the initiator plays the round branch, and it reaches `main` as one PR.

Machine budget (16 cores, 31 GB): at most three lanes that build at once, and one gate at a time on the machine (`scripts/gate` waits for a running one); research lanes without cargo are free. Every cargo command runs under `nice`. A NAS lane follows the same rules and needs its own `WORKSPACE.md` with that machine's paths.

## In a lane

1. **Set up** from the main checkout:
   ```sh
   git fetch && git worktree add -b <branch> ../exo-1-<name> origin/main
   ln -s ~/Work/exo-1/WORKSPACE.md ../exo-1-<name>/WORKSPACE.md
   cp -a --reflink=always ~/.cache/exo-1-target ~/.cache/exo-1-target-<name>
   ```
   Every cargo command in the lane runs with `CARGO_TARGET_DIR=~/.cache/exo-1-target-<name>` and `CARGO_BUILD_JOBS=4`. Done when `git branch --show-current` in the worktree prints `<branch>`.
2. **Work** test-first in the `*_core` crate where the logic lives. Subagents read, research and review; code goes to an `implementer` subagent (`.claude/agents/implementer.md`: one small ticket, crate tests only, never the gate), one at a time per worktree. Cargo runs one command at a time in the lane. Open design questions, and any step of a loaded skill that waits for the initiator (such as `tdd` confirming seams), become `TODO(initiator)` placeholders plus a line in the report; the lane keeps going. A choice the initiator should judge by feel gets an in-game toggle key.
3. **Check in tiers**: while working, `cargo test -p <crate>` and the lane's own scenario as `cargo ts scenario_<name>` (dynamic like the gate; `cargo test -p exo_app` builds static Bevy, 900 MB per binary); once at the end, `scripts/gate` (about 40 s on a quiet machine). Subagents never run the gate.
4. **Push after every commit** to the lane branch. Check `git branch --show-current` before each commit.
5. **Finish**: merge `origin/main` into the lane (merge, keep the commits), run `scripts/gate`, push. Done when it exits 0 on the pushed commit.
6. **Report** as one comment on the lane's issue: what was built, the check results with numbers, the `TODO(initiator)` list, anything surprising. The coordinator reads it there.
7. **Clean up** once the report is posted: the branch is on origin, so remove the lane's target dir (`rm -rf ~/.cache/exo-1-target-<name>`, 3 to 40 GB) and its worktree (`git worktree remove ../exo-1-<name>`). Say so in the report.

## Coordinating a round

1. **Plan** the lanes with the initiator. The coordinator runs on Opus (`/model opus`; `opusplan` turns it into Sonnet after the plan), subagents default to Haiku (`CLAUDE_CODE_SUBAGENT_MODEL`). One issue each, a base (`origin/main`), a prompt from [`PROMPT.md`](PROMPT.md). File moves and splits (one file into a folder, renames across crates) run alone or as the first lane of the round, before feature lanes branch.
2. **Open** `round/<n>` from `origin/main` and push it.
3. **Watch** with `git fetch` and `gh issue view <n> --comments`; a lane without a push for 30 minutes gets a nudge. `/loop 20m` fits this.
4. **Collect**: merge each finished lane into `round/<n>` in its own worktree with its own target dir, resolve conflicts keeping both sides, then run `scripts/gate --no-fail-fast`, `cargo t -P perf` once all lanes are in and no lane builds, and `cargo check --workspace --all-targets` (the static build compiles; links nothing) before the PR. The gate runs here once per round, not once per subagent. A spike lane stays out of the round; a separate throwaway `playtest/<n>` branch can carry it for comparison.
5. **Hand over** the round to the initiator to play: the run command, what changed, what to look at.
6. **Merge** only on the initiator's release: one PR `round/<n>` → `main` as a merge commit, then tag the merge commit `sprint/<milestone>-<what>` (annotated, pushed). Rules for tags, merges and deleting branches: concept repo `docs/LEARNINGS.md`, section "Git and spike states".
7. **Clean up**: for each lane branch, check `git merge-base --is-ancestor origin/<branch> origin/main`; then delete the branch locally and on origin, and remove a worktree or target dir the lane left behind. Leftovers show in `btrfs filesystem du -s ~/.cache/exo-1-target*` (the `Exclusive` column is what deleting frees).
