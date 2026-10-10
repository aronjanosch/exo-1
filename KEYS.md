# Keys

Every key of the game, sorted into **game** and **debug**. The keys come from `content/tuning/bindings.json`; the settings menu's Keybinds tab changes them in the game. The action name is the key in `bindings.json`. A test (`controls::tests::keys_md_lists_every_action`) fails when an action is missing here.

Marks: **new** since round 5 · **provisional**: the key is not decided yet · **cut?**: may go after the playtest.

## Game

### On foot

| Key | Action | What it does | Pad |
|---|---|---|---|
| W A S D | `move_x`, `move_z` | Walk | left stick |
| Mouse | – | Look | right stick |
| Shift | `run` | Run | left stick click |
| Space | `jump` | Jump | South |
| F | `interact` | The one verb: sit or stand up, pick up or set down a crate, a counter | North |
| R | `throw` | Throw the held crate | right stick click |
| G | `lag` | Cabin gravity by hand, only in a landed ship | – |

### Ship (both flight models)

| Key | Action | What it does | Pad |
|---|---|---|---|
| W / S | `move_z` | Thrust forward / back | left stick Y |
| A / D | `move_x` | Strafe | left stick X |
| Space / Ctrl | `move_y` | Thrust up / down | triggers |
| Q / E | `roll` | Roll | bumpers |
| Mouse | `turn_pitch`, `turn_yaw` | Virtual stick: pitch and yaw | right stick |
| Shift | `boost` | Boost (capacitor) | left stick click |
| X | `brake` | Brake; in the SC model along the flight path | East |
| C | `decoupled` | Coupled ↔ decoupled, blends over 4 s | D-pad left |
| K | `landing_mode` | Landing mode: precision near the ground | – |
| F | `interact` | Stand up | North |

### SC flight model (after F7)

The switches of Star Citizen's flight control. H and F8 mean these in the SC model.

| Key | Action | What it does | Pad |
|---|---|---|---|
| H | `hover_assist` | Gravity compensation on/off: off, the ship falls with full gravity | D-pad up |
| F8 | `turn_cap` | G-safe on/off: thrust and turns stay inside the G tolerance | – |
| B | `master_mode` | Master mode SCM ↔ NAV (new, provisional) | – |
| U | `comstab` | Comstab on/off: off, the ship slides wide in turns (new, provisional) | – |
| P | `proximity_assist` | Proximity assist on/off: catches a dive towards the ground (new, provisional) | – |
| I | `wind_comp` | Wind compensation on/off: off, the ship drifts with the wind (new, provisional) | – |
| Page Up / Page Down | `limiter_up`, `limiter_down` | Speed limiter in 10 % steps; the mouse wheel later (new, provisional) | – |

### Axis flight model (old, frozen until SC is the default)

| Key | Action | What it does | Pad |
|---|---|---|---|
| H | `hover_assist` | Assist on/off (gravity hold and damping together) | D-pad up |
| L | `horizon_follow` | Horizon follow: the ship follows the planet's curve (cut?) | D-pad down |

### Quantum drive

| Key | Action | What it does | Pad |
|---|---|---|---|
| N | `warp_target` | Select the jump target | Select |
| J | `warp` | Jump; J again cancels while spooling | West |
| J (hold 1 s) | `warp_exit` | Emergency exit during the jump | West |

### Jobs, map, notices

| Key | Action | What it does | Pad |
|---|---|---|---|
| M | `map` | Open or close the map | – |
| T | `track_job` | Track the next active job (arrow and map target) | – |
| Y, Y | `abandon_job` | Drop the tracked job: the first press asks, the second drops it | – |
| Tab | `next_offer` | Next offer at the counter | – |
| Backspace | `decline` | Close the counter without taking the job | – |
| Enter | `skip_notices` | Run the queued banners and notices fast | – |
| F1 | `help` | The keys that apply right now (on foot, suit, ship axis or SC) | – |
| Escape | – | Menu; releases the mouse | – |

## Debug and comparison

| Key | Action | What it does |
|---|---|---|
| F7 | `flight_model` | Flight model axis ↔ SC, until SC is the default (new) |
| F3 | `debug_hud` | Debug lines under the HUD: frame time, chunks, flight values, model |
| F6 | `boost_mode` | Axis model: boost as the capacitor or the old speed stage (cut?) |
| F8 | `turn_cap` | Axis model: the G-safety turn cap (in the SC model: G-safe, above) |
| F9 | `camera_fx` | Camera effects on/off: shake, lag, field of view from G |
| V | `debug_fly` | Debug flight on foot |
| O | `orbit_camera` | Orbit camera |

One key may serve several actions when they apply in different situations: Space jumps on foot and lifts the ship, Shift runs and boosts.
