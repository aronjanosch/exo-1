# Spike 4 report — client authority and LAN

Date: 2026-10-04. Godot 4.7.2, Jolt, Forward+/Vulkan. Throwaway branch
`spike/4-network`, baseline `9aa3cbd`; code isolated in `spikes/network/`.
Brief: concept `docs/SPIKE-4-BRIEF.md`; approved scope/assumptions: `SPEC.md`.
Labels: **measured** (runs), **verified** (code/protocol/invariants), **assumed**
(fixture choices), **open** (manual play or initiator decisions).

## Result

Client authority is technically viable for the tested independent flight and
cabin movement. Plain Godot RPCs plus a small interpolation buffer suffice for
these paths. **This does not settle the authority design:** ship-to-ship contacts
produce conflicting physical histories and still need a human system decision.

**Measured:** 96 replay configurations and 25 invariants pass. Real ENet runs
with two and eight processes, a host-authority reference, independent planet
frames, the MCP bridge and all-interface/LAN-address binding pass. Forward+
windowed runs produced valid screenshots, no errors and no mouse capture.
Actual two-computer LAN play and by-eye smoothness remain the documented manual
acceptance checks in `README.md`; they were not replaced with a localhost claim.

## 1. Flight and own-input latency

The actual existing ship controller and Jolt run on a cheap flat landing patch.
Captured at 60 physics ticks/s: takeoff, cruise, yaw turn, landing, idle; 1501
states over 25 seconds, 48 whole-metre origin shifts. Maximum height 64.08 m,
final speed 0 m/s. The source responds on the first physics tick (16.7 ms at
60 Hz); **no added network wait** is on the local controller path. Clock sync
only gates sending, not the authority's own simulation.

Hermite uses position and velocity; rotation uses slerp. Errors compare the
receiver with the sender's path at the **same delayed timestamp**, not with the
sender's current position. The 60 Hz source history is interpolated for that
oracle; this is a sampled-path measurement, not continuous ground truth.

Example: 30 snapshots/s, 150 ms extra buffer, 150 ms one-way delay, ±20 ms
jitter, 10% independent loss, two players. Units below are mm except jerk.

| Phase | RMS position | P95 position | Max position | P95 frame-step error | P95 jerk m/s³ |
| --- | ---: | ---: | ---: | ---: | ---: |
| cruise | 0.466 | 0.493 | 4.597 | 0.990 | 541.7 |
| idle | 0.000 | 0.000 | 0.000 | 0.000 | 635.8 |
| landing | 1.654 | 4.414 | 18.818 | 4.414 | 2516.8 |
| takeoff | 0.198 | 0.000 | 2.869 | 0.000 | 0.0 |
| turn | 0.367 | 0.775 | 2.738 | 0.900 | 530.0 |

P95 rotation error: cruise 0.040°, turn 0.172°. No underruns in this case.
Across **all** 30 Hz/150 ms-buffer cases (including eight players and every
loss/delay), worst P95 position error is 4.414 mm and underruns are 0%.
The 10 mm bound is a fixture acceptance check, not a claim about perceived feel.
Absolute numerical jerk is sensitive to float32 position quantization,
acceleration boundaries and floor contacts; it is recorded, but frame-step error
and by-eye play are better evidence of visible smoothness.

## 2. Latency, jitter and loss

Cartesian matrix: total players 2/8 × rate 20/30 Hz × extra buffer 100/150 ms ×
one-way delay 0/50/150 ms × independent loss 0/1/5/10% = **96 cases**. Delayed
links have uniform ±20 ms jitter. Separate deterministic RNG seeds per sender.
All raw per-phase results are in `results/matrix.json`.

At 20 Hz, 100 ms buffer, 150 ms delay and 10% loss, the two-player source has
0.833% holds in cruise and 0.556% in turns; maximum position error 1.062 m.
Increasing that buffer to 150 ms removes those holds in the same seed and lowers
cruise maximum error to 3.10 mm. **30 Hz/150 ms is the tested default**, not final
game tuning. The extra buffer adds to the one-way delay: this stress case displays
300 ms-old states, plus actual transport/scheduling delay. Loss bursts longer
than the buffer remain a limitation; there is no extrapolation through contacts.

Real eight-process ENet also has occasional brief holds: client observations
0–0.092% in the final run. Host totals around 15% include the deliberately longer
2 s shutdown drain after all clients stop; that statistic is not steady flight
quality. Receiving and rendering continue until a departed owner's 2 s expiry.

Fault injection is receiver-side **after actual ENet transport**, on snapshots
only; measured ENet bytes therefore include data later dropped by the fixture.
Input delay/loss is also applied in the host-reference mode. No `tc`, root network
changes or addon are used. ENet's adaptive unreliable-packet throttle is disabled
for this **LAN fixture** (`throttle_configure(1000, 32, 0)`), because eight local
physics processes otherwise added uncontrolled packet drops. This is not a WAN
congestion-control design or WAN result.

## 3. CPU, bandwidth and host-authority reference

Headless real-time runs are capped at 60 display frames/s. Otherwise headless
Godot spun at about 1000 display frames/s, repeatedly sampled all histories and
interfered with ENet polling. CPU results are from this machine, short runs and
a simplified scene, not performance guarantees. Absolute engine-physics monitor
values also include scheduling effects; no broad performance winner is inferred.

| Measured receiver | Process mean / P95 ms | Networking physics-script mean ms | Engine physics mean ms | ENet send / receive kB/s |
| --- | --- | ---: | ---: | --- |
| 2-player client authority, 150 ms/5% loss | 0.108 / 0.178 | 0.052 | 0.558 | 4.786 / 4.830 |
| 8-player client authority, 150 ms/10% loss | 0.454–0.510 / 0.725–0.841 | 0.040–0.046 | 0.603–0.694 | 4.747–4.792 / 32.630–32.707 |
| One remote ship under host authority, same 150 ms/5% loss | 0.181 / 0.288 | 0.060 | 0.678 | 3.252 / 9.499 |

`cpu_process` includes interpolation, proxy transforms, origin shifts and optional
bridge work. `cpu_physics_script` includes snapshot encoding/sending and test
inputs, **not** the imported controller/native solver; their work is included in
the separate engine monitor. RPC receive/relay totals are also stored in each
live result. Replay mean CPU including encode, receive, truth comparison and
metrics is about 0.035–0.046 ms/frame for one remote and 0.235–0.312 for seven.

Fixed payload is **144 bytes per player including ship and walker**. At 20/30 Hz:
2.88/4.32 kB/s per sender. Seven remote players at 30 Hz: 30.24 kB/s incoming
payload per client. Host relay fans out: 49 player streams, 211.68 kB/s outgoing
payload at steady eight-player traffic. Actual final host ENet traffic averaged
183.03 kB/s sent, 26.73 kB/s received over the whole 10 s session, including the
2 s drain. ENet counters include its framing/RPC/control traffic, not Ethernet/IP
wire headers. They are distinguished from the pure snapshot counters.

Host reference: host owns one additional real client ship; the client sends
inputs, freezes its own body and shows snapshots without prediction. Median
input-to-displayed-echo is **467.3 ms** in the 150 ms input + 150 ms snapshot +
150 ms buffer case, with localhost/scheduling overhead. Both approaches use the
same controller and snapshot layer. The unpredicted reference demonstrates the
latency difference; it does not compare a mature predictive host architecture.
Host engine-physics mean: 0.429 ms for client authority versus 0.572 ms for this
one-ship reference, in short separate runs. More host-authoritative ships and
actual low-spec hardware are not benchmarked.

## 4. Shared frames and independent origins

Snapshots contain planet id and planet-relative position encoded as three
scalar float64 values. They contain **no per-client local world coordinates**.
Planet centres and accumulated origins are scalar GDScript doubles; shared-to-
local conversion subtracts those scalars before creating a Vector3. The engine's
Vector3/source physics still has float32 precision (about 0.5 mm at 5 km);
encoding as doubles preserves the source, it cannot restore missing precision.

Verified: histories survive arbitrary client shifts without rewriting entries;
reordered/duplicate packets, old/new frame transitions and bounded history.
Measured: 48 shifts during the real ship capture, 0 mm shared-coordinate jump and
no velocity change. Real clients on planet ids 0/1, centres 200 km apart, use
independent origins. A live 10 km shift passes the reconstruction invariant;
`results/two-planets-frame.json` holds the evidence. Distant proxies remain
float32 nodes, so this is not a proof of close-up rendering or collision precision
for a body 200 km away from the observer's origin. Travel between planets is not
simulated by this spike; a planet-id transition holds the old frame until the
new timestamp rather than blending incompatible coordinates.

**Confirmed and fixed in this spike:** `_process` saw a moving ship's visual
transform one physics step behind the physics server. Shifting that old transform
rewound it: a 31 m/s source repeated a position for one tick at each shift,
producing about 0.52 m step errors. Shift now uses the current
`PhysicsServer3D.BODY_STATE_TRANSFORM` for dynamic ships, while frozen proxies and
other nodes use their display transforms. The replay spikes disappear; jump is
measured against that authoritative pre-shift transform. Other spike worktrees
were not changed or retested.

## 5. Contacts: unresolved authority conflict

Two separate World3D/Jolt spaces each own one dynamic ship and a delayed,
kinematic representation of the other. Head-on 15 m/s, receiver histories delayed
by 150 and 50 ms. Both detect the contact, at physics ticks **54 and 41**:
13 ticks = 216.7 ms apart. Largest proxy/authority disagreement **3.00 m**.
Recorded final velocities are in `results/matrix.json`.

A kinematic proxy can stop/push a local body, but it cannot receive the other
client's authoritative impulse. Contact times, response and mutual momentum can
therefore disagree; docking and multiple passengers could expose the same issue.
The spike intentionally does not invent a rule to reconcile those histories.
Who owns a ship contact, docking event or coupled physics island is **open for
the initiator**. Current ship/walker collision layers also intentionally prevent
passengers from pushing the ship, following spike 3's existing assumption.

## 6. Walker next to and inside another ship

Same CharacterBody3D movement and cabin controller as spike 3. Remote ship is a
frozen kinematic copy with the original deck/wall/ramp colliders. The local walker
becomes its child, with motion relative to the cabin. Measured: 350 m/s translated
and yawing foreign ship, origin stress, 360 physics ticks: lateral standing drift
0.000 m, deck contact 358/360 ticks. Walking within the cabin passes. Outside,
the planet-frame walker walks beside the parked foreign ship on the real floor:
24/24 grounded ticks; the network roundtrip preserves that planet-relative pose.
Live MCP tests board a foreign ENet ship and survive an independent local shift.

Minimum representation implemented: ship owner, planet id, timestamp; walker
frame kind (planet/ship), referenced ship owner, pose and velocity **in that frame**.
Observers sample all foreign ships/walkers at the same playout timestamp, then
compose the ship transform and local walker transform. Frame changes do not
interpolate ship-local coordinates against planet-local coordinates; state repeats
in every snapshot so a single lost entry/exit packet cannot permanently lose the
parent. Local boarding uses cabin bounds/hysteresis; no reparenting in Area signals.

Frame ownership, real seamless boarding, what happens when a ship disconnects or
jumps between planets under passengers, and whether interpolation delay feels
acceptable while standing on a foreign ship remain design/playtest questions.
`B` and seat return are marked debug placements, not a proposed boarding mechanic.

## 7. Plain Godot versus netfox

Own unreliable-channel RPCs with fixed validated bytes, a small timestamp-ordered
Hermite/Slerp buffer, clock ping/pong on a separate channel, host relay and shared
frame conversion are enough for the measured uncoupled paths. No lockstep,
MultiplayerSynchronizer raw-local-property sync, rollback, addon or prediction.
**Netfox was not added or benchmarked**, because ordinary interpolation met the
fixture bound. It would not by itself decide who owns the contact conflict.

Default SceneMultiplayer peer relay is disabled: the spike already relays its
own snapshots. This also removed ENet errors during simultaneous eight-client
shutdown. Unknown-owner payloads, invalid size/version, nonfinite values and
invalid parent ids are rejected; queues/history/stat arrays are bounded. No
objects/code are deserialized and the optional control bridge is localhost only.

## Delivery and manual acceptance

Source scene: `spikes/network/main.tscn`, launch it explicitly from the unchanged
root project. Portable `dist/exo-spike4-source.zip` has a separate minimal project
with this scene as its default. Godot 4.7.2 is sufficient. No binary was built:
there are no installed export templates, and source was expressly sufficient.

Host listens on `0.0.0.0` by default, configurable `--bind`; client uses
`--connect=HOST_IP`; **17440/UDP** by default, configurable `--port` on both ends.
The automated LAN-address test binds all interfaces and connects through this
machine's private non-loopback IP; it remains a **one-computer test**.
`README.md` contains the two-computer commands, firewall/UDP requirement, flight,
foreign cabin, independent shift and reconnect acceptance steps. Real addresses
are not copied to reports or logs. Reconnection gets the same owner slot after
departure. A newer shared timestamp with a restarted sequence resets its history;
late packets from the former lifetime are discarded. The live same-slot rejoin
before expiry passes (`results/reconnect.json`). Stale representations expire
after 2 s without snapshots.

Manual: two physical computers, smoothness by eye, own control feel, contacts and
passenger feel. No production design decisions, commits, pushes or PRs are made.

Method references: [Godot ENet peer](https://docs.godotengine.org/en/stable/classes/class_enetmultiplayerpeer.html),
[ENet connection counters](https://docs.godotengine.org/en/stable/classes/class_enetconnection.html),
[packet throttle](https://docs.godotengine.org/en/stable/classes/class_enetpacketpeer.html),
[SceneMultiplayer relay](https://docs.godotengine.org/en/stable/classes/class_scenemultiplayer.html).
