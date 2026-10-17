---
name: exo-handover
description: Finish a piece of work and hand it to the initiator. Use when a task, fix, research job or doc change is done, before saying "fertig", or when the initiator asks for the state (pushed, branch, what to test). Lanes and rounds use exo-orchestrate instead.
---

# exo-handover

A **handover** leaves the initiator nothing to ask: where the work is, how to try it, what is still open, and a clean machine. In past sessions the initiator had to ask "ist gepushed?", "welcher Branch?", "welche Files?" and "wieso 55 GB?" again and again; each step below answers one of those before it is asked.

## 1. Land it

Commit and push what was asked for. Small doc and skill changes go straight to `main` from a worktree on `origin/main` (the checkout often sits on a round branch; the push prints a "must be made through a pull request" bypass notice and still lands, so check `git log origin/main`). Features go on their branch; one branch and at most one PR per piece of work. Done when `git status` is clean for your files and the remote has the commit.

## 2. Run what you hand over

Every command the initiator should type, run once yourself exactly as written (dynamic build, `--mute` for window runs). Done when it starts without an argument or build error.

## 3. Clean up

Remove what this work left: worktrees of pushed branches (`git worktree remove`), their target dirs (`~/.cache/exo-1-target-<name>`), finished background processes (`pgrep -af cargo`), scratch files that matter to nobody. Leave other sessions' worktrees and unstaged files alone. Done when you can say how many GB you freed.

## 4. Report

Short, in German, in this order:

- **Stand:** repo, branch, commit hash, pushed yes/no, tag if any.
- **Geändert:** the files, one line each on what changed.
- **Testen:** numbered steps, each with the expected result; for feel changes the run command from step 2.
- **Offen:** decisions only the initiator can take, as numbered questions with your recommendation; then the next step you propose.
- **Aufgeräumt:** what was removed and the space freed.

Show only what is new since the last report.
