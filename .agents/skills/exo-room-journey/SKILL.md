---
name: exo-room-journey
description: Turn one room of the ship into a player journal and a walkthrough station. Use when the initiator dictates a room, asks for a player journal, or wants to work out a room or one of its machines (reactor, airlock, lab) before it is built.
---

# exo-room-journey

The ship is designed room by room along the player's **journey**, from the airlock inward. Each room becomes a **player journal** (a first-person scene in German, the initiator's voice) and then a **station** in the concept repo's `docs/WALKTHROUGH.md` (English, what the journey says, plus open questions). The epic with per-department tasks comes after, usually in a fresh session.

All paths below are in the concept repo (`docs/`). Commits there go straight to `main`; stage only your own files and leave other unstaged changes alone.

## 1. Read

`WALKTHROUGH.md` (format, neighbouring stations), `design/controls-guideline.md`, the role page in `design/`, and every row in `decisions/` that touches this room's machines. Done when you can name the decisions the room must respect.

## 2. Pick the entry

- **Dictated:** the initiator speaks the room. Save it verbatim (light cleanup at most) as `transcripts/<date>-<room>.md`, the scene headings as plain lines. Go to step 5.
- **Worked out:** the room or its machine is still an idea. Go to step 3.

## 3. Spar

Research first: the concept repo's `research/`, then the source files of the games we borrow from (local extracts, listed in `WORKSPACE.md`; fan the search out to a Haiku Explore subagent with exact search terms and ask for file:line quotes), then the web. Bring options with a **recommendation**, ask **one question at a time**, and follow the initiator's taste when they pull back ("zu krass" means simpler, not a variant of the same). A claim about another game that rests on memory gets checked in the files before it is settled; say what the files do not answer.

Done when every part the journal will show has the initiator's yes, or is on the list of your own inventions for step 4.

## 4. Draft the player journal

Write `transcripts/<date>-<room>-entwurf.md`: German, first person, present tense, short scenes under plain heading lines, in the voice of `transcripts/2026-10-17-schleuse-ausruestung-lager.md`. Show the room calm, then busy, then failing; machines as physical parts from the controls guideline; sounds and lights a crewmate notices from elsewhere. First line: who wrote it and that the initiator reviews it.

In chat, list what you invented (parts, numbers, sounds) so the initiator checks those first. Revise until they approve, then rename to `transcripts/<date>-<room>.md` and change the first line to "written by the agent, corrected by the initiator".

## 5. Record the decisions

Add or extend one row per topic in the right `decisions/<topic>.md`: the decision in plain words, then the initiator's words verbatim with the date. Name every older row this one overrides, and keep its other parts standing. Add the row name to its topic line in `DECISIONS.md`. Done when nothing the journal settles lives only in chat.

## 6. Write the station

Append `## <n>. <Room>` to `WALKTHROUGH.md`, in English: Source (transcript and decision rows), Place, the parts, Sequence blocks per situation, Parts, Sound, Open questions. Open questions are only what the initiator must decide; each names the gap in one line.

## 7. Hand over

Commit and push the concept repo. Report the files and the open questions, and offer the epic for the station in the code repo (one epic per station, tasks per department: design, art, gameplay, content, audio).
