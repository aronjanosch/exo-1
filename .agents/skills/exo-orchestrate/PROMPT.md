# Lane prompt template

Fill the angle brackets; leave out lines that do not apply. The prompt is the task; a `/goal` is only the checkable end state and only for a lane that runs for hours.

## Prompt

```
Lane <name> for issue #<n>: <one-paragraph task, the outcome in game terms>.
Read first: <concept repo docs, research notes, records under research/local/>.
Follow the lane rules of the skill exo-orchestrate (worktree ../exo-1-<name>,
branch <branch> from origin/main, CARGO_TARGET_DIR=~/.cache/exo-1-target-<name>).
Logic test-first in <crate> if it can break something; a scenario <scenario-name>
only if it crosses crates, driven through Controls. Feel gets no test.
Values and names as TODO(initiator) in <content file>. <Toggle key if the
initiator should compare variants by feel.>
Out of scope: <what stays untouched, e.g. files another lane owns>.
Report as a comment on #<n> when done.
```

## Goal (hours-long lanes only)

A goal judges only the transcript against its first wording. Keep it to the end state and the limits. Procedures (usage checks, review greps) belong in the prompt. When the initiator widens the task mid-run, set a new goal at once; the old one cannot pass any more.

```
/goal the feature's own tests (`cargo test -p <crate>`, plus `cargo ts scenario_<name>` if it has one) pass in the transcript on the pushed head of <branch>; <one measurable result, e.g. "the scenario prints drift below 1 mm">; the report comment on #<n> exists. Or stop after <N> turns.
```
