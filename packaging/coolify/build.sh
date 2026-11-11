#!/bin/sh
# Builds the Horizon dedicated server with the mods in server-mods/ (packaging/coolify/Dockerfile runs it).
#
#   server-mods/native/<mod>/horizon-mixin.toml   native Rust mods, compiled in: injections and modules are
#                                                 applied with horizon-mixins, an [api] trait package is linked
#                                                 into a generated game executable (as the launcher does)
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
die() { printf 'Error: %s\n' "$*" >&2; exit 1; }

# The value of `key = "..."` (or '...') inside table [table] of a TOML file; empty when absent.
toml_get() { # file table key
    awk -v table="[$2]" -v key="$3" -v q="'" '
        { sub(/\r$/, "") }
        /^\[/ { inside = ($0 == table); next }
        inside && $0 ~ "^" key "[ \t]*=" {
            sub("^" key "[ \t]*=[ \t]*", "")
            sub("[ \t]*#.*$", "")
            gsub("[\"" q "]", "")
            print
            exit
        }' "$1"
}

# The manifest without its [api] and [dependencies] tables, which horizon-mixins doesn't know.
strip_api() { # manifest
    awk '{ sub(/\r$/, "") } /^\[/ { skip = ($0 == "[api]" || $0 == "[dependencies]") } !skip' "$1" > "$1.tmp" \
        && mv "$1.tmp" "$1"
}

# A bundle's [dependencies] (docs/native-mods.md, "Dependencies between trait mods"), one per line:
# "id<TAB>requirement<TAB>optional", from `id = "^1"` or `id = { version = "^2", optional = true }`.
toml_deps() { # manifest
    awk -v q="'" '
        { sub(/\r$/, ""); sub("[ \t]*#.*$", "") }
        /^\[/ { inside = ($0 == "[dependencies]"); next }
        !inside || $0 !~ /=/ { next }
        {
            id = $0; sub("[ \t]*=.*$", "", id); gsub("[\"" q " \t]", "", id)
            value = $0; sub("^[^=]*=[ \t]*", "", value)
            optional = (value ~ /optional[ \t]*=[ \t]*true/) ? "true" : "false"
            if (value ~ /^\{/) { if (!match(value, /version[ \t]*=[ \t]*["\047][^"\047]*/)) next
                value = substr(value, RSTART, RLENGTH); sub(/^version[ \t]*=[ \t]*./, "", value) }
            gsub("[\"" q " \t]", "", value)
            printf "%s\t%s\t%s\n", id, value, optional
        }' "$1"
}

# Whether version `1.4.2` meets Cargo requirement `^1.2` (also `1.2`, `~1.2`, `>=1.2`, `=1.2.3`, `*`).
semver_ok() { # version requirement
    req=$(printf '%s' "$2" | tr -d ' ')
    case "$req" in
        '*') return 0 ;;
        '>='*) op=ge; req=${req#>=} ;;
        '='*) op=eq; req=${req#=} ;;
        '^'*) op=caret; req=${req#^} ;;
        '~'*) op=tilde; req=${req#\~} ;;
        [0-9]*) op=caret ;;
        *) return 2 ;;
    esac
    # shellcheck disable=SC2046
    set -- $(printf '%s' "$1" | tr '.+-' '   ')
    v1=${1:-0}; v2=${2:-0}; v3=${3:-0}
    # shellcheck disable=SC2046
    set -- $(printf '%s' "$req" | tr '.' ' ')
    r1=${1:-0}; r2=${2:-0}; r3=${3:-0}; parts=$#
    at_least=1
    if [ "$v1" -lt "$r1" ] || { [ "$v1" -eq "$r1" ] && { [ "$v2" -lt "$r2" ] \
        || { [ "$v2" -eq "$r2" ] && [ "$v3" -lt "$r3" ]; }; }; }; then
        at_least=0
    fi
    case $op in
        ge) [ $at_least = 1 ] ;;
        eq) [ "$v1" -eq "$r1" ] && [ "$v2" -eq "$r2" ] && [ "$v3" -eq "$r3" ] ;;
        tilde) [ $at_least = 1 ] && [ "$v1" -eq "$r1" ] && { [ "$parts" -lt 2 ] || [ "$v2" -eq "$r2" ]; } ;;
        caret)
            [ $at_least = 1 ] || return 1
            if [ "$r1" -gt 0 ] || [ "$parts" -lt 2 ]; then [ "$v1" -eq "$r1" ]
            elif [ "$r2" -gt 0 ] || [ "$parts" -lt 3 ]; then [ "$v1" -eq 0 ] && [ "$v2" -eq "$r2" ]
            else [ "$v1" -eq 0 ] && [ "$v2" -eq 0 ] && [ "$v3" -eq "$r3" ]
            fi ;;
    esac
}

# Checks every trait mod's dependencies against the installed ones and writes the mods in dependency
# order (a mod after the mods it depends on, else in folder order), as the launcher orders dispatch.
order_trait_mods() { # list file, output file
    tab=$(printf '\t')
    while IFS="$tab" read -r id dir path; do
        toml_deps "$dir/horizon-mixin.toml" | while IFS="$tab" read -r dep req optional; do
            line=$(grep "^$dep$tab" "$1" || true)
            if [ -z "$line" ]; then
                [ "$optional" = true ] && continue
                die "$id needs the native mod $dep ($req) with an [api] package in server-mods/native"
            fi
            version=$(toml_get "$(printf '%s' "$line" | cut -f2)/horizon-mixin.toml" mod version)
            semver_ok "$version" "$req" \
                || die "$id needs $dep $req, server-mods/native has $dep $version"
        done || exit 1
    done < "$1"
    : > "$2"
    cp "$1" /tmp/trait-left.txt
    while [ -s /tmp/trait-left.txt ]; do
        : > /tmp/trait-next.txt
        placed=0
        while IFS="$tab" read -r id dir path; do
            waits=0
            for dep in $(toml_deps "$dir/horizon-mixin.toml" | cut -f1); do
                grep -q "^$dep$tab" /tmp/trait-left.txt && waits=1
            done
            if [ $waits = 0 ]; then
                printf '%s\t%s\t%s\n' "$id" "$dir" "$path" >> "$2"
                placed=1
            else
                printf '%s\t%s\t%s\n' "$id" "$dir" "$path" >> /tmp/trait-next.txt
            fi
        done < /tmp/trait-left.txt
        [ $placed = 1 ] || die "these native mods depend on each other in a cycle: $(cut -f1 /tmp/trait-next.txt | tr '\n' ' ')"
        mv /tmp/trait-next.txt /tmp/trait-left.txt
    done
}

# Trait mods (docs/native-mods.md, "Trait mods"): each [api] package is copied into the workspace with its
# horizon-client dependency pointed at this source, and a generated package, horizon-native-game, owns the
# `horizon-client` executable and hands every mod's hooks() to the game (launcher/src/native_mods/api.rs).
stage_trait_mods() { # list file of "id<TAB>bundle dir<TAB>api path", in build order
    game="$SRC/crates/native-game"
    rm -rf "$game" "$SRC/native-api"
    mkdir -p "$game/src" "$SRC/native-api"
    : > /tmp/trait-deps.toml
    : > /tmp/trait-members.txt
    : > /tmp/trait-entries.rs
    ids=""
    i=0
    while IFS="$(printf '\t')" read -r id dir path; do
        [ -n "$id" ] || continue
        dest="$SRC/native-api/$id"
        cp -r "$dir/$path" "$dest"
        rm -rf "$dest/target"
        manifest="$dest/Cargo.toml"
        grep -q '^horizon-client[ \t]*=' "$manifest" \
            || die "$id: $path/Cargo.toml needs a line 'horizon-client = ...' in [dependencies]"
        sed -i 's|^horizon-client[ \t]*=.*|horizon-client = { path = "../../crates/horizon-client", default-features = false }|' "$manifest"
        package=$(toml_get "$manifest" package name)
        [ -n "$package" ] || die "$id: $path/Cargo.toml has no [package] name"
        # Its dependencies on other trait mods: a path dependency named by the mod id, with the target's own
        # package name; an optional one also gets a feature of that name, on only when the mod is here.
        : > /tmp/trait-mod-deps.toml
        : > /tmp/trait-mod-features.toml
        enabled=""
        toml_deps "$dir/horizon-mixin.toml" > /tmp/trait-mod-deplist.txt
        while IFS="$(printf '\t')" read -r dep req optional; do
            line=$(grep "^$dep$(printf '\t')" "$1" || true)
            [ -n "$line" ] || continue
            dep_manifest="$(printf '%s' "$line" | cut -f2)/$(printf '%s' "$line" | cut -f3)/Cargo.toml"
            dep_package=$(toml_get "$dep_manifest" package name)
            if [ "$optional" = true ]; then
                printf '%s = { path = "../%s", package = "%s", optional = true }\n' "$dep" "$dep" "$dep_package" >> /tmp/trait-mod-deps.toml
                printf '%s = ["dep:%s"]\n' "$dep" "$dep" >> /tmp/trait-mod-features.toml
                enabled="$enabled\"$dep\", "
            else
                printf '%s = { path = "../%s", package = "%s" }\n' "$dep" "$dep" "$dep_package" >> /tmp/trait-mod-deps.toml
            fi
        done < /tmp/trait-mod-deplist.txt
        if [ -s /tmp/trait-mod-deps.toml ]; then
            sed -i '/^\[dependencies\][ \t]*$/r /tmp/trait-mod-deps.toml' "$manifest"
        fi
        if [ -s /tmp/trait-mod-features.toml ]; then
            if grep -q '^\[features\]' "$manifest"; then
                sed -i '/^\[features\][ \t]*$/r /tmp/trait-mod-features.toml' "$manifest"
            else
                { printf '\n[features]\n'; cat /tmp/trait-mod-features.toml; } >> "$manifest"
            fi
        fi
        if [ -n "$enabled" ]; then
            printf 'mod_%s = { path = "../../native-api/%s", package = "%s", features = [%s] }\n' "$i" "$id" "$package" "$enabled" >> /tmp/trait-deps.toml
        else
            printf 'mod_%s = { path = "../../native-api/%s", package = "%s" }\n' "$i" "$id" "$package" >> /tmp/trait-deps.toml
        fi
        printf '    "native-api/%s",\n' "$id" >> /tmp/trait-members.txt
        printf '        mod_%s::hooks(),\n' "$i" >> /tmp/trait-entries.rs
        ids="$ids\"$id\", "
        i=$((i + 1))
    done < "$1"

    {
        printf '[package]\nname = "horizon-native-game"\nversion = "0.1.0"\nedition = "2024"\npublish = false\n'
        # horizon-client's build script (stack size, mod API generation) runs for this package too.
        printf 'build = "../horizon-client/build.rs"\n\n'
        printf '[[bin]]\nname = "horizon-client"\npath = "src/main.rs"\n\n'
        printf '[dependencies]\n'
        printf 'horizon-client = { path = "../horizon-client", default-features = false }\n'
        printf 'horizon-threads = { path = "../horizon-threads" }\n'
        printf 'horizon-mods = { path = "../horizon-mods" }\n'
        cat /tmp/trait-deps.toml
        printf '\n'
        awk '{ sub(/\r$/, "") } /^\[/ { on = ($0 == "[build-dependencies]") } on' "$SRC/crates/horizon-client/Cargo.toml"
        # The server build's --no-default-features turns the game's sound and controllers off here.
        printf '\n[features]\ndefault = ["horizon-client/default"]\n'
    } > "$game/Cargo.toml"

    naming=""
    if grep -q 'pub fn name_mods(' "$SRC/crates/horizon-client/src/mod_api.rs"; then
        naming="    horizon_client::mod_api::name_mods(&[$ids]);"
    fi
    {
        printf '//! Generated by packaging/coolify/build.sh: the game with its trait mods.\n'
        printf '#![forbid(unsafe_code)]\n\n'
        printf '#[global_allocator]\nstatic ALLOCATOR: horizon_threads::Allocator = horizon_threads::ALLOCATOR;\n\n'
        printf 'static MOD_API_LEVEL: [u8; horizon_mods::api_level::MARK.len()] = horizon_mods::api_level::MARK;\n\n'
        printf 'fn main() {\n    std::hint::black_box(&MOD_API_LEVEL);\n'
        printf '    let mods: horizon_client::mod_api::Mods = vec![\n'
        cat /tmp/trait-entries.rs
        printf '    ].into();\n%s\n    horizon_client::entry::main_with(mods);\n}\n' "$naming"
    } > "$game/src/main.rs"

    printf '    "crates/native-game",\n' >> /tmp/trait-members.txt
    grep -q '^members = \[' "$SRC/Cargo.toml" || die "the workspace Cargo.toml has no 'members = [' line"
    sed -i '/^members = \[/r /tmp/trait-members.txt' "$SRC/Cargo.toml"
    # Adds the new path packages to Cargo.lock; their crates.io dependencies must already be in it.
    (cd "$SRC" && cargo metadata --format-version 1 > /dev/null)
}

mkdir -p /out/content
: > /out/content/ids.txt
: > /out/native-mods.txt
SRC=/src

# --- Native mods -------------------------------------------------------------------------------------
bundles=$(find /src/server-mods/native -mindepth 2 -maxdepth 2 -name horizon-mixin.toml 2>/dev/null | sort || true)
mixin_mods=0
: > /tmp/trait-mods.txt
if [ -n "$bundles" ]; then
    say "Native mods found:"
    rm -rf /mods /stage
    mkdir -p /mods
    for manifest in $bundles; do
        dir=$(dirname "$manifest")
        id=$(toml_get "$manifest" mod id)
        [ -n "$id" ] || die "$manifest has no [mod] id"
        api=$(toml_get "$manifest" api path)
        # Anything in [[...]] tables (injections, modules, crates, post-build steps) is horizon-mixins' work.
        if grep -q '^\[\[' "$manifest"; then
            grep -q '^[ \t]*trait_method[ \t]*=' "$manifest" \
                && die "$id: trait_method injections need the launcher's build (horizon-launcher native-build --server-output)"
            cp -r "$dir" /mods/
            [ -n "$api" ] && strip_api "/mods/$(basename "$dir")/horizon-mixin.toml"
            mixin_mods=$((mixin_mods + 1))
            kind="mixins"
        else
            kind=""
        fi
        if [ -n "$api" ]; then
            [ -f "$dir/$api/Cargo.toml" ] || die "$id: [api] path $api has no Cargo.toml"
            printf '%s\t%s\t%s\n' "$id" "$dir" "$api" >> /tmp/trait-mods.txt
            kind="${kind:+$kind + }trait mod"
        fi
        [ -n "$kind" ] || die "$id: neither injections nor an [api] package: nothing to build"
        echo "  - $id ($kind)"
        echo "$id" >> /out/native-mods.txt
    done
fi

if [ "$mixin_mods" -gt 0 ]; then
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

# Trait mods change the workspace (new members, Cargo.lock), so they are staged before the mirror below: the
# staged files are the same every build, and the mirror keeps their timestamps too.
game_package=horizon-client
if [ -s /tmp/trait-mods.txt ]; then
    say "Linking the trait mods into the game"
    order_trait_mods /tmp/trait-mods.txt /tmp/trait-mods-ordered.txt
    echo "  in dependency order: $(cut -f1 /tmp/trait-mods-ordered.txt | tr '\n' ' ')"
    stage_trait_mods /tmp/trait-mods-ordered.txt
    game_package=horizon-native-game
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
cargo build --locked --profile "$PROFILE" -p "$game_package" --no-default-features --bin horizon-client
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
