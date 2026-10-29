#!/bin/bash
# Distant Horizons for Horizon Client: the installer in the release zip.
#
# A native mod is compiled into the game, so it cannot simply be dropped into a folder: this script does every
# step instead. It finds the Horizon launcher, installs and prepares the Horizon Nightly the mod is pinned to (if
# needed), gathers this mod with the native mods already turned on in your "Native mods" instance, asks you to
# confirm, and has the launcher build them. Then play the "Native mods" instance.
#
# macOS: double-click install.command (the first time: right-click it > Open). Linux: run ./install.sh.
# It changes nothing until you type "yes". LGPL-3.0, part of Distant Horizons for Horizon Client.
set -euo pipefail

NIGHTLY="nightly-2026-10-29-d009ebd"
HERE="$(cd "$(dirname "$0")" && pwd)"

say() { printf '%s\n' "$*"; }
fail() {
    say ""
    say "Error: $*"
    say "Nothing was built. The manual steps are in README.md (Install)."
    [ -t 0 ] && read -r -p "Press Enter to close." _
    exit 1
}

say "Distant Horizons for Horizon Client: installer"
say ""

# The mod file beside this script.
HNMOD="$(ls "$HERE"/distant_horizons-*.hnmod 2>/dev/null | head -1 || true)"
[ -n "$HNMOD" ] || fail "no distant_horizons-*.hnmod beside this script: unzip the whole release first."
command -v unzip >/dev/null || fail "the 'unzip' command is missing."

# The launcher and its folders (the ones the launcher itself uses, unless overridden).
case "$(uname -s)" in
    Darwin)
        DATA="$HOME/Library/Application Support"
        CANDIDATES=("/Applications/Horizon.app/Contents/MacOS/horizon-launcher"
                    "$HOME/Applications/Horizon.app/Contents/MacOS/horizon-launcher")
        ;;
    Linux)
        DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
        CANDIDATES=("$(command -v horizon-launcher || true)" "$HOME/.local/bin/horizon-launcher")
        ;;
    *) fail "this installer runs on macOS and Linux; on Windows follow README.md (Install)." ;;
esac
LAUNCHER=""
for c in "${CANDIDATES[@]}"; do
    if [ -n "$c" ] && [ -x "$c" ]; then LAUNCHER="$c"; break; fi
done
[ -n "$LAUNCHER" ] || fail "the Horizon launcher was not found. Install it from https://playhorizon.dev first."
GAME_HOME="${HORIZON_HOME:-$DATA/horizon}"
LAUNCHER_HOME="${HORIZON_LAUNCHER_HOME:-$DATA/horizon-launcher}"
say "Launcher: $LAUNCHER"

# The Horizon the mod is pinned to: installed, then prepared from your own Minecraft (or Mojang, for your account).
VERSION_DIR="$LAUNCHER_HOME/versions/$NIGHTLY"
if [ ! -d "$VERSION_DIR" ]; then
    say "Installing Horizon $NIGHTLY (the version this mod is built for)..."
    "$LAUNCHER" install "$NIGHTLY" || fail "could not install $NIGHTLY."
fi
if [ ! -f "$VERSION_DIR/.local/horizon-client/26.3/horizon-resources.json" ]; then
    say "Preparing $NIGHTLY's resources from your Minecraft 26.3..."
    "$LAUNCHER" prepare --version "$NIGHTLY" || fail "could not prepare $NIGHTLY (sign in to the launcher first)."
fi

# The set to build: this mod, and the native mods turned on in the "Native mods" instance, so a rebuild keeps them.
SET="$(mktemp -d)/native-set"
mkdir -p "$SET/distant_horizons"
unzip -q "$HNMOD" -d "$SET/distant_horizons"
MODS=("Distant Horizons")
INSTANCE="$GAME_HOME/instances/native-mods"
if [ -d "$INSTANCE/horizon-native-mods" ]; then
    for f in "$INSTANCE"/horizon-native-mods/*.hnmod; do
        [ -e "$f" ] || continue
        id="$(unzip -p "$f" horizon-native.json | sed -n 's/.*"id": *"\([^"]*\)".*/\1/p' | head -1)"
        [ -n "$id" ] && [ "$id" != "distant_horizons" ] || continue
        grep -qx "mod\.$id:true" "$INSTANCE/horizon-mods.txt" 2>/dev/null || continue
        mkdir -p "$SET/$id"
        unzip -q "$f" -d "$SET/$id"
        MODS+=("$id")
    done
fi

say ""
say "This will build Horizon $NIGHTLY with these native mods:"
for m in "${MODS[@]}"; do say "  - $m"; done
say ""
say "Native mods are not sandboxed: their code runs with your rights on this computer."
say "Their source is in the release and on GitHub; read it if you have any doubt."
say "The first build downloads the launcher's own Rust and takes 10 to 30 minutes; later ones a few minutes."
say ""
read -r -p "Type yes to trust these mods and build: " answer
[ "$answer" = "yes" ] || { say "Nothing was built."; exit 0; }

"$LAUNCHER" native-build "$SET" --version "$NIGHTLY" --trust-native-code || fail "the build failed (its log is in the launcher's native/build.log)."

say ""
say "Done. In the Horizon launcher, play the \"Native mods\" instance."
say "In a world, /dh in chat shows Distant Horizons' status; far terrain fills in over the first minutes."
say "WebAssembly mods (.hmod) keep working beside it."
[ -t 0 ] && read -r -p "Press Enter to close." _
exit 0
