#!/bin/bash
# Probe P-39, part 1: which directories does rustup make for a channel, and
# what does its record of components hold as components come and go?
# Run as: install.sh <rustup binary> <empty scratch dir>
# Everything is installed into a rustup home below the scratch directory, and
# rustup runs in an empty directory: in a directory that holds a
# rust-toolchain.toml it installs what that file names (the last case).
set -u
RU=$1
S=$2
HOST=$(uname -m | sed 's/arm64/aarch64/')-apple-darwin
mkdir -p "$S/empty"
ru() { (cd "$S/empty" && env -i HOME="$S/home" RUSTUP_HOME="$S/rustup" CARGO_HOME="$S/cargo" PATH=/usr/bin:/bin "$RU" "$@" 2>&1); }
rec() { cat "$S/rustup/toolchains/$1-$HOST/lib/rustlib/components"; }

echo "## rustup"
ru --version | head -1
echo "## install 1.100.0: profile minimal, components clippy and rustfmt, target x86_64-unknown-linux-gnu"
ru toolchain install 1.100.0 --profile minimal --component clippy --component rustfmt \
  --target x86_64-unknown-linux-gnu --no-self-update | grep installed
echo "## install 1.100: profile minimal"
ru toolchain install 1.100 --profile minimal --no-self-update | grep installed
echo "## install stable: profile minimal"
ru toolchain install stable --profile minimal --no-self-update | grep installed
echo "## the rustup home"
/bin/ls -1 "$S/rustup"
echo "## its toolchains directory"
/bin/ls -1 "$S/rustup/toolchains"

echo "## 1.100.0: lib/rustlib/components"
rec 1.100.0
echo "## 1.100.0: bin"
/bin/ls -1 "$S/rustup/toolchains/1.100.0-$HOST/bin"
echo "## 1.100.0: lib/rustlib"
/bin/ls -1 "$S/rustup/toolchains/1.100.0-$HOST/lib/rustlib"
echo "## 1.100 (minimal): lib/rustlib/components"
rec 1.100
echo "## 1.100 (minimal): bin"
/bin/ls -1 "$S/rustup/toolchains/1.100-$HOST/bin"

echo "## 1.100.0: rustup component remove clippy"
ru component remove clippy --toolchain 1.100.0 | tail -1
rec 1.100.0
echo "## 1.100.0: bin after it"
/bin/ls -1 "$S/rustup/toolchains/1.100.0-$HOST/bin"
echo "## 1.100.0: rustup target remove x86_64-unknown-linux-gnu"
ru target remove x86_64-unknown-linux-gnu --toolchain 1.100.0 | tail -1
rec 1.100.0
echo "## 1.100.0: entries of lib/rustlib that name that target"
/bin/ls -1 "$S/rustup/toolchains/1.100.0-$HOST/lib/rustlib" | grep -c x86_64-unknown-linux-gnu
echo "## 1.100.0: both added again"
ru component add clippy --toolchain 1.100.0 | tail -1
ru target add x86_64-unknown-linux-gnu --toolchain 1.100.0 | tail -1
rec 1.100.0
echo "## 1.100: rustup component add rust-src"
ru component add rust-src --toolchain 1.100 | tail -1
rec 1.100

echo "## the names that the channel manifest of 1.100.0 renames"
python3 - "$S/rustup/toolchains/1.100.0-$HOST/lib/rustlib/multirust-channel-manifest.toml" <<'PY'
import sys, tomllib
m = tomllib.load(open(sys.argv[1], "rb"))
for name, to in sorted(m["renames"].items()):
    print(f"{name} -> {to['to']}")
PY

echo "## a second rustup home, empty; rustup --version in a directory whose rust-toolchain.toml names 1.100.0 with clippy"
mkdir -p "$S/withfile"
printf '[toolchain]\nchannel = "1.100.0"\ncomponents = ["clippy"]\n' >"$S/withfile/rust-toolchain.toml"
(cd "$S/withfile" && env -i HOME="$S/home" RUSTUP_HOME="$S/rustup2" CARGO_HOME="$S/cargo" PATH=/usr/bin:/bin "$RU" --version 2>&1) |
  grep -v '^$' | sed "s|$S|<scratch>|"
echo "## the toolchains directory of that home afterwards"
/bin/ls -1 "$S/rustup2/toolchains"
echo "## and its record of components"
cat "$S/rustup2/toolchains/1.100.0-$HOST/lib/rustlib/components"
