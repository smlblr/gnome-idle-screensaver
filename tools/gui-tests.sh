#!/usr/bin/env bash
# Runs the tests that need a graphical session (the ignored ones). Each runs in its own
# process because GTK can only be initialised once per process, and libtest gives every
# test its own thread. Needs a Wayland or X11 session and a GPU; CI does not run these.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."

cargo test --release --no-run 2>&1 | tail -1
bin=$(ls -t target/release/deps/gnome_idle_screensaver-* | grep -v '\.d$' | head -1)
for group in stage::tests settings::tests; do
    echo "== $group"
    timeout 120 "$bin" --include-ignored --test-threads=1 "$group" | grep -E '^test |test result'
done
