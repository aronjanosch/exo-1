# Assisted-flight spike review

Summary: one ship gains assisted velocity goals and a terrain-clearance speed
curve. Neutral movement input brakes; H preserves the original unassisted
comparison. Horizon transport and force allocation support the small planet.

Risk class: yellow (spike gameplay and diagnostics).
Vision fit: fits the accepted arcade-flight experiment; values remain provisional.

Review findings addressed:

- A large velocity error initially consumed all the thrust budget, starving
  curved-path acceleration and causing an unintended climb. Reserve support
  thrust before velocity correction. Fixture now stays within 2 m of its initial
  2000 m clearance during a 25 s acceleration/cruise run.
- Smooth player rotation independently of horizon transport so the frame does
  not lag behind changing local up.
- Existing vertical-thrust bot phase timeouts were too short at the new 15 m/s
  vertical target. Extend them; full regression now reaches 2000 m before the
  vacuum glide check. Precision fly-out explicitly uses unassisted thrust.
- Headless bot runs skip screenshots instead of requesting unavailable images.

Validation:

- `flight_test.gd`: actual Jolt rigid-body integration, scripted movement input,
  18 checks covering low/mid/high speed, release braking, boost release, turning,
  diagonal speed, descent, terrain clearance and origin shift. Exit 0.
- Full headless `--auto-test --origin-shift=1000`: walked/boarded, reached 2000 m,
  glided, descended, cruised, landed, exited/reentered and walked inside the moving
  ship. Zero terrain rescues; no walker ejection; eight origin shifts with no
  reported speed discontinuity. Fixed-FPS frame-time output is not a benchmark.
- `git diff --check`: clean.
- Rendered `--auto-test --start-at=cruise`: low cruise remained at least 84 m
  above terrain; landing finished 0.03 m above it, zero rescues. Screenshot
  checked for the assist/goal/limit overlay. This overlapped the headless run
  initially, so it is a visual/behavior regression, not an isolated benchmark.

Limits: three terrain preview samples do not guarantee collision avoidance.
No sophisticated aerodynamics, multiplayer change or native extension added.
Production MCP integration is still absent from this prototype; the existing
scripted-input CLI remains its bot entry point. No performance guarantee for
other hardware. One accelerated fixture run emitted a Jolt job-capacity warning;
checks completed successfully and the full terrain regression did not emit it.

Existing initiator edits to main.gd, AGENTS.md and the README introduction are
outside the flight commit. No red-class paths changed by this implementation.
This review labels findings and fit; it does not approve a PR.
