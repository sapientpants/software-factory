#!/bin/zsh
# cargo itself: configuration file names and locations, precedence, what an explicit variable does.
source ${0:A:h}/env.sh
mkfx cargo
unset CARGO_TARGET_DIR      # the canary is build.target-dir, so the variable must not mask it
echo "## versions"; cargo --version
ALL=( $REPO/.cargo/config.toml $REPO/.cargo/config $MEMBER/.cargo/config.toml $MEMBER/.cargo/config $MEMBER/src/.cargo/config.toml
      $OUTER/.cargo/config.toml $OUTER/.cargo/config $FX/.cargo/config.toml $CARGO_HOME/config.toml $CARGO_HOME/config
      $REPO/.cargo/config.json $REPO/.cargo/config.d/extra.toml $REPO/cargo.toml.d/x.toml $REPO/.config/cargo.toml $REPO/.config/cargo/config.toml $REPO/config.toml $REPO/.cargo.toml $REPO/cargo-config.toml $REPO/.cargo/Config.TOML )
clean() { local g; for g in $ALL; do rm -f "$g"; done; rm -rf $REPO/.cargo $MEMBER/.cargo $MEMBER/src/.cargo $OUTER/.cargo $FX/.cargo $REPO/.config $REPO/cargo.toml.d; }
# put FILE ID: FILE sets build.target-dir = "td-ID" (one winner, shown by `cargo metadata`) and alias canary-ID (merged, shown by `cargo --list`)
put() { mkdir -p "${1:h}"; printf '[build]\ntarget-dir = "td-%s"\n[alias]\ncanary-%s = "version"\n' $2 $2 > "$1"; }
# show DIR: what cargo run in DIR resolved
show() {
  local dir=$1; shift
  echo "\$ (cd ${dir#$FX/} && $* cargo metadata | target_directory ; cargo --list | grep canary)" | strip
  (cd $dir && env "$@" cargo metadata --format-version 1 --no-deps 2> >(strip | grep -E '^(warning|error)' | sed 's/^/    stderr: /' >&2) | python3 -c 'import json,sys; print("    target_directory =", json.load(sys.stdin)["target_directory"])' | strip
   env "$@" cargo --list 2>/dev/null | grep canary | sed 's/^ */    alias: /') 2>&1
}
echo; echo "## A. baseline"; clean; show $REPO
echo; echo "## B. one candidate at a time, cwd = workspace root"
n=0; for f in $ALL; do n=$((n+1)); clean; put "$f" $n; echo; echo "# only $f (ID $n)" | strip; show $REPO; done
echo; echo "## C. cwd = member directory"
clean; put $MEMBER/.cargo/config.toml member; put $REPO/.cargo/config.toml root; put $OUTER/.cargo/config.toml outer; put $CARGO_HOME/config.toml home
echo "# member, root, outer and CARGO_HOME files all present"; show $MEMBER; echo "# same files, cwd = workspace root"; show $REPO
echo "# same files, cwd outside the tree, --manifest-path into it"
(cd $S && cargo metadata --format-version 1 --no-deps --manifest-path $REPO/Cargo.toml | python3 -c 'import json,sys; print("    target_directory =", json.load(sys.stdin)["target_directory"])' | strip)
echo; echo "## D. config and config.toml in the same directory"
clean; put $REPO/.cargo/config.toml toml; put $REPO/.cargo/config legacy; show $REPO
echo "# full stderr:"; (cd $REPO && cargo metadata --format-version 1 --no-deps 2>&1 >/dev/null | strip)
echo; echo "## E. explicit settings against an in-tree file (repo/.cargo/config.toml sets target-dir=td-root)"
clean; put $REPO/.cargo/config.toml root
echo "# E1: CARGO_BUILD_TARGET_DIR / CARGO_TARGET_DIR in the environment"; show $REPO CARGO_TARGET_DIR=/tmp/td-env
echo "# E2: --config on the command line"; (cd $REPO && cargo metadata --format-version 1 --no-deps --config 'build.target-dir="td-cli"' | python3 -c 'import json,sys; print("    target_directory =", json.load(sys.stdin)["target_directory"])' | strip)
echo "# E3: the in-tree alias is still defined with the variable set (the file is still read):"; (cd $REPO && CARGO_TARGET_DIR=/tmp/td-env cargo --list | grep canary)
echo "# E4: include = [...] inside .cargo/config.toml pulling an unprotected path"
printf 'include = ["../not-in-dot-cargo.toml"]\n' > $REPO/.cargo/config.toml; put $REPO/not-in-dot-cargo.toml included; show $REPO; rm -f $REPO/not-in-dot-cargo.toml
echo; echo "## F. rustflags: environment against file (clippy canary: clippy.toml disallows std::env::var, the source calls it)"
clean; export CARGO_TARGET_DIR=$S/target/cargo-f
printf 'pub fn f() -> Option<String> {\n    std::env::var("X").ok()\n}\n' > $MEMBER/src/lib.rs
printf 'disallowed-methods = [{ path = "std::env::var", reason = "CANARY" }]\n' > $REPO/clippy.toml
mkdir -p $REPO/.cargo; printf '[build]\nrustflags = ["--cap-lints", "allow"]\n' > $REPO/.cargo/config.toml
r() { echo "\$ $*" | strip; (cd $REPO && env "$@" 2>&1 | strip | grep -E 'CANARY|^error|Finished' | sort -u | sed 's/^/    /'; echo "  -> exit ${pipestatus[1]}"); touch $MEMBER/src/lib.rs; }
echo "# F1: [build] rustflags = [\"--cap-lints\",\"allow\"] in repo/.cargo/config.toml, no variable"; r cargo clippy --workspace --all-targets --locked -- -D warnings
echo "# F2: plus RUSTFLAGS=--cfg=x in the environment"; r RUSTFLAGS=--cfg=x cargo clippy --workspace --all-targets --locked -- -D warnings
echo "# F3: plus RUSTFLAGS= (set, empty)"; r RUSTFLAGS= cargo clippy --workspace --all-targets --locked -- -D warnings
echo "# F4: plus CARGO_ENCODED_RUSTFLAGS= (set, empty)"; r CARGO_ENCODED_RUSTFLAGS= cargo clippy --workspace --all-targets --locked -- -D warnings
echo "# F5: plus CARGO_BUILD_RUSTFLAGS=--cfg=x"; r CARGO_BUILD_RUSTFLAGS=--cfg=x cargo clippy --workspace --all-targets --locked -- -D warnings
printf '[target.aarch64-apple-darwin]\nrustflags = ["--cap-lints", "allow"]\n' > $REPO/.cargo/config.toml
echo "# F6: [target.aarch64-apple-darwin] rustflags in the file, RUSTFLAGS=--cfg=x in the environment"; r RUSTFLAGS=--cfg=x cargo clippy --workspace --all-targets --locked -- -D warnings
echo "# F7: same file, CARGO_BUILD_RUSTFLAGS=--cfg=x"; r CARGO_BUILD_RUSTFLAGS=--cfg=x cargo clippy --workspace --all-targets --locked -- -D warnings
echo "# F8: same file, --config 'build.rustflags=[]' on the command line"; r cargo clippy --config 'build.rustflags=[]' --workspace --all-targets --locked -- -D warnings
rm -f $REPO/clippy.toml
echo; echo "## G. cargo test: inputs that change the verdict of a failing test"
clean; printf 'pub fn f() -> u32 {\n    1\n}\n#[test]\nfn fails() {\n    assert_eq!(f(), 2);\n}\n' > $MEMBER/src/lib.rs
echo "# G0: control"; r cargo test --workspace --locked
mkdir -p $REPO/.cargo; printf '[target.aarch64-apple-darwin]\nrunner = "/usr/bin/true"\n' > $REPO/.cargo/config.toml
echo "# G1: repo/.cargo/config.toml [target.aarch64-apple-darwin] runner = \"/usr/bin/true\""; r cargo test --workspace --locked
echo "# G2: same, CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER= (empty) in the environment"; r CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER= cargo test --workspace --locked
printf '[target."cfg(all())"]\nrunner = "/usr/bin/true"\n' > $REPO/.cargo/config.toml
echo "# G3: [target.\"cfg(all())\"] runner in the file, CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER=/usr/bin/env in the environment"; r CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER=/usr/bin/env cargo test --workspace --locked
rm -rf $REPO/.cargo
printf '\n[lib]\ntest = false\ndoctest = false\n' >> $MEMBER/Cargo.toml; echo "# G4: member/Cargo.toml [lib] test = false"; r cargo test --workspace --locked
sed -i '' '/^\[lib\]/,$d' $MEMBER/Cargo.toml
printf 'fn main() {\n    eprintln!("BUILD-RS-RAN");\n    println!("cargo::rustc-cfg=canary_off");\n}\n' > $MEMBER/build.rs
printf 'pub fn f() -> u32 {\n    1\n}\n#[cfg(not(canary_off))]\n#[test]\nfn fails() {\n    assert_eq!(f(), 2);\n}\n' > $MEMBER/src/lib.rs
echo "# G5: member/build.rs added (no Cargo.toml change) emits a cfg that compiles the failing test out"; r cargo test --workspace --locked
rm -f $MEMBER/build.rs
echo; echo "## H. file-lookup trace of 'cargo metadata' from the member directory: config-like and manifest probes"
clean; export FTRACE_OUT=$S/out/03-cargo.trace; rm -f $FTRACE_OUT
(cd $MEMBER && ftrace cargo metadata --format-version 1 --no-deps >/dev/null 2>&1)
grep -E '(/config[^/]*|Cargo\.toml|Cargo\.lock|rust-toolchain[^/]*|credentials[^/]*)$' $FTRACE_OUT | grep -vE '/toolchains/|settings.toml' | awk '{ $1=""; print }' | strip | sort | uniq -c
rm -rf $S/target/cargo-f
