# EXO-1

A goofy co-op space game in a strange galaxy: walk, fly and land on planets, carry cargo, take jobs. Small groups of about 2-5 players.

## Language

### Bodies and movement

**Frame**:
A moving reference frame that carries things with it: a planet, a ship, later a station. A thing belongs to one frame and keeps that frame's velocity.
_Avoid_: physics grid, render frame (say tick for one render or simulation step)

**Frame zone**:
The distance around a planet inside which things join that planet's frame.
_Avoid_: adoption radius, gravity well

**Cabin**:
The walkable inside of a ship, with its own gravity (LAG).
_Avoid_: interior, deck

**LAG**:
The cabin's artificial gravity, a level from 0 to 1 that ramps and can be switched.
_Avoid_: ship gravity, floor gravity

**Warp**:
Real, fast movement between two planets along a path, with a spool-up, a calibration and a cooldown.
_Avoid_: quantum travel, jump, hyperspace

### People

**Player**:
The human at a keyboard or pad, one per slot in a co-op session.
_Avoid_: user, client

**Walker**:
A player's body on foot, in gravity or weightless in a suit.
_Avoid_: character, avatar, pawn

**Character model**:
How a walker looks. Only the look, never the movement.
_Avoid_: skin, avatar

### Places

**Biome**:
A row of a planet's recipe: a point or intervals in the space of the shared macro fields (elevation, temperature, moisture, landform, weirdness, height above sea), with its ground colours, scatter multipliers and allowed site kinds. Each spot belongs to the nearest row. Rows have ids; names come from the initiator.
_Avoid_: zone, region (a region is a band of the height noise), climate

**Site**:
A spot on a planet that the generator found flat enough to build on.
_Avoid_: spawn point, POI

**Site kind**:
A recipe row that says what a site is and where it may be: footprint, allowed biomes, slope and height filters, count, separation, the ground edits under it and its kit of pieces. Kinds are placeholders until the initiator names them.
_Avoid_: site type, structure, POI

**Landmark**:
A unique, large thing visible from afar (a giant arch, a lone huge tree), one to three per planet, placed on high ground so it shows above the horizon. A site kind of the category landmark.
_Avoid_: monument, wonder

**Location**:
A site with content: a name, goods, jobs.
_Avoid_: outpost, town, place

**Pad**:
The flat landing area at a location.
_Avoid_: landing zone, helipad

### Goods

**Commodity**:
A kind of good, defined as data: name, base price, crate size, tags.
_Avoid_: item, resource

**Crate**:
A physical object in the world that holds commodities. Comes in a few standard sizes.
_Avoid_: box, container, package

**Cargo**:
Whatever crates a ship currently carries.
_Avoid_: load, inventory

### Gameplay

**Crew**:
The players of one co-op session together. Money, unlocks and jobs belong to the crew.
_Avoid_: party, team, group

**Client id**:
The id a game install creates once and keeps; the host save keys personal tracks by it. Not the network slot.
_Avoid_: player id, user id

**Domain event**:
A message that something happened in the game ("crate delivered at a pad", "unlock bought"). The host applies each one once (by its id); every system reads them. In this project "event" means only this.
_Avoid_: action, command, signal; route event (say encounter)

**System**:
One area of gameplay in its own `*_core` crate with its own state, events, content records and save section: jobs, encounters, later farming, mining, markets. Systems talk only through domain events and conditions.
_Avoid_: module, feature, manager

**Condition**:
A small typed rule in the data (track at least, has tag, flag set, all, any, not) that says when something is available.
_Avoid_: requirement, prerequisite, gate

**Progress track**:
A number the crew or a player owns, with thresholds: money (the crew track `wallet`), XP per job kind, later reputation.
_Avoid_: stat, skill, currency

**Unlock**:
Something the crew buys with money that grants tags; places and job templates with those tags become available.
_Avoid_: upgrade, perk, license

**Tag**:
A label on content (`dusty`, `route_bent_spoon`). The crew owns the tags its unlocks grant.
_Avoid_: category, keyword

**Flag**:
A fact a system raised at runtime (`job_completed:<template>`) that conditions can ask for.
_Avoid_: achievement, state

**Job**:
Work the crew took on: an instance of a job template with objectives, a reward, maybe a deadline.
_Avoid_: mission, quest, contract

**Objective**:
One part of a job that counts domain events until it is done (deliver, later find, collect, harvest).
_Avoid_: task, goal, step

**Offer**:
A job not yet accepted, on a board.
_Avoid_: listing, posting

**Board**:
The list of offers at a location.
_Avoid_: mission board, terminal, job list

**Encounter**:
Something that happens to the crew on the way (floating cargo, a systems hiccup), drawn from pools by a seeded timer.
_Avoid_: route event, random event, incident
