#!/usr/bin/env bash
# The floating pane's process: a normal interactive login shell.
#
# floax's defining behavior is a session that survives toggling. The toggle
# hides the pane by closing it, so to preserve state across open/close we run
# the shell inside a per-workspace detached session (dtach/abduco/tmux) and
# re-attach on every open. No multiplexer installed? Degrade to a plain login
# shell — still fully usable, just fresh each time it's reopened.
#
# NOTE: the starting directory arrives via $HERDR_FLOAX_CWD, not herdr's --cwd
# flag. In herdr 0.7.1, `plugin pane open --cwd <path>` makes the new plugin
# pane exit immediately (it vanishes), so we set the directory here instead.
set -u

shell="${SHELL:-/bin/sh}"
ws="${HERDR_WORKSPACE_ID:-default}"
state_dir="${HERDR_PLUGIN_STATE_DIR:-${TMPDIR:-/tmp}}"

cd "${HERDR_FLOAX_CWD:-$HOME}" 2>/dev/null || cd "$HOME" 2>/dev/null || true

# The embedded vt100 supports truecolor; advertise it so shell prompts emit
# exact RGB (a 256-color approximation of the terminal background reads as
# an off-color stripe inside the box).
export COLORTERM=truecolor

# dtach: attach-or-create (-A); -z disables the suspend key.
if command -v dtach >/dev/null 2>&1; then
  exec dtach -A "$state_dir/floax-$ws.dtach" -z "$shell" -l
# abduco: -A attach-or-create a session named per workspace.
elif command -v abduco >/dev/null 2>&1; then
  exec abduco -A "floax-$ws" "$shell" -l
# tmux: new-session-or-attach to a per-workspace session in its own server,
# with a minimal dedicated config (truecolor passthrough, no status bar) —
# see floax-tmux.conf. The set-options keep an already-running server from a
# previous floax version consistent too (existing shells keep their old
# TERM until they exit, but chrome and passthrough update immediately).
elif command -v tmux >/dev/null 2>&1; then
  tmux -L "herdr-floax" set -g status off >/dev/null 2>&1
  tmux -L "herdr-floax" set -ga terminal-overrides ",xterm-256color:RGB" >/dev/null 2>&1
  exec tmux -L "herdr-floax" -f "${HERDR_PLUGIN_ROOT:-$(dirname "$0")/..}/scripts/floax-tmux.conf" \
    new-session -A -s "$ws" "$shell -l"
fi

exec "$shell" -l
