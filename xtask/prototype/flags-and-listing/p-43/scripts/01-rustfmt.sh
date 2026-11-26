#!/bin/zsh
# rustfmt through `cargo fmt`: which files it reads unasked, precedence, explicit arguments.
source ${0:A:h}/env.sh
mkfx rustfmt
echo "## versions"; cargo --version; cargo fmt --version; rustfmt --version
APPSUP="$HOME/Library/Application Support/rustfmt"
ALL=( $REPO/rustfmt.toml $REPO/.rustfmt.toml $MEMBER/rustfmt.toml $MEMBER/.rustfmt.toml $MEMBER/src/rustfmt.toml $MEMBER/src/.rustfmt.toml
      $OUTER/rustfmt.toml $OUTER/.rustfmt.toml $FX/rustfmt.toml $HOME/rustfmt.toml $HOME/.rustfmt.toml $HOME/.config/rustfmt/rustfmt.toml $HOME/.config/rustfmt/.rustfmt.toml
      $HOME/.config/rustfmt.toml "$APPSUP/rustfmt.toml" "$APPSUP/.rustfmt.toml" $REPO/.config/rustfmt.toml $REPO/.cargo/rustfmt.toml $REPO/.rustfmt $REPO/rustfmt $REPO/rustfmt.TOML )
clean() { local g; for g in $ALL; do rm -f "$g"; done }
# put FILE N: FILE gets tab_spaces = N (the canary: the source is indented by 4, so any N != 4 fails --check and the diff shows N spaces)
put() { mkdir -p "${1:h}"; echo "tab_spaces = $2" > "$1"; }
# check [DIR] [extra args]: run cargo fmt --all --check -v from DIR and summarise
check() {
  local dir=$1; shift
  echo "\$ (cd ${dir#$FX/} && cargo fmt --all --check $*)" | strip
  local out; out=$(cd $dir && cargo fmt --all --check "$@" 2>&1); local rc=$?
  out=$(echo "$out" | strip)
  echo "$out" | grep -E '^(Using rustfmt config|Warning|warning|error|Error)'
  local plus; plus=$(echo "$out" | grep -E '^\+ +1' | head -1 | sed -E 's/^\+( +)1.*/\1/')
  if [ -n "$plus" ]; then echo "  -> exit $rc; rustfmt wants an indent of ${#plus} spaces"; else echo "  -> exit $rc; no space-indent diff"; fi
}
echo; echo "## A. baseline: no configuration file anywhere"
clean; check $REPO -- -v
echo; echo "## B. one candidate at a time (workspace root as cwd). tab_spaces value identifies the file."
n=10
for f in $ALL; do n=$((n+1)); clean; put "$f" $n; echo; echo "# only ${${f#$FX/}/$HOME/\$HOME} (tab_spaces = $n)"; check $REPO -- -v; done
echo; echo "## C. precedence, same directory: repo/rustfmt.toml=2 and repo/.rustfmt.toml=3"
clean; put $REPO/rustfmt.toml 2; put $REPO/.rustfmt.toml 3; check $REPO -- -v
echo "# raw output of the same run:"; (cd $REPO && cargo fmt --all --check 2>&1 | sed "s|$FX/||g")
echo; echo "## D. precedence across directories"
echo "# repo/rustfmt.toml=2, member/.rustfmt.toml=3"; clean; put $REPO/rustfmt.toml 2; put $MEMBER/.rustfmt.toml 3; check $REPO -- -v
echo "# repo/rustfmt.toml=2, member/src/.rustfmt.toml=3"; clean; put $REPO/rustfmt.toml 2; put $MEMBER/src/.rustfmt.toml 3; check $REPO -- -v
echo "# repo/rustfmt.toml=2, outer/.rustfmt.toml=3"; clean; put $REPO/rustfmt.toml 2; put $OUTER/.rustfmt.toml 3; check $REPO -- -v
echo "# outer/rustfmt.toml=2, \$HOME/.rustfmt.toml=3"; clean; put $OUTER/rustfmt.toml 2; put $HOME/.rustfmt.toml 3; check $REPO -- -v
echo "# is the nearer file merged with the farther one? repo/rustfmt.toml: max_width=20 ; member/.rustfmt.toml: tab_spaces=3"
clean; echo 'hard_tabs = true' > $REPO/rustfmt.toml; put $MEMBER/.rustfmt.toml 3; (cd $REPO && cargo fmt --all --check 2>&1 | strip | cat -t; echo "[exit ${pipestatus[1]}]")
echo '# control: hard_tabs = true alone in repo/rustfmt.toml'; clean; echo 'hard_tabs = true' > $REPO/rustfmt.toml; (cd $REPO && cargo fmt --all --check 2>&1 | strip | cat -t; echo "[exit ${pipestatus[1]}]")
echo; echo "## E. run from the member directory, and with --manifest-path from outside the tree"
clean; put $REPO/.rustfmt.toml 3; check $MEMBER -- -v
echo "\$ (cd $S && cargo fmt --all --check --manifest-path repo/Cargo.toml -- -v)"; (cd $S && cargo fmt --all --check --manifest-path $REPO/Cargo.toml -- -v 2>&1 | strip | grep -E 'Using|^\+'; echo "[exit ${pipestatus[1]}]")
echo; echo "## F. explicit arguments with a sibling present"
echo "# F1: --config-path repo/rustfmt.toml(=4, the trusted file) while member/.rustfmt.toml=3 and repo/.rustfmt.toml=5 exist"
clean; put $REPO/rustfmt.toml 4; put $MEMBER/.rustfmt.toml 3; put $REPO/.rustfmt.toml 5
check $REPO -- -v
check $REPO -- -v --config-path $REPO/rustfmt.toml
check $REPO -- -v --config-path $REPO
echo "# F2: --config tab_spaces=4 (inline key) while repo/.rustfmt.toml=5 and member/.rustfmt.toml: max_width=20"
clean; put $REPO/.rustfmt.toml 5; echo 'hard_tabs = true' > $MEMBER/.rustfmt.toml
(cd $REPO && cargo fmt --all --check -- --config tab_spaces=4 2>&1 | strip | cat -t; echo "[exit ${pipestatus[1]}]")
echo "# F3: --config-path pointing at a file outside the tree, siblings in the tree"
clean; put $REPO/.rustfmt.toml 5; put $MEMBER/src/rustfmt.toml 6; mkdir -p $FX/trusted; echo 'tab_spaces = 4' > $FX/trusted/rustfmt.toml
check $REPO -- -v --config-path $FX/trusted/rustfmt.toml
echo; echo "## G. other inputs"
echo "# G1: a file no target reaches (member/src/orphan.rs, badly formatted, no 'mod orphan;') is not checked"
clean; printf 'pub fn g(){let x=1;x}\n' > $MEMBER/src/orphan.rs; (cd $REPO && cargo fmt --all --check; echo "[exit $?]")
echo "# G2: #[rustfmt::skip] on the item"; rm $MEMBER/src/orphan.rs; printf '#[rustfmt::skip]\npub fn f() -> u32 {\n1\n}\n' > $MEMBER/src/lib.rs; (cd $REPO && cargo fmt --all --check; echo "[exit $?]")
printf 'pub fn f() -> u32 {\n    1\n}\n' > $MEMBER/src/lib.rs
echo "# G3: unstable key 'ignore' in .rustfmt.toml on the stable toolchain"
clean; printf 'ignore = ["member/src/lib.rs"]\n' > $REPO/.rustfmt.toml; printf 'pub fn f() -> u32 {\n1\n}\n' > $MEMBER/src/lib.rs; (cd $REPO && cargo fmt --all --check 2>&1 | strip | cat -t; echo "[exit ${pipestatus[1]}]")
echo "# G4: disable_all_formatting = true in .rustfmt.toml (stable key), source badly formatted"
clean; printf 'disable_all_formatting = true\n' > $REPO/.rustfmt.toml; (cd $REPO && cargo fmt --all --check 2>&1 | strip | cat -t; echo "[exit ${pipestatus[1]}]")
echo "# G5: RUSTFMT set through .cargo/config.toml [env] (does cargo fmt see it?)"
clean; mkdir -p $REPO/.cargo; printf '[env]\nRUSTFMT = "/usr/bin/true"\n' > $REPO/.cargo/config.toml; (cd $REPO && cargo fmt --all --check 2>&1 | strip | head -8; echo "[exit ${pipestatus[1]}]")
echo "# G5b: RUSTFMT=/usr/bin/true in the real environment (control: the variable works)"; (cd $REPO && RUSTFMT=/usr/bin/true cargo fmt --all --check 2>&1 | strip | head -8; echo "[exit ${pipestatus[1]}]")
echo "# G6: [alias] fmt = \"--version\" in .cargo/config.toml"
printf '[alias]\nfmt = "--version"\n' > $REPO/.cargo/config.toml; (cd $REPO && cargo fmt --all --check 2>&1 | strip | head -8; echo "[exit ${pipestatus[1]}]")
rm -rf $REPO/.cargo
printf 'pub fn f() -> u32 {\n    1\n}\n' > $MEMBER/src/lib.rs
echo; echo "## H. file-lookup trace (interposed open/stat), no config present: every rustfmt.toml-like name probed"
clean; export FTRACE_OUT=$S/out/01-rustfmt.trace; rm -f $FTRACE_OUT
(cd $REPO && ftrace cargo fmt --all --check)
grep -iE 'rustfmt[^/]*$' $FTRACE_OUT | grep -v '/bin/' | awk '{ $1=""; print }' | sed "s|$FX/||g; s|$HOME|\$HOME|g" | sort | uniq -c
