#!/usr/bin/env bash
# Probe P-34: what cargo reads as a dependency in a manifest, and what a
# lockfile holds. Run in an empty directory: ./probe.sh <workdir>
# Needs cargo, git and a reachable crates.io (one crate: cfg-if).
set -u
W="$1"
rm -rf "$W" && mkdir -p "$W" && cd "$W" || exit 1
W="$(pwd -P)"
export CARGO_TARGET_DIR="$W/target"
export GIT_AUTHOR_NAME=probe GIT_AUTHOR_EMAIL=probe@example.invalid
export GIT_COMMITTER_NAME=probe GIT_COMMITTER_EMAIL=probe@example.invalid

say() { printf '\n===== %s =====\n' "$*"; }
run() { printf '$ %s\n' "$*"; "$@" 2>&1 | sed "s#$W#<W>#g"; printf '[exit %s]\n' "${PIPESTATUS[0]}"; }
show() { printf -- '--- %s\n' "$1"; sed "s#$W#<W>#g" "$1"; }
# pkg <dir> <name> <version> <edition> : a library package with an empty lib
pkg() {
  mkdir -p "$1/src" && : > "$1/src/lib.rs"
  printf '[package]\nname = "%s"\nversion = "%s"\nedition = "%s"\n' "$2" "$3" "$4" > "$1/Cargo.toml"
}

cargo --version; git --version

# A local git repository that holds one package, with a branch and a tag.
pkg "$W/gitdep" gitdep 0.3.0 2021
( cd "$W/gitdep" && git init -q -b main && git add . && git commit -qm one \
  && git -c tag.gpgSign=false tag v0.3.0 && git branch dev )
GITURL="file://$W/gitdep"
GITSHA="$(git -C "$W/gitdep" rev-parse HEAD)"

# A local git repository whose package has an optional registry dependency.
pkg "$W/gitopt" gitopt 0.1.0 2021
cat >> "$W/gitopt/Cargo.toml" <<'EOF'

[dependencies]
cfg-if = { version = "1", optional = true }

[features]
extra = ["dep:cfg-if"]
EOF
( cd "$W/gitopt" && git init -q -b main && git add . && git commit -qm one )
OPTURL="file://$W/gitopt"

say "1a underscore tables, edition 2021"
pkg "$W/u21" u21 0.1.0 2021
pkg "$W/u21/a" dep-a 0.1.0 2021
pkg "$W/u21/b" dep-b 0.1.0 2021
pkg "$W/u21/c" dep-c 0.1.0 2021
cat >> "$W/u21/Cargo.toml" <<'EOF'

[dev_dependencies]
dep-a = { path = "a" }

[build_dependencies]
dep-b = { path = "b" }

[target.'cfg(unix)'.dev_dependencies]
dep-c = { path = "c", default_features = false }
EOF
( cd "$W/u21" && run cargo generate-lockfile && show Cargo.lock )

say "1b underscore tables, edition 2024"
cp -R "$W/u21" "$W/u24" && rm -f "$W/u24/Cargo.lock"
sed -i '' 's/name = "u21"/name = "u24"/; 1,5s/edition = "2021"/edition = "2024"/' "$W/u24/Cargo.toml"
( cd "$W/u24" && run cargo generate-lockfile; ls Cargo.lock 2>&1 )

say "2 shapes: workspace member, path, registry, git by branch, tag, rev and default"
mkdir -p "$W/shapes" && cd "$W/shapes" || exit 1
cat > Cargo.toml <<'EOF'
[workspace]
resolver = "3"
members = ["m1", "m2", "m3", "m4"]
EOF
pkg m1 m1 0.1.0 2024; pkg m2 m2 0.1.0 2024; pkg m3 m3 0.1.0 2024; pkg m4 m4 0.1.0 2024
pkg local local-path 0.2.0 2024
cat >> m1/Cargo.toml <<EOF

[dependencies]
cfg-if = "1"
local-path = { path = "../local" }
m2 = { path = "../m2", version = "0.1" }
gitdep = { git = "$GITURL", branch = "dev" }
EOF
printf '\n[dependencies]\ngitdep = { git = "%s", tag = "v0.3.0" }\n' "$GITURL" >> m2/Cargo.toml
printf '\n[dependencies]\ngitdep = { git = "%s", rev = "%s" }\n' "$GITURL" "$GITSHA" >> m3/Cargo.toml
printf '\n[dependencies]\ngitdep = { git = "%s", version = "0.3" }\n' "$GITURL" >> m4/Cargo.toml
run cargo generate-lockfile; show Cargo.lock

say "3 a [workspace.dependencies] entry that no member inherits"
mkdir -p "$W/wsdep" && cd "$W/wsdep" || exit 1
cat > Cargo.toml <<'EOF'
[workspace]
resolver = "3"
members = ["m1"]

[workspace.dependencies]
cfg-if = "1"
EOF
pkg m1 m1 0.1.0 2024
run cargo generate-lockfile; show Cargo.lock
say "3b the member inherits it"
printf '\n[dependencies]\ncfg-if.workspace = true\n' >> m1/Cargo.toml
run cargo generate-lockfile; show Cargo.lock

say "4a an optional dependency that no feature turns on"
pkg "$W/opt" opt 0.1.0 2024 && cd "$W/opt" || exit 1
printf '\n[dependencies]\ncfg-if = { version = "1", optional = true }\n' >> Cargo.toml
run cargo generate-lockfile; show Cargo.lock
run cargo tree

say "4b a dependency whose own optional dependency is off, then on by a feature"
pkg "$W/feat" feat 0.1.0 2024 && cd "$W/feat" || exit 1
printf '\n[dependencies]\ngitopt = { git = "%s" }\n' "$OPTURL" >> Cargo.toml
run cargo generate-lockfile; cp Cargo.lock Cargo.lock.off; show Cargo.lock
run cargo tree
sed -i '' 's#gitopt = { git = "\(.*\)" }#gitopt = { git = "\1", features = ["extra"] }#' Cargo.toml
run cargo generate-lockfile
run diff Cargo.lock.off Cargo.lock
run cargo tree

say "5 a renamed dependency"
pkg "$W/rename" rename 0.1.0 2024 && cd "$W/rename" || exit 1
printf '\n[dependencies]\nalias = { package = "cfg-if", version = "1" }\n' >> Cargo.toml
run cargo generate-lockfile; show Cargo.lock

say "6a [patch] in the root manifest, used"
pkg "$W/patch" patch 0.1.0 2024 && cd "$W/patch" || exit 1
pkg vendor/cfg-if cfg-if 1.0.99 2021
pkg vendor/unused never-used 9.9.9 2021
cat >> Cargo.toml <<'EOF'

[dependencies]
cfg-if = "1"

[patch.crates-io]
cfg-if = { path = "vendor/cfg-if" }
never-used = { path = "vendor/unused" }
EOF
run cargo generate-lockfile; show Cargo.lock

say "6b [patch] in a member that is not the workspace root"
mkdir -p "$W/patchm" && cd "$W/patchm" || exit 1
printf '[workspace]\nresolver = "3"\nmembers = ["m1"]\n' > Cargo.toml
pkg m1 m1 0.1.0 2024
pkg m1/vendor/cfg-if cfg-if 1.0.99 2021
cat >> m1/Cargo.toml <<'EOF'

[dependencies]
cfg-if = "1"

[patch.crates-io]
cfg-if = { path = "vendor/cfg-if" }
EOF
printf '[workspace]\n' >> m1/vendor/cfg-if/Cargo.toml
run cargo generate-lockfile; show Cargo.lock

say "7 [replace]"
pkg "$W/replace" replace 0.1.0 2024 && cd "$W/replace" || exit 1
cp "$W/net/Cargo.lock" /dev/null 2>/dev/null
printf '\n[dependencies]\ncfg-if = "1"\n' >> Cargo.toml
run cargo generate-lockfile
V="$(sed -n '/name = "cfg-if"/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)"
pkg vendor/cfg-if cfg-if "$V" 2021
printf '\n[replace]\n"cfg-if:%s" = { path = "vendor/cfg-if" }\n' "$V" >> Cargo.toml
run cargo generate-lockfile; show Cargo.lock

say "8 a lockfile with a package that no manifest leads to"
pkg "$W/extra" extra 0.1.0 2024 && cd "$W/extra" || exit 1
printf '\n[dependencies]\ncfg-if = "1"\n' >> Cargo.toml
run cargo generate-lockfile; cp Cargo.lock Cargo.lock.orig
cat >> Cargo.lock <<EOF

[[package]]
name = "gitdep"
version = "0.3.0"
source = "git+$GITURL#$GITSHA"
EOF
run cargo build --locked
run cargo build
run diff Cargo.lock.orig Cargo.lock

say "9 a lockfile with a changed checksum"
cp Cargo.lock.orig Cargo.lock
sed -i '' 's/^checksum = "..../checksum = "0000/' Cargo.lock
run grep checksum Cargo.lock
run cargo build --locked
run cargo build
run diff Cargo.lock.orig Cargo.lock

say "10 a lockfile whose package has another source than its manifest asks for"
pkg "$W/gitcfg" cfg-if "$V" 2021
( cd "$W/gitcfg" && git init -q -b main && git add . && git commit -qm one )
CFGSHA="$(git -C "$W/gitcfg" rev-parse HEAD)"
cp Cargo.lock.orig Cargo.lock
sed -i '' "s#^source = \"registry+.*#source = \"git+file://$W/gitcfg\#$CFGSHA\"#; /^checksum/d" Cargo.lock
show Cargo.lock
run cargo build --locked
run cargo build
run diff Cargo.lock.orig Cargo.lock

say "11 a lockfile of a version cargo does not know"
cp Cargo.lock.orig Cargo.lock
sed -i '' 's/^version = 4/version = 99/' Cargo.lock
run cargo build --locked

say "12 a Cargo.lock in a member directory"
cd "$W/shapes" || exit 1
printf 'this is not a lockfile\n' > m1/Cargo.lock
run cargo build --locked
( cd m1 && run cargo build --locked )
printf 'this is not a lockfile\n' > Cargo.lock.bad && cp Cargo.lock Cargo.lock.good && cp Cargo.lock.bad Cargo.lock
run cargo build --locked
cp Cargo.lock.good Cargo.lock; rm m1/Cargo.lock

say "13 the key registry-index"
pkg "$W/regidx" regidx 0.1.0 2024 && cd "$W/regidx" || exit 1
printf '\n[dependencies]\ncfg-if = { version = "1", registry-index = "https://github.com/rust-lang/crates.io-index" }\n' >> Cargo.toml
run cargo generate-lockfile; show Cargo.lock

say "14 tables and entries of an unexpected shape"
shape() {
  pkg "$W/shape" shape 0.1.0 2024; rm -f "$W/shape/Cargo.lock"
  printf '%s\n' "$1" >> "$W/shape/Cargo.toml"
  printf -- '--- appended: %s\n' "$(printf '%s' "$1" | tr '\n' '|')"
  ( cd "$W/shape" && run cargo generate-lockfile )
}
shape 'dependencies = "cfg-if"'
shape $'[dependencies]\ncfg-if = 1'
shape $'[dependencies]\ncfg-if = ["1"]'
shape $'[dependencies]\ncfg-if = { version = "1", git = 5 }'
shape $'[dependencies]\ncfg-if = { version = "1", bogus-key = "x" }'
shape $'[dependencies]\ncfg-if = { }'
shape $'[target]\ndependencies = { cfg-if = "1" }'
shape $'[target.\'cfg(unix)\'.bogus-dependencies]\ncfg-if = "1"'
shape $'[bogus-dependencies]\ncfg-if = "1"'
shape $'[patch]\ncrates-io = "x"'
shape $'[workspace]\ndependencies = "x"'
shape $'[dependencies]\ncfg-if = { version = "1" }\n[dependencies.cfg-if]\nversion = "1"'

say "15 git and path together; two registries"
shape $'[dependencies]\ngitdep = { git = "file:///nowhere", path = "x" }'
shape $'[dependencies]\ncfg-if = { version = "1", registry = "nowhere" }'

say "16 a git dependency: the lockfile's commit decides what is built"
pkg "$W/gitmove" gitmove 0.1.0 2021
( cd "$W/gitmove" && git init -q -b main && git add . && git commit -qm one )
SHA1="$(git -C "$W/gitmove" rev-parse HEAD)"
pkg "$W/usegit" usegit 0.1.0 2024 && cd "$W/usegit" || exit 1
printf '\n[dependencies]\ngitmove = { git = "file://%s/gitmove" }\n' "$W" >> Cargo.toml
run cargo generate-lockfile; run grep -n 'source' Cargo.lock
( cd "$W/gitmove" && echo 'pub fn two() {}' > src/lib.rs && git commit -qam two )
SHA2="$(git -C "$W/gitmove" rev-parse HEAD)"
printf 'first commit %s, second commit %s, same package version\n' "$SHA1" "$SHA2"
run cargo build --locked
run cargo tree --locked
sed -i '' "s/$SHA1/$SHA2/" Cargo.lock
run grep -n 'source' Cargo.lock
run cargo build --locked
run cargo tree --locked

say "17 a registry dependency: another version in the lockfile, the manifest unchanged"
cd "$W/extra" || exit 1
cp Cargo.lock.orig Cargo.lock
run cargo update cfg-if --precise 1.0.0
run diff Cargo.lock.orig Cargo.lock
run cargo build --locked

say "18 the key registry-index with an index of its own"
mkdir -p "$W/noindex"
pkg "$W/regidx2" regidx2 0.1.0 2024 && cd "$W/regidx2" || exit 1
printf '\n[dependencies]\ncfg-if = { version = "1", registry-index = "file://%s/noindex" }\n' "$W" >> Cargo.toml
run cargo generate-lockfile

say "19 a dependency table that is no table"
mkdir -p "$W/notable/src" && cd "$W/notable" && : > src/lib.rs || exit 1
printf 'dependencies = "cfg-if"\n\n[package]\nname = "notable"\nversion = "0.1.0"\nedition = "2024"\n' > Cargo.toml
run cargo generate-lockfile
printf 'dev-dependencies = 5\n\n[package]\nname = "notable"\nversion = "0.1.0"\nedition = "2024"\n' > Cargo.toml
run cargo generate-lockfile
printf 'target = "x"\n\n[package]\nname = "notable"\nversion = "0.1.0"\nedition = "2024"\n' > Cargo.toml
run cargo generate-lockfile
printf '[package]\nname = "notable"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\ncfg-if = { version = "1", optional = "yes" }\n' > Cargo.toml
run cargo generate-lockfile

say "20 unknown tables and keys draw a warning from a build"
shape $'[bogus-dependencies]\ncfg-if = "1"'
( cd "$W/shape" && run cargo check )
shape $'[dependencies]\ncfg-if = { version = "1", bogus-key = "x", registry-index = "https://github.com/rust-lang/crates.io-index" }'
( cd "$W/shape" && run cargo check )

say "21 more tables that are no table"
top() {
  mkdir -p "$W/top/src" && : > "$W/top/src/lib.rs"; rm -f "$W/top/Cargo.lock"
  printf '%s\n\n[package]\nname = "top"\nversion = "0.1.0"\nedition = "2024"\n%s\n' "$1" "${2-}" > "$W/top/Cargo.toml"
  printf -- '--- before [package]: %s ; after it: %s\n' "$1" "$(printf '%s' "${2-}" | tr '\n' '|')"
  ( cd "$W/top" && run cargo generate-lockfile )
}
top 'workspace = "x"'
top 'replace = "x"'
top 'patch = "x"'
top 'build-dependencies = ["x"]'
top '' $'[target]\nx = "y"'
top '' $'[target.x]\ndependencies = "y"'
top '' $'[target.x]\nbuild_dependencies = 5'
top '' $'[replace]\n"cfg-if:1.0.5" = 5'
top '' $'[patch.crates-io]\ncfg-if = 5'

say "22 a manifest that gained a feature, with the lockfile left as it was"
cd "$W/feat" || exit 1
cp Cargo.lock Cargo.lock.on; cp Cargo.lock.off Cargo.lock
run grep -n 'gitopt' Cargo.toml
run cargo build --locked
run cargo build
run diff Cargo.lock.off Cargo.lock

say "23 [replace] in a member that is not the workspace root"
mkdir -p "$W/replacem" && cd "$W/replacem" || exit 1
printf '[workspace]\nresolver = "3"\nmembers = ["m1"]\n' > Cargo.toml
pkg m1 m1 0.1.0 2024
pkg m1/vendor/cfg-if cfg-if "$V" 2021
printf '[workspace]\n' >> m1/vendor/cfg-if/Cargo.toml
printf '\n[dependencies]\ncfg-if = "1"\n\n[replace]\n"cfg-if:%s" = { path = "vendor/cfg-if" }\n' "$V" >> m1/Cargo.toml
run cargo generate-lockfile; show Cargo.lock

say "24 more of the underscore spellings"
pkg "$W/u21b" u21b 0.1.0 2021 && cd "$W/u21b" || exit 1
pkg d dep-d 0.1.0 2021
printf "\n[target.'cfg(unix)'.build_dependencies]\ndep-d = { path = \"d\" }\n" >> Cargo.toml
run cargo generate-lockfile; show Cargo.lock
( cd "$W/u21" && run cargo check && cargo metadata --format-version 1 --no-deps 2>/dev/null | tr '{' '\n' | grep '"name":"dep-' | sed 's/"req".*"uses_default_features":/ ... uses_default_features:/; s/,"features".*//' )
pkg "$W/u24b" u24b 0.1.0 2024 && cd "$W/u24b" || exit 1
pkg b dep-b 0.1.0 2021
printf '\n[build_dependencies]\ndep-b = { path = "b" }\n' >> Cargo.toml
run cargo generate-lockfile
pkg "$W/u24c" u24c 0.1.0 2024 && cd "$W/u24c" || exit 1
pkg c dep-c 0.1.0 2021
printf '\n[dependencies]\ndep-c = { path = "c", default_features = false }\n' >> Cargo.toml
run cargo generate-lockfile

say "25 a lockfile with the checksum taken out"
cd "$W/extra" || exit 1
cp Cargo.lock.orig Cargo.lock
sed -i '' '/^checksum/d' Cargo.lock
run cargo build --locked
run cargo build
run diff Cargo.lock.orig Cargo.lock

say "26 an optional dependency made no longer optional"
cd "$W/opt" || exit 1
cp Cargo.lock Cargo.lock.optional
sed -i '' 's/cfg-if = { version = "1", optional = true }/cfg-if = { version = "1" }/' Cargo.toml
run cargo generate-lockfile
run diff Cargo.lock.optional Cargo.lock
run cargo tree

say "27 an unknown table under target, in a build"
shape $'[target.\'cfg(unix)\'.bogus-dependencies]\ncfg-if = "1"'
( cd "$W/shape" && run cargo check )
