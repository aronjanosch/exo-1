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

**Location**:
A site with content: a name, goods, jobs.
_Avoid_: outpost, town, place

**Pad**:
The flat landing area at a location.
_Avoid_: landing zone, helipad

### Goods

**Commodity**:
A kind of good, defined as data: name, base price, volume.
_Avoid_: item, resource

**Crate**:
A physical object in the world that holds commodities. Comes in a few standard sizes.
_Avoid_: box, container, package

**Cargo**:
Whatever crates a ship currently carries.
_Avoid_: load, inventory
