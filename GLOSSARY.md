# EXO-1

A co-op crew game in space: friends run one walkable ship together, each in a role, fly and land on planets and explore them. Crews of about 2-5 players.

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

**Room**:
One compartment of the cabin (bridge, engine room, cargo hold, airlock …), closed off by doors, with its own air.
_Avoid_: compartment, section, module

**LAG**:
The cabin's artificial gravity, a level from 0 to 1 that ramps and can be switched.
_Avoid_: ship gravity, floor gravity

**Warp**:
Real, fast movement between two planets of one star system along a path, with a spool-up, a calibration and a cooldown.
_Avoid_: quantum travel, hyperspace (between star systems say jump)

**Jump**:
The move from one star system to the next. It cannot be undone, and an emergency jump costs a charge.
_Avoid_: warp (that stays inside a star system), hyperspace

**Star system**:
A star with its planets, stations and sites; the unit of play of the campaign.
_Avoid_: system (that word means a gameplay system), sector, level

**System clock**:
The time a crew has in one star system before it turns lethal.
_Avoid_: chaos clock, timer, doom clock

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

**Cabinet**:
A fixed piece of furniture with slots that hold small items, such as a medical cabinet, a locker or a supply crate's inside. Each slot caps its stack and takes only some kinds; some open only with a key card.
_Avoid_: container (the code type), storage, chest

**Batch**:
One brew from the lab's mixer on its way through the other lab devices (heater, centrifuge) into syringes. It carries its effects, quality and volume in doses.
_Avoid_: brew (the mixing step), product (the finished good), sample

### Gameplay

**Crew**:
The players of one co-op session together, plus the bots on their ship. Money, unlocks and jobs belong to the crew.
_Avoid_: party, team, group

**Role**:
A player's job on the ship: captain, pilot, engineer, tactical officer or scientist. A role gives bonuses in its own field; anyone may help elsewhere at a penalty.
_Avoid_: class, job (a job is a contract)

**Station**:
A place on the ship where one role works a device: the helm, the reactor, a turret, the lab bench.
_Avoid_: console, post, terminal; a station in space is a space station

**Display**:
A screen in the world that a walker uses with the interact key: the mouse cursor comes free and points at its panel, the walker stands still until it leaves. A station may have several.
_Avoid_: terminal, monitor, screen (the player's screen)

**Panel**:
What a display shows: labels, buttons, bars, toggles and sliders laid out in panel pixels, rebuilt each tick from its owner's state. A click on it names the widget's action.
_Avoid_: page, UI, menu (menus are outside the game world)

**Space station**:
A place in space where the crew docks, trades, takes missions and saves; the game starts on one.
_Avoid_: station (that is a place on the ship), outpost, hub

**Skill**:
A value from 0 to 100 that grows by use and sets how fast and how surely a character does something, such as repairing or steering.
_Avoid_: stat, ability, level

**Talent**:
A perk a character unlocks in its role's talent tree with points earned from experience.
_Avoid_: skill, perk, ability, upgrade

**Ship system**:
One working part of the ship, such as reactor, shields, life support, engines or turrets, with its own condition and power draw.
_Avoid_: system (that word means a gameplay system), component, subsystem

**Device condition**:
How whole a ship system or device is, 0 to 100: below the repair threshold it asks for repair, below the broken limit it stops; it falls with wear, overload and fire, and rises while someone holds the interact key on it with a tool.
_Avoid_: health, durability, HP; "condition" alone is the data rule

**Power board**:
The engineer's display that shares the reactor's output between the consumer groups (engines, shields, weapons, life support, lights): a limit and a priority per group.
_Avoid_: power panel, conduit screen, fuse box

**Brownout**:
Less power than the groups ask for: the high priorities are served first, a group short of power works slower, and one below its minimum share switches off.
_Avoid_: blackout (no power at all), power cut

**Hit**:
One blow to the ship, as data: its damage type (energy, physical, collision), strength, the hull section it lands on, and what it can cause (hull damage, a breach, fire, damage to the ship systems in the room).
_Avoid_: shot, impact, damage event

**Hull section**:
A piece of the ship's outer wall belonging to one room, with its own damage; past a tenth of damage it leaks.
_Avoid_: hull segment, plate (plating is the armour level), wall

**Breach**:
A hole in a hull section to space: air leaves through it, faster the bigger the hole, and a big one pulls people and loose things towards it. Patching the section closes it.
_Avoid_: leak (say breach for the hull), hole, puncture

**Scram**:
The reactor's safety shutdown after overheating too long: power off, then a short lockout before it can start again.
_Avoid_: meltdown (what happens with the safety off), trip

**Bot**:
An AI crew member that fills an empty station and works through a priority list.
_Avoid_: NPC (people in the world), AI player

**Priority table**:
The captain's settings for the bots: per bot role a level from ignore to max for each kind of task, with overrides by alert level, crew order or a flag.
_Avoid_: AI settings, bot config

**Crew order**:
What the captain tells the crew or one role to do now (go to a room, operate a station, repair a device, follow me), shown to players as a HUD line and obeyed by bots above their priority table.
_Avoid_: command, task (a task is open work a bot can pick)

**Alert level**:
Green, yellow or red: how much danger the captain declares for the ship; a new threat raises green to yellow by itself.
_Avoid_: alarm (an alarm warns about one thing: fire, breach, the clock)

**Client id**:
The id a game install creates once and keeps; the host save keys personal tracks by it. Not the network slot.
_Avoid_: player id, user id

**Domain event**:
A message that something happened in the game ("crate delivered at a pad", "unlock bought"). The host applies each one once (by its id); every system reads them. In this project "event" means only this.
_Avoid_: action, command, signal; route event (say encounter)

**System**:
One area of gameplay in its own `*_core` crate with its own state, events, content records and save section: jobs, encounters, later farming, mining, markets. Systems talk only through domain events and conditions.
_Avoid_: module, feature, manager; for the ship's reactor or shields say ship system, for a star with planets say star system

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
