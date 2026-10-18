#!/usr/bin/env bash
# Ephemeral hosted-runner keyring; never use the developer's desktop vault.
set -euo pipefail
export XDG_DATA_HOME
XDG_DATA_HOME=$(mktemp -d "${RUNNER_TEMP:-/tmp}/sherka-ci-keyring.XXXXXX")
dbus-run-session -- bash -euo pipefail -c '
  printf "%s" "" | gnome-keyring-daemon --unlock --components=secrets --daemonize
  # These lifecycle fixtures read absent credentials. Creating a persistent
  # login collection would require a GUI prompt on a headless runner.
  code=0
  secret-tool lookup service fr.sherka.compagnon target ci-probe 2> "$XDG_DATA_HOME/lookup-error" || code=$?
  test "$code" = 1
  test ! -s "$XDG_DATA_HOME/lookup-error"
  cargo test --workspace --locked
'
