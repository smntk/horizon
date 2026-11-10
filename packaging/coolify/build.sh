#!/bin/sh
# Builds the Horizon dedicated server with the mods in server-mods/ (packaging/coolify/Dockerfile runs it).
#
#   server-mods/native/<mod>/horizon-mixin.toml   native Rust mods: applied with horizon-mixins, compiled in
#   server-mods/content/<mod>/horizon-mod.toml    content mods from source: built into .hmod files
#   server-mods/content/*.hmod                    content mods already built
#
# Output: /out/horizon-engine, /out/horizon-server, /out/content/*.hmod (+ content/ids.txt), /out/native-mods.txt
set -eu

PROFILE=${PROFILE:-release}
MIXINS_GIT=${MIXINS_GIT:-https://github.com/horizon-rust/horizon-mixins}
MIXINS_REV=${MIXINS_REV:-5a64af938ec86b00d53d16a408aabc0038fb3e57}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/target}

say() { printf '\n==> %s\n' "$*"; }

mkdir -p /out/content
: > /out/content/ids.txt
: > /out/native-mods.txt
SRC=/src

# --- Native mods -------------------------------------------------------------------------------------
bundles=$(find /src/server-mods/native -mindepth 2 -maxdepth 2 -name horizon-mixin.toml 2>/dev/null | sort || true)
if [ -n "$bundles" ]; then
    say "Native mods found:"
    rm -rf /mods /stage
    mkdir -p /mods
    for manifest in $bundles; do
        dir=$(dirname "$manifest")
        echo "  - $(basename "$dir")"
        basename "$dir" >> /out/native-mods.txt
        cp -r "$dir" /mods/
    done

    tool="$CARGO_TARGET_DIR/tools/$MIXINS_REV/bin/horizon-mixins"
    if [ ! -x "$tool" ]; then
        say "Building horizon-mixins $MIXINS_REV (once, then cached)"
        cargo install --locked --git "$MIXINS_GIT" --rev "$MIXINS_REV" \
            --root "$CARGO_TARGET_DIR/tools/$MIXINS_REV" horizon-mixins-cli
    fi

    # Bundles pinned to a Horizon commit are checked against it.
    if [ -n "${HORIZON_REVISION:-}" ]; then
        printf '%s\n' "$HORIZON_REVISION" > /src/.horizon-revision
    elif [ -d /src/.git ] && command -v git >/dev/null 2>&1; then
        git -C /src rev-parse HEAD > /src/.horizon-revision 2>/dev/null || true
    fi

    say "Applying the native mods to the game source"
    "$tool" apply --workspace /src --mods /mods --out /stage
    SRC=/stage
fi

# --- Stable timestamps for cargo -------------------------------------------------------------------------
# Cargo rebuilds a crate when one of its files is newer than its last build. Every Coolify deployment is a
# fresh checkout, so every file looks new and all Horizon crates recompile even when nothing changed. Build
# from a mirror kept in a cache instead: rsync without -t copies only the files whose content changed (they
# get the current time), the others keep the time of the build that last saw them.
if [ -d /srcmirror ] && command -v rsync >/dev/null 2>&1; then
    say "Syncing the sources into the build mirror (changed files only)"
    rsync -rlp --checksum --delete "$SRC/" /srcmirror/
    SRC=/srcmirror
fi

# --- The game (server build) and the supervisor ---------------------------------------------------------
say "Building horizon-engine and horizon-server ($PROFILE)"
cd "$SRC"
cargo build --locked --profile "$PROFILE" -p horizon-client --no-default-features --bin horizon-client
cargo build --locked --profile "$PROFILE" -p horizon-server --bin horizon-server
cp "$CARGO_TARGET_DIR/$PROFILE/horizon-client" /out/horizon-engine
cp "$CARGO_TARGET_DIR/$PROFILE/horizon-server" /out/horizon-server

# --- Content mods --------------------------------------------------------------------------------------
sources=$(find /src/server-mods/content -mindepth 2 -maxdepth 2 -name horizon-mod.toml 2>/dev/null | sort || true)
if [ -n "$sources" ]; then
    say "Building cargo-horizon-mod"
    (cd /src && cargo build --locked --profile "$PROFILE" -p cargo-horizon-mod)
    hmod_tool="$CARGO_TARGET_DIR/$PROFILE/cargo-horizon-mod"
    for manifest in $sources; do
        dir=$(dirname "$manifest")
        say "Content mod $(basename "$dir")"
        (cd "$dir" && env -u CARGO_TARGET_DIR "$hmod_tool" horizon-mod build)
        cp "$dir"/target/horizon-mod/*.hmod /out/content/
    done
fi
for f in /src/server-mods/content/*.hmod; do
    [ -f "$f" ] && cp "$f" /out/content/
done

# The ids, so the start can turn each mod on (horizon-mods.txt: mod.<id>:true).
for f in /out/content/*.hmod; do
    [ -f "$f" ] || continue
    id=$(unzip -p "$f" horizon-mod.toml | sed -n 's/^id *= *"\([a-z0-9_]*\)".*/\1/p' | head -n 1)
    [ -n "$id" ] || { echo "Error: no id in $f's horizon-mod.toml" >&2; exit 1; }
    echo "$id" >> /out/content/ids.txt
    echo "  content mod: $id ($(basename "$f"))"
done

say "Done"
