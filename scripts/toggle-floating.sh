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

  # Snapshot the tab's panes (geometry + visible ANSI screen) so the floating
  # box can paint the real workspace dimmed behind it instead of a dead fill
  # (see src/snapshot.rs). Best-effort: on any failure snap stays empty and
  # the app falls back to the plain backdrop.
  local snap="" layout
  layout="$("$herdr" pane layout ${target:+--pane "$target"} 2>/dev/null)"
  if [ -n "$layout" ]; then
    snap="${TMPDIR:-/tmp}/herdr-floax-snap-${ws//[^A-Za-z0-9_-]/_}.txt"
    {
      printf '%s' "$layout" | jq -r '.result.layout.area | "AREA \(.x) \(.y) \(.width) \(.height)"'
      printf '%s' "$layout" \
        | jq -r '.result.layout.panes[] | "\(.pane_id) \(.rect.x) \(.rect.y) \(.rect.width) \(.rect.height)"' \
        | while read -r pid x y w h; do
            echo "PANE $x $y $w $h"
            "$herdr" pane read "$pid" --source visible --format ansi 2>/dev/null
            echo "FLOAX_END_PANE"
          done
    } > "$snap" 2>/dev/null || snap=""
  fi

  # `tab` — a real, persistent tab holding just the plugin pane. herdr draws
  # single-pane tabs borderless over the full tab area, so the floating box
  # and its snapshot backdrop line up 1:1 with where the captured panes were
  # (a `split` + `pane zoom` gets wrapped in a 1-cell pane border that crops
  # and offsets the backdrop by one row/column). NOT `overlay`/`zoomed`:
  # those are transient views herdr tears down the instant the creating
  # keybinding action completes (the pane flashes then vanishes).
  #
  # The starting directory is passed via --env HERDR_FLOAX_CWD, NOT herdr's
  # --cwd flag: in herdr 0.7.1 `plugin pane open --cwd <path>` makes the pane
  # exit immediately. floating-shell.sh cd's there instead.
  set -- plugin pane open --plugin herdr-floax --entrypoint floating \
      --placement tab --env HERDR_FLOAX=1 --focus
  [ -n "$cwd" ] && set -- "$@" --env "HERDR_FLOAX_CWD=$cwd"
  [ -n "$snap" ] && set -- "$@" --env "HERDR_FLOAX_SNAPSHOT=$snap"
  exec "$herdr" "$@" >/dev/null 2>&1
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
  # Currently shown → dismiss (closing the tab's only pane closes the tab).
  # floating-shell.sh keeps the session alive (detach multiplexer) so the
  # next open re-attaches.
  exec "$herdr" plugin pane close "$pid"
else
  # Exists but you focused away → close the stale pane and reopen here: the
  # persistent shell re-attaches, and the snapshot backdrop is recaptured
  # from the tab you are actually looking at now.
  "$herdr" plugin pane close "$pid" >/dev/null 2>&1
  open_pane
fi
