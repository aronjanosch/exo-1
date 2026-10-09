# Lane prompt template

Fill the angle brackets; leave out lines that do not apply. The prompt is the task; a `/goal` is only the checkable end state and only for a lane that runs for hours.

## Prompt

```
Lane <name> for issue #<n>: <one-paragraph task, the outcome in game terms>.
Read first: <concept repo docs, research notes, records under research/local/>.
Follow the lane rules of the skill exo-orchestrate (worktree ../exo-1-<name>,
branch <branch> from origin/main, CARGO_TARGET_DIR=~/.cache/exo-1-target-<name>).
Logic test-first in <crate>; a scenario <scenario-name> drives it through Controls.
Values and names as TODO(initiator) in <content file>. <Toggle key if the
initiator should compare variants by feel.>
Out of scope: <what stays untouched, e.g. files another lane owns>.
Report as a comment on #<n> when done.
```

## Goal (hours-long lanes only)

```
/goal `cargo t` and `cargo scenario` exit 0 in the transcript on the pushed head of <branch>; <one measurable result, e.g. "the scenario prints drift below 1 mm">; the report comment on #<n> exists. Or stop after <N> turns.
```
