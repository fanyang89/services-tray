#!/usr/bin/env bash
# Dev loop for the backend: rebuild on Rust changes and restart the manager
# gracefully (SIGTERM), so managed services are stopped instead of orphaned.
#
# `cargo watch` works too (it sends SIGTERM to the process group by default,
# see task dev:backend:watch); this script additionally needs no install and
# restarts the manager automatically if it crashes.
#
# Usage: scripts/dev-backend.sh [manager args...]
#   e.g. scripts/dev-backend.sh --port 8765
set -u
cd "$(dirname "$0")/.."

# ~/.cargo/bin fallback for environments where rustup's PATH is not set up
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

export SM_SKIP_WEB_BUILD="${SM_SKIP_WEB_BUILD:-1}"
PKG=services-manager-server
BIN=target/debug/services-manager
WATCH_DIRS=(crates Cargo.toml)

say() { printf '\033[36m[dev-backend]\033[0m %s\n' "$*"; }

manager_pid=
stamp=$(mktemp)

stop_manager() {
  [ -n "$manager_pid" ] || return 0
  # SIGTERM: graceful path — the manager stops all services, then exits.
  kill -TERM "$manager_pid" 2>/dev/null || return 0
  # up to 10s for the graceful stop, then force
  for _ in $(seq 1 40); do
    kill -0 "$manager_pid" 2>/dev/null || break
    sleep 0.25
  done
  kill -9 "$manager_pid" 2>/dev/null || true
  wait "$manager_pid" 2>/dev/null || true
  manager_pid=
}

cleanup() {
  stop_manager
  rm -f "$stamp"
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

# First changed source file since the baseline; empty when unchanged.
# Portable polling (works on macOS too), ~0.5s latency.
changed_since_last_check() {
  find "${WATCH_DIRS[@]}" -type f \
    \( -name '*.rs' -o -name '*.toml' \) -newer "$stamp" -print -quit 2>/dev/null
}

first=1
while :; do
  if [ "$first" = 1 ]; then
    say "initial build"
    cargo build -p "$PKG" || say "build failed; waiting for changes"
    first=0
  fi

  if [ -z "$manager_pid" ] && [ -x "$BIN" ]; then
    say "starting: $BIN $*"
    "$BIN" "$@" &
    manager_pid=$!
  fi

  changed=
  while :; do
    changed=$(changed_since_last_check)
    if [ -n "$changed" ]; then
      break
    fi
    # restart automatically if the manager exited (crash, port clash, ...)
    if [ -n "$manager_pid" ] && ! kill -0 "$manager_pid" 2>/dev/null; then
      wait "$manager_pid" 2>/dev/null || true
      manager_pid=
      say "manager exited; restarting in 1s"
      sleep 1
      break
    fi
    sleep 0.5
  done

  # crash-restart path: loop back and start the manager again
  [ -n "$changed" ] || continue

  say "change: $changed"
  stop_manager
  say "rebuilding"
  # baseline reset AFTER the rebuild so edits made while compiling are caught
  if cargo build -p "$PKG"; then
    touch "$stamp"
  else
    say "build failed; waiting for next change"
  fi
done
