#!/bin/bash
# Probe P-39: is a toolchain directory that rustup made usable with its `bin`
# on a bare PATH, with no rustup, from a copy, without write permission and
# through a symbolic link? Run as: use.sh <scratch dir>, after install.sh on
# the same directory. The crate of case/ is copied into the scratch directory.
set -u
S=$1
CASE=$(cd "$(dirname "$0")/case" && pwd)
TC=$S/rustup/toolchains/1.100.0-aarch64-apple-darwin
MIN=$S/rustup/toolchains/1.100-aarch64-apple-darwin
n=0
rm -rf "$S/proj" && cp -R "$CASE" "$S/proj"

# bare <toolchain dir> <command...>: a cleared environment, the toolchain's
# bin first on PATH, a home and a CARGO_HOME of the case's own.
bare() {
  local tc=$1
  shift
  (cd "$S/proj" && env -i HOME="$S/home2" CARGO_HOME="$S/ch2" \
    CARGO_TARGET_DIR="$S/target-$n" PATH="$tc/bin:/usr/bin:/bin" "$@")
}

# try <toolchain dir> <command...>: the exit status, and the first error line.
try() {
  local tc=$1
  shift
  n=$((n + 1))
  bare "$tc" "$@" >/dev/null 2>"$S/err.txt"
  echo "exit=$? :: $*"
  grep -m1 -E '^error' "$S/err.txt"
}

echo "## A. bare PATH, no rustup, no RUSTUP_HOME; the directory holds a rust-toolchain.toml that names 1.97.1"
bare "$TC" cargo --version
bare "$TC" rustc --version
bare "$TC" rustc --print sysroot | sed "s|$S|<scratch>|"
try "$TC" cargo build --offline
try "$TC" cargo test --offline
try "$TC" cargo clippy --offline -- -D warnings
try "$TC" cargo fmt --check
try "$TC" cargo +1.97.1 --version

echo "## B. a copy of the toolchain under another path and name"
rm -rf "$S/moved" && mkdir -p "$S/moved" && cp -cR "$TC" "$S/moved/tc"
bare "$S/moved/tc" rustc --print sysroot | sed "s|$S|<scratch>|"
try "$S/moved/tc" cargo build --offline
try "$S/moved/tc" cargo clippy --offline -- -D warnings
try "$S/moved/tc" cargo fmt --check

echo "## C. the copy with no write permission on any entry (chmod -R a-w)"
chmod -R a-w "$S/moved/tc"
try "$S/moved/tc" cargo build --offline
try "$S/moved/tc" cargo test --offline
try "$S/moved/tc" cargo clippy --offline -- -D warnings
try "$S/moved/tc" cargo fmt --check
chmod -R u+w "$S/moved/tc"

echo "## D. through a symbolic link to the toolchain directory"
ln -sfn "$TC" "$S/link-tc"
bare "$S/link-tc" rustc --print sysroot | sed "s|$S|<scratch>|"
try "$S/link-tc" cargo build --offline

echo "## E. the toolchain installed with the minimal profile: no clippy, no rustfmt"
try "$MIN" cargo build --offline
try "$MIN" cargo clippy --offline
try "$MIN" cargo fmt --check

echo "## F. a second target, where its rust-std is installed and where it is not"
try "$TC" cargo check --offline --target x86_64-unknown-linux-gnu
try "$MIN" cargo check --offline --target x86_64-unknown-linux-gnu
