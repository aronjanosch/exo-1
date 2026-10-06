# Spike 4: approved experimental scope

Authorization: initiator on 2026-10-04, “Wir starten Spike 4 (Netzwerk,
Client-Autorität)”, then “Erledige Spike 4 vollständig” and permission to
explore and address undocumented gaps. LAN testing on two computers was
explicitly added. Source with Godot 4.7.2 is sufficient; a binary is optional.

Scope is the concept repo's `docs/SPIKE-4-BRIEF.md`, all seven numbered
questions. This is disposable spike code, not a production feature proposal,
community approval or a settled multiplayer design.

Risk: **red, networking**, explicitly authorized and isolated in
`spikes/network/`. Existing controllers are reused without edits. No autoload,
addon, CI or root-project configuration changes. No engine/runtime shell,
external file access, object deserialization or network-loaded executable code.

Vision fit: **fits with changes as an experiment**. VISION describes
host-authoritative small co-op; the initiator explicitly authorized investigating
client authority. Hosting, meeting, player count and contact ownership remain
open. Test counts 2/8 are measurement fixtures, not game design decisions.

Acceptance/evidence:

1. Real Jolt ship: takeoff, cruise, turns, landing; delayed-path position and
   rotation error, frame-step error, jerk, own-input response.
2. Cartesian 20/30 Hz × 100/150 ms buffer × 0/50/150 ms delay × 0/1/5/10% loss
   × 2/8 players; ±20 ms jitter for the delayed links. Record all cases.
3. Real localhost ENet with 2/8 instances, CPU and ENet byte counters; same-link
   host-authority reference for one client ship, without prediction.
4. Independent origins and planet ids, shared-coordinate history across shifts;
   both numerical checks and real two-planet ENet clients.
5. Contact failure fixture in two independent physics worlds; problems reported,
   not hidden behind a new contact-ownership rule.
6. Real walker inside a moving foreign proxy; owner/frame/local pose on the wire,
   parent transitions, live bot access through narrow stdio MCP.
7. Plain Godot RPCs and interpolation evaluated. Add netfox only if needed and
   separately authorized; no addon necessary for the measured flight paths.
8. LAN bind, configurable host IP and UDP port, portable source package; short
   manual two-computer acceptance instructions. Actual two-computer LAN test
   remains manual, as requested.

Assumptions, not decisions: 5 km reference radius, flat 4 km landing patch,
second coordinate frame 200 km away, static planets; existing assisted-flight
tuning; receiver-side independent snapshot loss, known artificial delay plus
extra playout buffer, no extrapolation; kinematic remote colliders; test-placement
boarding; host relay topology and slot numbers 1–8; last-seen expiry 2 s.

Measured fixture acceptance bound (not an aesthetic judgment): at 30 Hz/150 ms
extra buffer, no underruns and p95 delayed-path error <10 mm across the fixed
seed matrix. By-eye flight smoothness and actual LAN play are manual checks.
