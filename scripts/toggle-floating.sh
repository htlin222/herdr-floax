#!/usr/bin/env bash
# Toggle the floating scratch pane for the CURRENT workspace.
#
# "Launch-or-reveal, dismiss on repeat", scoped per workspace (mirrors the
# bundled herdr-file-viewer's launcher pattern). The floating pane is a split
# that we zoom (maximize) for the fullscreen floating look, found by its label
# ("⌂ floax") within this workspace:
#
#   - no floating pane in this workspace       -> OPEN a new one, maximized
#   - a floating pane exists but isn't focused  -> REVEAL it (focus + maximize)
#   - the floating pane IS the focused pane      -> DISMISS it (close)
#
# herdr injects $HERDR_WORKSPACE_ID / $HERDR_PANE_ID / $HERDR_BIN_PATH into this
# action command. Any parse/edge failure degrades to OPEN — never a silent
# no-op. Persistence across a DISMISS is provided by scripts/floating-shell.sh
# (a detach session when a multiplexer is installed).
set -uo pipefail

LABEL="⌂ floax"
herdr="${HERDR_BIN_PATH:-herdr}"

# jq is required to parse the pane-list JSON. Fail loudly with a fix hint.
if ! command -v jq >/dev/null 2>&1; then
  "$herdr" notification show "herdr-floax needs 'jq' installed" >/dev/null 2>&1 || \
    echo "herdr-floax: 'jq' is required (brew install jq / apt install jq)" >&2
  exit 1
fi

# Which workspace are we in? Prefer the injected env; fall back to pane.current.
ws="${HERDR_WORKSPACE_ID:-}"
if [ -z "$ws" ]; then
  ws="$("$herdr" pane current 2>/dev/null | jq -r '.result.pane.workspace_id // empty')"
fi

open_pane() {
  # Which pane are we launching from? Needed as the split's target and to
  # inherit its cwd. Prefer the injected id; fall back to the focused pane.
  local target="${HERDR_PANE_ID:-}"
  [ -z "$target" ] && target="$("$herdr" pane current 2>/dev/null | jq -r '.result.pane.pane_id // empty')"

  local cwd=""
  if [ -n "$target" ]; then
    cwd="$("$herdr" pane get "$target" 2>/dev/null | jq -r '.result.pane.cwd // empty')"
  fi
  [ -z "$cwd" ] && cwd="$("$herdr" pane current 2>/dev/null | jq -r '.result.pane.cwd // empty')"

  # `split`, then zoom it — NOT `overlay`/`zoomed`. An overlay/zoomed placement
  # is a transient view herdr tears down the instant the creating keybinding
  # action completes (the pane flashes then vanishes). A split is a real,
  # persistent layout pane; `pane zoom --on` then maximizes it for the floating,
  # fullscreen look. Closing it later restores the original layout.
  #
  # The starting directory is passed via --env HERDR_FLOAX_CWD, NOT herdr's
  # --cwd flag: in herdr 0.7.1 `plugin pane open --cwd <path>` makes the pane
  # exit immediately. floating-shell.sh cd's there instead.
  set -- plugin pane open --plugin herdr-floax --entrypoint floating \
      --placement split --direction right --env HERDR_FLOAX=1 --focus
  [ -n "$target" ] && set -- "$@" --target-pane "$target"
  [ -n "$cwd" ] && set -- "$@" --env "HERDR_FLOAX_CWD=$cwd"
  local out pid
  out="$("$herdr" "$@" 2>/dev/null)"
  pid="$(printf '%s' "$out" | jq -r '.result.plugin_pane.pane.pane_id // empty')"
  [ -n "$pid" ] && "$herdr" pane zoom "$pid" --on >/dev/null 2>&1
  exit 0
}

# Find our floating pane in this workspace: "<focused> <pane_id>", or empty.
found=""
if [ -n "$ws" ]; then
  found="$("$herdr" pane list --workspace "$ws" 2>/dev/null \
    | jq -r --arg L "$LABEL" '
        .result.panes[]? | select(.label == $L)
        | "\(.focused) \(.pane_id)"' 2>/dev/null | head -n1)"
fi

# No floating pane here → open one.
[ -z "$found" ] && open_pane

focused="${found%% *}"
pid="${found#* }"

if [ "$focused" = "true" ]; then
  # Currently shown (focused + maximized) → dismiss. floating-shell.sh keeps the
  # session alive (detach multiplexer) so the next open re-attaches.
  exec "$herdr" plugin pane close "$pid"
else
  # Exists but you focused away (so it un-maximized) → reveal: `pane zoom --on`
  # both focuses AND re-maximizes it, restoring the floating look in one step.
  exec "$herdr" pane zoom "$pid" --on
fi
