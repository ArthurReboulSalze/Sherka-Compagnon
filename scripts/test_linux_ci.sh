#!/usr/bin/env bash
# Ephemeral hosted-runner keyring; never use the developer's desktop vault.
set -euo pipefail
export XDG_DATA_HOME
XDG_DATA_HOME=$(mktemp -d "${RUNNER_TEMP:-/tmp}/sherka-ci-keyring.XXXXXX")
dbus-run-session -- bash -euo pipefail -c '
  printf "%s" "" | gnome-keyring-daemon --unlock --components=secrets --daemonize
  printf "%s" "ci-fixture-value" | secret-tool store --label="Sherka CI probe" service fr.sherka.compagnon target ci-probe
  test "$(secret-tool lookup service fr.sherka.compagnon target ci-probe)" = ci-fixture-value
  secret-tool clear service fr.sherka.compagnon target ci-probe
  cargo test --workspace --locked
'
