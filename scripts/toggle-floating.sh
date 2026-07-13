#!/usr/bin/env bash
# Toggle the floating scratch pane for the CURRENT workspace.
#
# "Launch-or-reveal, dismiss on repeat", scoped per workspace (mirrors the
# bundled herdr-file-viewer's launcher pattern). The floating pane's id is
# remembered in a per-workspace state file (its pane/tab labels are renamed
# to the launch directory's basename, so a fixed label can't identify it):
#
#   - no floating pane in this workspace        -> OPEN a new one
#   - a floating pane exists but isn't focused  -> close it, reopen here
#   - the floating pane IS the focused pane     -> DISMISS it (close)
#
# herdr injects $HERDR_WORKSPACE_ID / $HERDR_PANE_ID / $HERDR_BIN_PATH into this
# action command. Any parse/edge failure degrades to OPEN — never a silent
# no-op. Persistence across a DISMISS is provided by scripts/floating-shell.sh
# (a detach session when a multiplexer is installed).
set -uo pipefail

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

# Where we remember the floating pane's id for this workspace.
state_dir="${HERDR_PLUGIN_STATE_DIR:-${TMPDIR:-/tmp}}"
pidfile="$state_dir/floax-pane-${ws//[^A-Za-z0-9_-]/_}"

open_pane() {
  # Which pane are we launching from? Needed for the snapshot and to
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
  # (see src/snapshot.rs). Captured in the BACKGROUND so opening never waits
  # on it: the app polls for the file and repaints when it lands (the panes'
  # contents don't change while the popup covers them, so capturing after the
  # open is equivalent). The old file is truncated first so a stale capture
  # is never shown, and the new one is moved into place atomically.
  # Best-effort: on any failure the app falls back to the plain backdrop.
  local snap="" layout
  layout="$("$herdr" pane layout ${target:+--pane "$target"} 2>/dev/null)"
  if [ -n "$layout" ]; then
    snap="${TMPDIR:-/tmp}/herdr-floax-snap-${ws//[^A-Za-z0-9_-]/_}.txt"
    : > "$snap" 2>/dev/null || snap=""
  fi
  if [ -n "$snap" ]; then
    (
      {
        printf '%s' "$layout" | jq -r '.result.layout.area | "AREA \(.x) \(.y) \(.width) \(.height)"'
        printf '%s' "$layout" \
          | jq -r '.result.layout.panes[] | "\(.pane_id) \(.rect.x) \(.rect.y) \(.rect.width) \(.rect.height)"' \
          | while read -r pid x y w h; do
              echo "PANE $x $y $w $h"
              "$herdr" pane read "$pid" --source visible --format ansi 2>/dev/null
              echo "FLOAX_END_PANE"
            done
      } > "$snap.tmp" 2>/dev/null && command mv -f "$snap.tmp" "$snap"
    ) >/dev/null 2>&1 &
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

  local out pid tab
  out="$("$herdr" "$@" 2>/dev/null)"
  pid="$(printf '%s' "$out" | jq -r '.result.plugin_pane.pane.pane_id // empty')"
  tab="$(printf '%s' "$out" | jq -r '.result.plugin_pane.pane.tab_id // empty')"

  # Name the pane and its tab "floax" so the popup is recognizable in the
  # tab bar, pickers, and pane lists.
  if [ -n "$pid" ]; then
    "$herdr" pane rename "$pid" "floax" >/dev/null 2>&1
    echo "$pid" > "$pidfile" 2>/dev/null || true
  fi
  [ -n "$tab" ] && "$herdr" tab rename "$tab" "floax" >/dev/null 2>&1
  exit 0
}

# Is the remembered floating pane still alive? "<focused> <pane_id>", or empty.
found=""
if [ -s "$pidfile" ]; then
  remembered="$(command cat "$pidfile" 2>/dev/null)"
  if [ -n "$remembered" ]; then
    found="$("$herdr" pane get "$remembered" 2>/dev/null \
      | jq -r '.result.pane | select(.pane_id != null) | "\(.focused) \(.pane_id)"' 2>/dev/null)"
  fi
fi

# No floating pane here → open one.
[ -z "$found" ] && open_pane

focused="${found%% *}"
pid="${found#* }"

if [ "$focused" = "true" ]; then
  # Currently shown → dismiss (closing the tab's only pane closes the tab).
  # floating-shell.sh keeps the session alive (detach multiplexer) so the
  # next open re-attaches.
  : > "$pidfile" 2>/dev/null || true
  exec "$herdr" plugin pane close "$pid"
else
  # Exists but you focused away → close the stale pane and reopen here: the
  # persistent shell re-attaches, and the snapshot backdrop is recaptured
  # from the tab you are actually looking at now.
  "$herdr" plugin pane close "$pid" >/dev/null 2>&1
  open_pane
fi
