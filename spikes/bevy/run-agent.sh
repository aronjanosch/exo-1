#!/usr/bin/env bash
# Run a windowed binary for coding agents without stealing focus (same idea as godot-agent):
# under Hyprland the window opens on workspace $AGENT_WORKSPACE (default 7), silent and
# unfocused. Output and exit code pass through. Outside Hyprland it just execs the command.
# Usage: run-agent.sh <binary> [args...]
set -euo pipefail
workspace="${AGENT_WORKSPACE:-7}"
if [[ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
  HYPRLAND_INSTANCE_SIGNATURE="$(ls -t /run/user/$(id -u)/hypr 2>/dev/null | head -1 || true)"
  export HYPRLAND_INSTANCE_SIGNATURE
fi
if [[ -z "$HYPRLAND_INSTANCE_SIGNATURE" ]] || ! command -v hyprctl >/dev/null; then
  exec "$@"
fi
run_dir="$(mktemp -d -t run-agent.XXXXXX)"
log="$run_dir/out.log"; rc_file="$run_dir/rc"; pid_file="$run_dir/pid"
: >"$log"
inner="$run_dir/run.sh"
{
  printf '#!/usr/bin/env bash\n'
  printf 'export PATH=%q\n' "$PATH"
  for v in RUST_LOG RUST_BACKTRACE EXO_DEBUG; do
    [[ -n "${!v:-}" ]] && printf 'export %s=%q\n' "$v" "${!v}"
  done
  printf 'cd %q\n' "$PWD"
  printf '%q' "$(command -v "$1" || realpath "$1")"
  shift
  printf ' %q' "$@"
  printf ' >>%q 2>&1 &\n' "$log"
  printf 'echo $! >%q\n' "$pid_file"
  printf 'wait $!; echo $? >%q\n' "$rc_file"
} >"$inner"
chmod +x "$inner"
cleanup() {
  [[ -f "$pid_file" ]] && kill "$(cat "$pid_file")" 2>/dev/null || true
  [[ -n "${tail_pid:-}" ]] && kill "$tail_pid" 2>/dev/null || true
  rm -rf "$run_dir"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
lua_cmd="$(printf '%q' "$inner")"
lua_cmd="${lua_cmd//\\/\\\\}"
lua_cmd="${lua_cmd//\"/\\\"}"
hyprctl dispatch "hl.dsp.exec_cmd(\"$lua_cmd\", { workspace = \"$workspace silent\", no_initial_focus = true })" >/dev/null
tail -n +1 -f "$log" &
tail_pid=$!
while [[ ! -s "$rc_file" ]]; do sleep 0.2; done
sleep 0.2
rc="$(cat "$rc_file")"
rm -f "$pid_file"
exit "$rc"
