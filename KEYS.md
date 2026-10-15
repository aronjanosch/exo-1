# Keys

Every key of the game, sorted into **game** and **debug**. The keys come from `content/tuning/bindings.json`; the settings menu's Keybinds tab changes them in the game. The action name is the key in `bindings.json`.

Marks: **provisional**: the key is not decided yet · **cut?**: may go after the playtest.

## Game

### On foot

| Key | Action | What it does | Pad |
|---|---|---|---|
| W A S D | `move_x`, `move_z` | Walk | left stick |
| Mouse | – | Look | right stick |
| Shift | `run` | Run | left stick click |
| Space | `jump` | Jump | South |
| F | `interact` | The one verb: sit or stand up, pick up or set down a crate, collect an item, a counter, use a display and leave it | North |
| R | `throw` | Throw the held crate, else the item in the hand | right stick click |
| G | `drop` | Drop the item in the hand; a held crate is set down | – |
| 1 to 8 | `slot_1`, `slot_2`, `slot_3`, `slot_4`, `slot_5`, `slot_6`, `slot_7`, `slot_8` | Take a slot of the action bar into the hand (while the dev menu is open the digits run its commands) | – |
| Tab | `inventory` | Open or close the own inventory with a mouse cursor: a click takes a slot into the hand, a drag moves, stacks or swaps | – |
| – | `lag` | Cabin gravity by hand, only in a landed ship. No key: later the captain sets it at a terminal; scripts fire it | – |

### At a display

F on a display (a machine's or console's screen, looked at from up to 2.5 m) frees the mouse cursor, and the cursor is then the pointer on the screen in the world: point and click the buttons, toggles and sliders right on it, drag a slider (it follows the cursor also beside the screen). Nothing opens over the view. The walker stands still and the world runs on; damage does not pull it off. Others see the live panel on the screen. One player per screen.

| Key | Action | What it does | Pad |
|---|---|---|---|
| Mouse, left button | – | Click on the screen under the cursor (a cursor off the screen hits nothing) | – |
| F | `interact` | Leave the display | North |
| Escape | `display_leave` | Leave the display (does not open the menu then) | – |

### Ship

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
| K | `landing_mode` | Landing mode (SC switch, see below): precision near the ground | – |
| F | `interact` | Stand up | North |

### SC flight model (the default)

The switches of Star Citizen's flight control (the flight model is the SC model, #206). Ctrl and K as above; H and F8 are the SC switches below.

| Key | Action | What it does | Pad |
|---|---|---|---|
| H | `hover_assist` | Gravity compensation on/off: off, the ship falls with full gravity | D-pad up |
| F8 | `turn_cap` | G-safe on/off: thrust and turns stay inside the G tolerance | – |
| B | `master_mode` | Master mode SCM ↔ NAV (provisional) | – |
| U | `comstab` | Comstab on/off: off, the ship slides wide in turns (provisional) | – |
| P | `proximity_assist` | Proximity assist on/off: catches a dive towards the ground (provisional) | – |
| I | `wind_comp` | Wind compensation on/off: off, the ship drifts with the wind (provisional) | – |
| Page Up / Page Down | `limiter_up`, `limiter_down` | Speed limiter in 10 % steps; the mouse wheel later (provisional) | – |

### Quantum drive

| Key | Action | What it does | Pad |
|---|---|---|---|
| N | `warp_target` | Select the jump target | Select |
| J | `warp` | Warp; J again cancels while spooling | West |
| J (hold 1 s) | `warp_exit` | Emergency exit during the jump | West |

### Jobs, map, notices

| Key | Action | What it does | Pad |
|---|---|---|---|
| M | `map` | Open or close the map | – |
| T | `track_job` | Track the next active job (arrow and map target) | – |
| Y, Y | `abandon_job` | Drop the tracked job: the first press asks, the second drops it | – |
| – | `next_offer` | Next offer at the counter. No key: jobs move to a terminal or a talk; scripts fire it | – |
| Backspace | `decline` | Close the counter without taking the job | – |
| Enter | `skip_notices` | Run the queued banners and notices fast | – |
| F1 | `help` | The keys that apply right now (on foot, suit or in the ship) | – |
| Escape | – | Menu; releases the mouse | – |

## Debug and comparison

| Key | Action | What it does |
|---|---|---|
| F3 | `debug_hud` | Debug lines under the HUD: frame time, chunks, flight values, model |
| F9 | `camera_fx` | Camera effects on/off: shake, lag, field of view from G |
| V | `debug_fly` | Debug flight on foot |
| O | `orbit_camera` | Orbit camera |
| F10 | `dev_menu` | Dev menu (dev builds only): cheats for playtests |
| 1 / 2 / 3 | `dev_1`, `dev_2`, `dev_3` | In the open dev menu: grant every licence / +1000 credits / refill the boost |
| 4 / 5 | `dev_4`, `dev_5` | In the open dev menu: always day / always night (the clock stands) |
| 6 / 7 | `dev_6`, `dev_7` | In the open dev menu: jump to day / to night, the cycle runs on |
| 8 | `dev_8` | In the open dev menu: put the ship 150 m above the next settlement, stopped and level |
| 9 | `dev_9` | In the open dev menu: noclip on/off (through walls along the view; walk keys, jump up, crouch down, run faster) |

One key may serve several actions when they apply in different situations: Space jumps on foot and lifts the ship, Shift runs and boosts.
