# Skills for EXO-1 contributors

Source: <https://github.com/mattpocock/skills>. We take a subset, not the whole set.

## Install

```
npx skills@latest add mattpocock/skills
```

The installer asks which skills and which agents. Pick the agent the contributor uses (opencode, Claude Code, Codex, ...) and tick exactly the skills below. If the installer cannot preselect them, read the list aloud and let the contributor tick. Needs Node.

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
