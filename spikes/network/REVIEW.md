# Spike 4 self-review

Summary: isolated client-authority ENet fixture, shared coordinate snapshots,
Hermite/Slerp playback, fault injection, foreign cabin, narrow MCP control,
real/recorded measurements and portable LAN source delivery.

Risk class: **red — networking**, explicitly authorized by the initiator.
Vision fit: **fits with changes as the requested experiment**; VISION still
states host authority. No production authority/contact/player-count decisions
are made and no public proposal or PR is filed.

Verified from code and runs:

- Existing controller files and root `project.godot`/CI/autoload/addon paths
  remain unchanged. The portable archive's separate project has its own main scene.
- Fixed-size/version data decode, finite/range/quaternion/parent checks, assigned
  sender ownership, fixed MCP actions and argument validation. No network code or
  object decoding; Godot file writes stay under `user://`.
- Snapshot/impairment/input/control/stat histories have limits. Coordinates stay
  shared until playback. Dynamic origin shifts use the current physics-server pose.
- Duplicate/reordered snapshots, frame changes, source sequence restart and old
  lifetime packets are handled. Actual same-slot reconnect is tested.
- Native ENet peer relay is off; own explicit snapshot relay has a separate channel.
  Simulated loss is separated from ENet's adaptive throttle for this LAN fixture.
- Headless real-time viewers cap display steps at 60 Hz; simulated send rates remain
  separate. Rendering screenshots and a headless matrix use different runs.
- 96 configurations, 25 checks, 2/8-process ENet, foreign cabin through MCP,
  same-link host reference, independent planets/shifts, private LAN-address bind,
  reconnect, syntax and portable-source smoke checks form the acceptance evidence.

Known limits requiring design/manual assessment, already exposed in the report:

- Independent ship contacts disagree; no global impulse/contact owner is selected.
- No extrapolation, long-burst recovery, adaptive WAN playout, predictive host
  controller or production congestion control. Eight-instance CPU figures are local
  observations; short native holds remain measured rather than suppressed.
- Test boarding/seat return are placements. Cross-planet travel and passenger
  planet changes are outside this static-frame fixture. No real two-computer LAN
  run is claimed; that is the requested documented manual finishing test.
- No export templates are installed, so the deliverable is the allowed source
  package, not a binary.

AI review label suggestion: `spike`, `risk:red`, `client-authority-experiment`.
This is a review record, **not PR approval**.
