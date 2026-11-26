#!/bin/zsh
# clippy: which files it reads unasked, precedence, CLIPPY_CONF_DIR, other inputs.
source ${0:A:h}/env.sh
mkfx clippy
echo "## versions"; cargo --version; cargo clippy --version
printf 'pub fn f() -> Option<String> {\n    std::env::var("X").ok()\n}\n' > $MEMBER/src/lib.rs
ALL=( $REPO/clippy.toml $REPO/.clippy.toml $MEMBER/clippy.toml $MEMBER/.clippy.toml $MEMBER/src/clippy.toml $MEMBER/src/.clippy.toml
      $OUTER/clippy.toml $OUTER/.clippy.toml $FX/clippy.toml $HOME/clippy.toml $HOME/.clippy.toml $HOME/.config/clippy.toml $HOME/.config/clippy/clippy.toml
      $REPO/.config/clippy.toml $REPO/.cargo/clippy.toml $REPO/.clippy $REPO/Clippy.toml )
clean() { local g; for g in $ALL; do rm -f "$g"; done; rm -rf $REPO/.cargo; }
# put FILE ID: FILE disallows std::env::var with reason "CANARY-ID"; the source calls it, so the lint fires and names the file
put() { mkdir -p "${1:h}"; printf 'disallowed-methods = [{ path = "std::env::var", reason = "CANARY-%s" }]\n' $2 > "$1"; }
empty() { mkdir -p "${1:h}"; printf '# empty\n' > "$1"; }
N=0
# lint [DIR] [env assignments...]: the project's command, fresh target dir each time so nothing is cached
lint() {
  local dir=$1; shift; N=$((N+1))
  echo "\$ (cd ${dir#$FX/} && $* cargo clippy --workspace --all-targets --locked -- -D warnings)" | strip
  local out rc; out=$(cd $dir && env CARGO_TARGET_DIR=$S/target/clippy/$N "$@" cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1); rc=$?
  echo "$out" | strip | grep -E '^(warning|error)|CANARY|will be ignored|clippy\.toml' | grep -v 'could not compile' | sort -u | sed 's/^/    /'
  echo "  -> exit $rc"
}
echo; echo "## A. baseline: no clippy configuration anywhere"; clean; lint $REPO
echo; echo "## B. one candidate at a time"
n=0; for f in $ALL; do n=$((n+1)); clean; put "$f" $n; echo; echo "# only $f (CANARY-$n)" | strip; lint $REPO; done
echo; echo "## C. same directory: repo/clippy.toml (CANARY-protected) and repo/.clippy.toml"
clean; put $REPO/clippy.toml protected; put $REPO/.clippy.toml dotfile; echo "# both carry a canary"; lint $REPO
echo "# full output of that run:"; (cd $REPO && CARGO_TARGET_DIR=$S/target/clippy/c-full cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1 | strip)
clean; put $REPO/clippy.toml protected; empty $REPO/.clippy.toml; echo "# repo/.clippy.toml is empty (the attack: switch the protected rules off)"; lint $REPO
echo "# full output of that run:"; (cd $REPO && CARGO_TARGET_DIR=$S/target/clippy/c-full2 cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1 | strip; echo "[exit ${pipestatus[1]}]")
echo; echo "## D. across directories"
clean; put $REPO/clippy.toml protected; empty $MEMBER/.clippy.toml; echo "# repo/clippy.toml=CANARY-protected, member/.clippy.toml empty"; lint $REPO
clean; put $REPO/clippy.toml protected; empty $MEMBER/clippy.toml; echo "# repo/clippy.toml=CANARY-protected, member/clippy.toml empty"; lint $REPO
clean; put $REPO/clippy.toml protected; put $MEMBER/src/.clippy.toml src; echo "# repo/clippy.toml=CANARY-protected, member/src/.clippy.toml=CANARY-src (not a manifest directory)"; lint $REPO
clean; put $REPO/clippy.toml protected; put $OUTER/.clippy.toml outer; echo "# repo/clippy.toml=CANARY-protected, outer/.clippy.toml=CANARY-outer"; lint $REPO
clean; put $OUTER/clippy.toml outer; printf 'too-many-arguments-threshold = 1\n' > $MEMBER/.clippy.toml; echo "# are files merged? outer/clippy.toml=CANARY-outer, member/.clippy.toml sets another key only"; lint $REPO
echo; echo "## E. other working directories"
clean; put $REPO/.clippy.toml root-dot; echo "# from the member directory"; lint $MEMBER
echo "# from outside the tree with --manifest-path"; N=$((N+1)); (cd $S && CARGO_TARGET_DIR=$S/target/clippy/$N cargo clippy --manifest-path $REPO/Cargo.toml --workspace --all-targets --locked -- -D warnings 2>&1 | strip | grep -E 'CANARY|^error' | sort -u; echo "[exit ${pipestatus[1]}]")
echo; echo "## F. CLIPPY_CONF_DIR"
clean; put $REPO/clippy.toml protected; empty $REPO/.clippy.toml; echo "# F1: CLIPPY_CONF_DIR=repo, repo/clippy.toml=CANARY-protected, repo/.clippy.toml empty"; lint $REPO CLIPPY_CONF_DIR=$REPO
clean; put $REPO/clippy.toml protected; empty $MEMBER/.clippy.toml; echo "# F2: CLIPPY_CONF_DIR=repo, repo/clippy.toml=CANARY-protected, member/.clippy.toml empty"; lint $REPO CLIPPY_CONF_DIR=$REPO
clean; mkdir -p $FX/trusted; put $FX/trusted/clippy.toml trusted; empty $REPO/.clippy.toml; empty $MEMBER/.clippy.toml; empty $REPO/clippy.toml; echo "# F3: CLIPPY_CONF_DIR=<dir outside the tree> holding clippy.toml=CANARY-trusted; empty .clippy.toml/clippy.toml in repo and member"; lint $REPO CLIPPY_CONF_DIR=$FX/trusted
clean; rm -f $FX/trusted/clippy.toml; put $OUTER/.clippy.toml outer-of-confdir; mkdir -p $OUTER/confdir; echo "# F4: CLIPPY_CONF_DIR=outer/confdir holds no file; outer/.clippy.toml=CANARY-outer-of-confdir (upward search from the named directory?)"; lint $REPO CLIPPY_CONF_DIR=$OUTER/confdir
clean; put $FX/trusted/clippy.toml trusted; mkdir -p $REPO/.cargo $REPO/evil; empty $REPO/evil/clippy.toml; printf '[env]\nCLIPPY_CONF_DIR = { value = "evil", relative = true }\n' > $REPO/.cargo/config.toml
echo "# F5: CLIPPY_CONF_DIR=trusted in the environment AND repo/.cargo/config.toml [env] CLIPPY_CONF_DIR=evil (no force)"; lint $REPO CLIPPY_CONF_DIR=$FX/trusted
printf '[env]\nCLIPPY_CONF_DIR = { value = "evil", relative = true, force = true }\n' > $REPO/.cargo/config.toml
echo "# F6: same with force = true"; lint $REPO CLIPPY_CONF_DIR=$FX/trusted
clean; put $REPO/clippy.toml protected; mkdir -p $REPO/.cargo $REPO/evil; empty $REPO/evil/clippy.toml; printf '[env]\nCLIPPY_CONF_DIR = { value = "evil", relative = true }\n' > $REPO/.cargo/config.toml
echo "# F7: no variable in the environment; repo/.cargo/config.toml [env] CLIPPY_CONF_DIR=evil (empty clippy.toml there); repo/clippy.toml=CANARY-protected"; lint $REPO
rm -rf $REPO/evil
echo; echo "## G. other inputs that switch the lint off (repo/clippy.toml=CANARY-protected throughout)"
clean; put $REPO/clippy.toml protected
printf '\n[lints.clippy]\ndisallowed_methods = "allow"\n' >> $MEMBER/Cargo.toml; echo "# G1: member/Cargo.toml [lints.clippy] disallowed_methods = \"allow\""; lint $REPO
sed -i '' '/^\[lints.clippy\]/,$d' $MEMBER/Cargo.toml
printf '\n[lints]\nworkspace = true\n' >> $MEMBER/Cargo.toml; printf '\n[workspace.lints.clippy]\ndisallowed_methods = "allow"\n' >> $REPO/Cargo.toml; echo "# G2: [workspace.lints.clippy] in the root Cargo.toml, inherited"; lint $REPO
sed -i '' '/^\[lints\]/,$d' $MEMBER/Cargo.toml; sed -i '' '/^\[workspace.lints.clippy\]/,$d' $REPO/Cargo.toml
mkdir -p $REPO/.cargo; printf '[build]\nrustflags = ["-Aclippy::disallowed_methods"]\n' > $REPO/.cargo/config.toml; echo "# G3: .cargo/config.toml [build] rustflags = [\"-Aclippy::disallowed_methods\"]"; lint $REPO
printf '[build]\nrustflags = ["--cap-lints", "allow"]\n' > $REPO/.cargo/config.toml; echo "# G4: .cargo/config.toml [build] rustflags = [\"--cap-lints\", \"allow\"]"; lint $REPO
mv $REPO/.cargo/config.toml $REPO/.cargo/config; echo "# G5: the same in the legacy file name .cargo/config"; lint $REPO
rm -rf $REPO/.cargo; mkdir -p $MEMBER/.cargo; printf '[build]\nrustflags = ["--cap-lints", "allow"]\n' > $MEMBER/.cargo/config.toml; echo "# G6: member/.cargo/config.toml, run from the workspace root, then from the member directory"; lint $REPO; lint $MEMBER
rm -rf $MEMBER/.cargo; mkdir -p $OUTER/.cargo; printf '[build]\nrustflags = ["--cap-lints", "allow"]\n' > $OUTER/.cargo/config.toml; echo "# G7: outer/.cargo/config.toml (above the git root)"; lint $REPO
rm -rf $OUTER/.cargo
printf '#![allow(clippy::disallowed_methods)]\npub fn f() -> Option<String> {\n    std::env::var("X").ok()\n}\n' > $MEMBER/src/lib.rs; echo "# G8: #![allow(clippy::disallowed_methods)] in the source"; lint $REPO
printf 'pub fn f() -> Option<String> {\n    std::env::var("X").ok()\n}\n' > $MEMBER/src/lib.rs
mkdir -p $REPO/.cargo; printf '[alias]\nclippy = "check"\n' > $REPO/.cargo/config.toml; echo "# G9: .cargo/config.toml [alias] clippy = \"check\""; (cd $REPO && CARGO_TARGET_DIR=$S/target/clippy/g9 cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1 | strip | head -8; echo "[exit ${pipestatus[1]}]")
rm -rf $REPO/.cargo
echo; echo "## H. does a cached build notice a NEW .clippy.toml? (same target dir, source untouched)"
clean; put $REPO/clippy.toml protected
(cd $REPO && export CARGO_TARGET_DIR=$S/target/clippy/h; cargo clippy --workspace --all-targets --locked -- -D warnings >/dev/null 2>&1; echo "run 1 (clippy.toml only): exit $?"
 empty $REPO/.clippy.toml; cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1 | strip | grep -E 'CANARY|ignored|Checking|Finished' | sort -u; echo "run 2 (after adding empty .clippy.toml): exit ${pipestatus[1]}"
 rm $REPO/.clippy.toml; cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1 | strip | grep -E 'CANARY|ignored|Checking|Finished' | sort -u; echo "run 3 (after removing it again): exit ${pipestatus[1]}"
 empty $MEMBER/.clippy.toml; cargo clippy --workspace --all-targets --locked -- -D warnings 2>&1 | strip | grep -E 'CANARY|ignored|Checking|Finished' | sort -u; echo "run 4 (after adding empty member/.clippy.toml): exit ${pipestatus[1]}")
echo; echo "## I. file-lookup trace, no config present: every clippy.toml-like name probed"
clean; export FTRACE_OUT=$S/out/02-clippy.trace; rm -f $FTRACE_OUT
(cd $REPO && CARGO_TARGET_DIR=$S/target/clippy/trace ftrace cargo clippy --workspace --all-targets --locked -- -D warnings >/dev/null 2>&1)
grep -iE 'clippy[^/]*$' $FTRACE_OUT | grep -vE '/bin/|/target/' | awk '{ $1=""; print }' | strip | sort | uniq -c
rm -rf $S/target/clippy
