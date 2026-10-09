# City brief

The first city: hand-built, not procedural, placed onto the planet as a whole. Direction from the initiator, 2026-10-08:
- "Eine futuristische höher gebaute Version von schedule 1 vll."
- "mehr futuristisch, flache farben, schwebeverkehr, richtung futurrama"

All numbers below are starting values (guide values, not rules); change them here when a review shows better ones.

## Feel

- Retro-future in the spirit of Futurama's city, but with our own buildings. The cartoon future of the 1950s and 60s: saucers on stalks, tapering towers with ribbon windows, bulbs and domes on top, tail fins, boomerang signs, glass lift tubes, pads for hover cars.
- Schedule I's small dense town, grown upwards: a shop at street level, flats above, junk and gadgets on the roofs.
- Goofy on purpose. Every building has one silly idea (a saucer parked on a stick, a tower that is a stack of lit rings).
- Read from the street first: a walker at eye height (1.7 m) on the opposite side sees the shop floor, the sign and the first floors. Upper floors read as silhouettes, rings and lit bands.
- Inspiration only: no shapes, signs or names taken from Futurama or any other show.

## Measures

| What | Value | Note |
|---|---|---|
| Lane tile | 10 × 10 m | 6 m hover lane, 2 m walkway each side, all flush (no kerb: traffic floats) |
| Hover traffic | cars float about 1–2 m over the lane | the car model sits on its origin; the game lifts it. Cars park on pads at buildings |
| Facade bay | 4 m | buildings are 1–3 bays wide or round |
| Facade grid | 0.5 m | windows, doors and signs snap to it |
| Shop floor | 4.5 m | upper floors 3.5 m |
| Door | 1.5 × 2.5 m | wider than a human door, aliens come in all sizes |
| Lot | the building plus its awnings, fins and pads | the model stays inside the lot it declares; the check enforces it |

## Look

- **Flat colours only:** vertex colours, no textures. Four materials: `paint`, `glass` (dark, opaque), `glow` (emissive), `clear` (see-through, alpha 0.25: shop windows into a room).
- **No shared outer faces:** two parts that overlap must not end in the same plane on a visible side (z-fighting). Walls stop under ceiling slabs, side walls stand between front and back walls, bands stop inside end piers. The kit check reports `coplanar overlaps` as a PROBLEM.
- Smooth shading with hard edges (edge split at 40°). Round shapes get enough segments to read round (16–48).
- **Palette per building:** one body colour, one accent, one glow colour. All buildings share the warm white `TRIM` (slabs, rings, frames) and `METAL`, so the streets hang together. Glows are cyan and pink.
- Kit shapes: `box`, `rounded_box`, `cylinder` (tapered), `dome`, `sphere`, `torus`, `prism` (outline extruded in any plane), plus the facade helpers `window`, `door`, `sign`.

## Recipe for houses (from the Schedule I review, 2026-10-09)

The simple boxes work when the facade carries relief and the silhouette carries the idea:

- **Relief, not colour:** piers on every bay line (0.3 m proud), floor bands, windows set back between them with frame strips, a deep sill, mullions and a spandrel panel. Light and shadow do the work.
- **False front:** the street facade rises above the roof (step, arch, fin, butterfly, saw). The house's silhouette from the street comes from it, not from the box.
- **Muted bodies, strong accents:** body colours are greyed pastels; only the accent (doors, awnings, fins) and the glow are saturated.
- **Glow strength 0.6:** higher burns lit windows white in Eevee and loses their colour.
- **Backs and roofs:** a plain back facade (small windows, a back door, a drainpipe) and junk on every roof (air units, vents, dishes).
- **Corners:** a corner house carries a second street facade and something tall on the corner (turret or sign pylon).
- Door sizes are guide values; anything at least the walker's size (capsule 0.7 x 1.8 m) works, a company gate may be 8 x 10 m.

## Scripts and families

| Script | Family |
|---|---|
| `rows.py` | row houses (one recipe, parameters per house) and corner houses |
| `interiors.py` | fit-outs for ground-floor rooms: shop, bar, workshop, office, each for rooms of 1, 2 and 3 bays |
| `landmarks.py` | one-offs that anchor a district and show over the horizon (company building, beacon spire, charge stop) |
| `homes.py` | suburb bungalows with carports (butterfly roof, dome, flying wedge) |
| `shops.py` | the first round shops (saucer diner, pod tower, bubble shop) |
| `lanes.py` | hover-lane tiles: straight, tee, crossing, curve, end, plaza, parking pads |
| `props.py` | street and roof props, and the quest-giver placeholder `npc_marker` |
| `flora.py` | alien plants |
| `vehicles.py` | hover cars, van, bus |
| `city_plan.py` | the city as data: placements, roads off the grid, paving, pond, pylon runs, quest givers; no Blender |
| `street_preview.py` | builds the plan in Blender from the exported models, generates roads and cables; live or headless renders |
| `catalog.py` | contact sheets: every model by family, labelled, one render per family |

## City layout (2026-10-09)

Initiator: "was wir brauchen ist eine gut aussehende stadt ... organisch und echt wirken mit einem dichteren stadtteil", districts apart like Schedule I's, connected by longer roads; shops and quest givers in each; a big spaceport later, like Star Citizen's.

- **Downtown:** dense, on the 10 m lane grid: two parallel streets, a cross street, the plaza with the spire, the company building; the round shops as accents on the south street.
- **The arterial:** a curved road east (about 170 m) with pylons and cables, groves, the charge stop.
- **Ringside:** a suburb on an oval ring road: bungalows outside, a park with a pond inside, a short shop street north, a cul-de-sac south.
- **The west road:** outskirts with a few homes, ending at a fence where the spaceport will start.
- Roads off the grid are polylines in the plan, smoothed, generated as ribbons (paving, lane, glowing edges that stop at junctions); the game can generate them the same way.
- Green between districts: groves of trees and bushes; the ground is grass, districts are paved.

`kit.Part.placed(matrix)` builds a part in its own frame and turns it into place: facades are built facing -Y and turned onto any side.

## Enterable buildings (#94, initiator's yes 2026-10-09)

Initiator: "denk dran dass man in einige der gebäude auch rein will. am besten potentiell kann jedes begehbar gemacht werden um questgeber, händler usw. dort einzuquartieren".

- **Ground floors are real rooms in the open world.** `rows.shell` builds the room (floor, lined walls, ceiling with light strips), the facade builds the front wall with real holes: a shop door (`Part.doorway`) and the shop windows (`clear` glass). Upper floors stay solid.
- **The door leaf** is its own child object and slides up into the wall above the door (`slide`); the wall and the solid floor above hide it. No pocket beside the door is needed, so any door width works (the garage's 7 m too).
- **Fit-outs** (`interiors.py`) are separate models, `fit_<kind>_<bays>`, placed on the building's `room` anchor. The plan picks the kind per placement (`city_plan.fit`, default per model in `city_plan.FIT`), so one house is a bar in one street and a workshop in the next. Every fit-out keeps the front of the room clear (doors sit anywhere along the front), has a counter or desk across the back and the `npc` anchor behind it.
- **Flat doors** (`use = "flat"`, `"back"`) stay panels: upper floors, flats and large buildings are instanced behind a portal (like the caves in #75).
- **One-offs furnish themselves** (assumption, initiator said to carry on 2026-10-09): the fit-outs are rectangular 9.4 m deep rooms, so round or shallow rooms (the diner's kiosk, the charge stop's kiosk, the bubble shop's gumball shop) carry their furniture and `npc` anchor in the model, with no `room` anchor. The pod tower's shop block is big enough and takes a 2-bay fit-out from the plan. Doors with no wall above them slide sideways (`Part.doorway(slide=...)`); round walls come from `Part.ring_wall`, rooms with rounded corners from `Part.rounded_room`.
- **Homes:** each bungalow is one room: see-through glass to the street, a door that slides sideways into the front wall (the roof is too thin to hide it), furniture (sofa, kidney table, sputnik lamp, screen) and an `npc` anchor with `role = "resident"`. The butterfly house's chimney wall has a passage; the dome house is entered through a hollow tunnel.
- Quest givers behind counters and in homes come from the npc anchors; `city_plan.QUESTS` keeps only the ones out in the open.
- The company gate and the spire's lift stay flat doors: large interiors are instanced.
- Rooms carry no light; the game lights them (the preview puts one soft panel light per room as a stand-in).
- Review renders (`kit.review`) light rooms like the preview and add an `_inside` view from just inside the shop or home door towards the character.

## What the game reads

- Empties parented to the building, with custom properties (glTF extras):
  - `kind = "door"` plus `use` (`shop`, `flat`): sits 1 m in front of the door, on the ground, where a walker stands.
  - `kind = "pad"` plus `size`: a hover-car pad, on its surface.
  - `kind = "room"` plus `bays` and `size` [width, depth, height]: on the floor at the back wall's inner face, centred, facing -Y. The fit-out `fit_<kind>_<bays>` goes here.
  - `kind = "npc"` plus `role` (`trader`, `barkeep`, `mechanic`, `quest`, `cook`, `resident`), in a fit-out or a one-off: where the character stands, behind the counter, facing -Y. The plan's `role` overrides it.
- Child meshes of the building with `kind = "leaf"`, `door` (the door anchor's name) and `slide` [x, y, z]: the door leaf; the game moves it by `slide` (building frame, metres) to open the door.
- Placements in `city_plan.py` with a sixth element `{"fit": kind, "role": role}`.
- Colours come as `COLOR_0`; Bevy multiplies it into the material. Blender's own glTF import ignores it (`street_preview.py` puts the kit materials back).
- Where a building sits in the city and where the city sits on the planet: a separate placement file, not in the model (`exo-1-concept/docs/research/place-authoring.md`).

## Open (initiator)

- Names and fiction of the shops; which fit-out each building gets (`city_plan.FIT` holds placeholders).
- Do walkers cross the hover lane freely, or only at crossings?
