# Skills for EXO-1 contributors

Source: <https://github.com/mattpocock/skills>. We take a subset, not the whole set.

## Install

Needs Node. Avoid the interactive selector in agent sessions; explicitly pass the agent and exact skill names from the table. Example for OpenCode (run from the repo root, after asking permission to install):

```sh
npx skills@latest add mattpocock/skills --agent opencode --skill setup-matt-pocock-skills writing-for-agents grilling domain-modeling codebase-design to-spec to-tickets implement code-review --yes
```

Replace `opencode` with the contributor's agent identifier and keep the skill list in sync with the table below. Verify the result with `npx skills@latest list --agent <agent>`; the installer can also report success while the current session's skill catalog remains stale, so check the local skill files if needed. The `--yes` flag avoids an interactive prompt that cannot complete in a non-TTY session; it is appropriate only after the contributor has approved this exact installation.

Claude Code users can alternatively install the plugin (`claude plugins install mattpocock-skills`), but that brings every skill. Install one way, not both, or every skill appears twice.

## Skills to take

| Skill | Why |
|---|---|
| `setup-matt-pocock-skills` | the other skills need its setup |
| `writing-for-agents` | required when editing skills, `AGENTS.md` or `CLAUDE.md` |
| `grilling` | sparring on a plan or idea |
| `domain-modeling` | glossary and ADRs |
| `codebase-design` | module interfaces and seams |
| `to-spec`, `to-tickets` | turn an agreed idea into slices |
| `implement` | build one slice |
| `code-review` | review a PR |

Run `setup-matt-pocock-skills` once after installing.

The external skill folders are local copies, not project-authored skills. The repo `.gitignore` excludes these specific folders while preserving project skills under `.agents/skills/`; keep `skills-lock.json` available as the install manifest. Do not ignore `.agents/skills/` wholesale.

## Project skills

The skills in `.agents/skills/` of this repo (including `exo-onboarding`) are found by opencode and Codex directly. Claude Code reads `.claude/skills/` instead. Link them locally, no commit:

```
# Linux, macOS
mkdir -p .claude/skills && for d in .agents/skills/*/; do ln -sfn "../../$d" ".claude/skills/$(basename "$d")"; done
```

```
# Windows (PowerShell)
New-Item -ItemType Directory -Force .claude\skills | Out-Null
Get-ChildItem .agents\skills -Directory | ForEach-Object { New-Item -ItemType Junction -Force -Path ".claude\skills\$($_.Name)" -Target $_.FullName }
```
