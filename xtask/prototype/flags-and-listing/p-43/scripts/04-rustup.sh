#!/bin/zsh
# rustup: toolchain file names, upward search, precedence, what RUSTUP_TOOLCHAIN / +toolchain do.
# Uses a rustup home inside the scratch directory, so no override or install touches ~/.rustup.
source ${0:A:h}/env.sh
export RUSTUP_HOME=$S/rustup-home; unset RUSTUP_TOOLCHAIN
RU=<home>/.cargo/bin/rustup
mkfx rustup 2>/dev/null
echo "## versions"; $RU --version 2>&1
# two installed names to tell apart: canary-a = 1.100.0 (scratch install), canary-b = 1.97.1 (link to the existing toolchain directory, read only)
$RU toolchain link canary-a $S/rustup-home/toolchains/1.100.0-aarch64-apple-darwin 2>&1
$RU toolchain link canary-b <home>/.rustup/toolchains/1.97.1-aarch64-apple-darwin 2>&1
$RU default 1.100.0 2>&1 | tail -1
$RU toolchain list
ALL=( $REPO/rust-toolchain.toml $REPO/rust-toolchain $MEMBER/rust-toolchain.toml $MEMBER/rust-toolchain $MEMBER/src/rust-toolchain $OUTER/rust-toolchain.toml $OUTER/rust-toolchain $FX/rust-toolchain
      $HOME/rust-toolchain.toml $HOME/rust-toolchain $REPO/.rust-toolchain.toml $REPO/.rust-toolchain $REPO/.rust-version $REPO/rust-version $REPO/.config/rust-toolchain.toml $REPO/.cargo/rust-toolchain.toml $REPO/.cargo/rust-toolchain $REPO/rust-toolchain.TOML $REPO/.tool-versions )
clean() { local g; for g in $ALL; do rm -f "$g"; done; rm -rf $REPO/.config $REPO/.cargo $REPO/evil-tc; }
tomlput() { mkdir -p "${1:h}"; printf '[toolchain]\nchannel = "%s"\n' $2 > "$1"; }
bareput() { mkdir -p "${1:h}"; printf '%s\n' $2 > "$1"; }
# show DIR [env...] : which toolchain rustup picks in DIR and why; and what `cargo`/`rustc` (the proxies) then report
show() {
  local dir=$1; shift
  echo "\$ (cd ${dir#$FX/} && $* rustup show active-toolchain; rustc --version)" | strip
  (cd $dir && env "$@" $RU show active-toolchain 2>&1 | strip | sed 's/^/    /'; env "$@" rustc --version 2>&1 | strip | sed 's/^/    rustc: /')
}
echo; echo "## A. baseline, no file"; clean; show $REPO
echo; echo "## B. one candidate at a time (file names canary-b = rustc 1.97.1; default is 1.100.0), cwd = workspace root"
for f in $ALL; do clean; case $f in *.toml|*.TOML) tomlput "$f" canary-b;; *.tool-versions) mkdir -p ${f:h}; echo "rust canary-b" > $f;; *) bareput "$f" canary-b;; esac; echo; echo "# only $f" | strip; show $REPO; done
clean; tomlput $REPO/rust-toolchain canary-b; echo; echo "# only outer/repo/rust-toolchain, holding TOML ([toolchain] channel = ...) instead of a bare name"; show $REPO
clean; bareput $REPO/rust-toolchain.toml canary-b; echo; echo "# only outer/repo/rust-toolchain.toml, holding a bare name instead of TOML"; show $REPO
echo; echo "## C. both names in one directory: rust-toolchain=canary-b (1.97.1), rust-toolchain.toml=canary-a (1.100.0)"
clean; bareput $REPO/rust-toolchain canary-b; tomlput $REPO/rust-toolchain.toml canary-a; show $REPO
echo "# the reverse assignment: rust-toolchain=canary-a, rust-toolchain.toml=canary-b"
clean; bareput $REPO/rust-toolchain canary-a; tomlput $REPO/rust-toolchain.toml canary-b; show $REPO
echo; echo "## D. across directories"
clean; tomlput $REPO/rust-toolchain.toml canary-a; bareput $MEMBER/rust-toolchain canary-b
echo "# repo/rust-toolchain.toml=canary-a, member/rust-toolchain=canary-b; cwd = workspace root"; show $REPO
echo "# same, cwd = member"; show $MEMBER
echo "# same, cwd = member/src"; show $MEMBER/src
echo "# same, cwd outside the tree, cargo --manifest-path into it"; (cd $S && cargo metadata --format-version 1 --no-deps --manifest-path $REPO/Cargo.toml >/dev/null 2>&1; echo "cargo metadata exit $?"; rustc --version; $RU show active-toolchain) | strip | sed 's/^/    /'
clean; tomlput $REPO/rust-toolchain.toml canary-a; bareput $OUTER/rust-toolchain canary-b
echo "# repo/rust-toolchain.toml=canary-a, outer/rust-toolchain=canary-b (above the git root); cwd = workspace root"; show $REPO
echo; echo "## E. explicit selection with files present (repo/rust-toolchain=canary-b, repo/rust-toolchain.toml=canary-b, member/rust-toolchain=canary-b)"
clean; bareput $REPO/rust-toolchain canary-b; tomlput $REPO/rust-toolchain.toml canary-b; bareput $MEMBER/rust-toolchain canary-b
echo "# E1: RUSTUP_TOOLCHAIN=1.100.0"; show $REPO RUSTUP_TOOLCHAIN=1.100.0; show $MEMBER RUSTUP_TOOLCHAIN=1.100.0
echo "# E2: cargo +1.100.0 / rustc +1.100.0"; (cd $REPO && rustc +1.100.0 --version 2>&1; cargo +1.100.0 --version 2>&1) | strip | sed 's/^/    /'
echo "# E3: a directory override (rustup override set, stored in the scratch rustup home) against the files"
(cd $REPO && $RU override set canary-a 2>&1 | tail -1 | strip); show $REPO; show $MEMBER; (cd $REPO && $RU override unset 2>&1 | tail -1 | strip)
echo "# E4: trace with RUSTUP_TOOLCHAIN set: does rustup even look for the files?"
export FTRACE_OUT=$S/out/04-rustup-env.trace; rm -f $FTRACE_OUT; (cd $MEMBER && RUSTUP_TOOLCHAIN=1.100.0 ftrace rustc --version >/dev/null 2>&1); echo "    probes for rust-toolchain* with the variable set: $(grep -c 'rust-toolchain' $FTRACE_OUT)"
echo; echo "## F. [toolchain] path = <directory in the tree>: the file names the binaries to run"
clean; mkdir -p $REPO/evil-tc/bin; printf '#!/bin/sh\necho "PWNED: this is evil-tc/bin/$(basename $0), run with args: $*"\n' > $REPO/evil-tc/bin/rustc; cp $REPO/evil-tc/bin/rustc $REPO/evil-tc/bin/cargo; chmod +x $REPO/evil-tc/bin/*
printf '[toolchain]\npath = "evil-tc"\n' > $REPO/rust-toolchain.toml
echo "# F0a: repo/rust-toolchain.toml: [toolchain] path = \"evil-tc\" (relative)"; (cd $REPO && rustc --version 2>&1) | strip | sed 's/^/    /'
printf '[toolchain]\npath = "./evil-tc"\n' > $REPO/rust-toolchain.toml
echo "# F0b: path = \"./evil-tc\" (relative)"; (cd $REPO && rustc --version 2>&1) | strip | sed 's/^/    /'
printf '[toolchain]\npath = "%s/evil-tc"\n' $REPO > $REPO/rust-toolchain.toml
echo "# F1: path = \"<absolute path of repo>/evil-tc\" (evil-tc holds only bin/rustc and bin/cargo, two shell scripts)"; (cd $REPO && rustc --version 2>&1; cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1; echo "[exit $?]"; $RU show active-toolchain 2>&1) | strip | sed 's/^/    /'
echo "# F2: the same content in the legacy name repo/rust-toolchain, cwd = member"; mv $REPO/rust-toolchain.toml $REPO/rust-toolchain; (cd $MEMBER && cargo test --workspace --locked 2>&1; echo "[exit $?]") | strip | sed 's/^/    /'
echo "# F3: with RUSTUP_TOOLCHAIN=1.100.0"; (cd $REPO && RUSTUP_TOOLCHAIN=1.100.0 cargo --version 2>&1) | strip | sed 's/^/    /'
echo "# F4: with cargo +1.100.0"; (cd $REPO && cargo +1.100.0 --version 2>&1) | strip | sed 's/^/    /'
echo; echo "## G. a file naming a toolchain that is not installed"
clean; tomlput $REPO/rust-toolchain.toml 1.0.99
echo "# G1: default settings (channel = \"1.0.99\", which does not exist)"; (cd $REPO && rustc --version 2>&1 | head -6) | strip | sed 's/^/    /'
echo "# G2: RUSTUP_AUTO_INSTALL=0"; (cd $REPO && RUSTUP_AUTO_INSTALL=0 rustc --version 2>&1 | head -6) | strip | sed 's/^/    /'
clean; printf '[toolchain]\nchannel = "canary-a"\ncomponents = ["no-such-component"]\ntargets = ["wasm32-unknown-unknown"]\n' > $REPO/rust-toolchain.toml
echo "# G3: an installed custom toolchain plus components/targets keys"; (cd $REPO && rustc --version 2>&1 | head -6) | strip | sed 's/^/    /'
echo; echo "## H. file-lookup trace from member/src with no file present: every toolchain-file name probed"
clean; export FTRACE_OUT=$S/out/04-rustup.trace; rm -f $FTRACE_OUT
(cd $MEMBER/src && ftrace rustc --version >/dev/null 2>&1)
grep -vE '/toolchains/|/usr/|/System/|/dev/' $FTRACE_OUT | awk '{ $1=""; print }' | strip | sort | uniq -c
