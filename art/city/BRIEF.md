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

- **Flat colours only:** vertex colours, no textures. Three materials: `paint`, `glass`, `glow` (emissive).
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
| `landmarks.py` | one-offs that anchor a quarter and show over the horizon (company building, beacon spire) |
| `shops.py` | the first round shops (saucer diner, pod tower, bubble shop) |
| `lanes.py` | hover-lane tiles: straight, tee, crossing, curve, end, plaza, parking pads |
| `props.py` | street and roof props |
| `flora.py` | alien plants |
| `vehicles.py` | hover cars, van, bus |
| `street_preview.py` | the first quarter laid out from the exported models; live or headless renders |
| `catalog.py` | contact sheets: every model by family, labelled, one render per family |

`kit.Part.placed(matrix)` builds a part in its own frame and turns it into place: facades are built facing -Y and turned onto any side.

## What the game reads

- Empties parented to the building, with custom properties (glTF extras):
  - `kind = "door"` plus `use` (`shop`, `flat`): sits 1 m in front of the door, on the ground, where a walker stands.
  - `kind = "pad"` plus `size`: a hover-car pad, on its surface.
- Colours come as `COLOR_0`; Bevy multiplies it into the material. Blender's own glTF import ignores it (`street_preview.py` puts the kit materials back).
- Where a building sits in the city and where the city sits on the planet: a separate placement file, not in the model (`exo-1-concept/docs/research/place-authoring.md`).

## Open (initiator)

- Names and fiction of the shops; which ones are enterable (interiors in the same model, like Schedule I).
- Do walkers cross the hover lane freely, or only at crossings?
