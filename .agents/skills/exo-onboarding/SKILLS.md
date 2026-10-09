# Skills for EXO-1 contributors

Source: <https://github.com/mattpocock/skills>. We take a subset, not the whole set.

## Install

Needs Node. From the repo root, after the contributor approves:

```sh
npx skills@latest experimental_install
```

This restores exactly the skills in `skills-lock.json` into `.agents/skills/`, which OpenCode and Codex read directly. The installer rewrites the hashes in `skills-lock.json` on every machine (they differ per machine, cause not isolated); never commit that change, discard it with `git checkout skills-lock.json`. Check with `npx skills@latest list`. If the command fails, fall back to `npx skills@latest add mattpocock/skills --agent <agent> --skill <names from the table> --yes`; the `--yes` flag is only appropriate after the contributor approved it.

Claude Code reads `.claude/skills/`, so it needs the links under "Link for Claude Code" below. Do not install the Claude Code plugin as well, or every skill appears twice.

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
| `tdd` | red-green loop at agreed seams; `implement` calls it |
| `code-review` | review a PR |

`setup-matt-pocock-skills` is part of the restore, but its output (`docs/agents/`, the `AGENTS.md` section) is already committed. Do not run it again, it would propose the configuration anew.

The external skill folders are local copies, not project-authored skills. The repo `.gitignore` excludes these specific folders while preserving project skills under `.agents/skills/`; keep `skills-lock.json` available as the install manifest. Do not ignore `.agents/skills/` wholesale.

## Link for Claude Code

The skills in `.agents/skills/` are found by OpenCode and Codex directly. Claude Code reads `.claude/skills/` instead. Link them locally, no commit. Skip a skill the contributor already has in `~/.claude/skills` (a personal skill of the same name wins, and the link would only duplicate it); the Linux/macOS command below does that, on Windows check by hand:

```
# Linux, macOS
mkdir -p .claude/skills && for d in .agents/skills/*/; do n=$(basename "$d"); [ -e ~/.claude/skills/$n ] || ln -sfn "../../$d" ".claude/skills/$n"; done
```

```
# Windows (PowerShell)
New-Item -ItemType Directory -Force .claude\skills | Out-Null
Get-ChildItem .agents\skills -Directory | ForEach-Object { New-Item -ItemType Junction -Force -Path ".claude\skills\$($_.Name)" -Target $_.FullName }
```
