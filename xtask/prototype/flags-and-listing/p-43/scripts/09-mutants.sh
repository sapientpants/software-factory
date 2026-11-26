#!/bin/zsh
# cargo-mutants: configuration files read unasked, what --config / --no-config do, other inputs.
source ${0:A:h}/env.sh
mkfx mutants
export CARGO_TARGET_DIR=$S/target/mutants
echo "## versions"; cargo mutants --version; cargo --version
printf 'pub fn f(a: u32) -> u32 {\n    a + 1\n}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(super::f(1), 2);\n    }\n}\n' > $MEMBER/src/lib.rs
ALL=( $REPO/.cargo/mutants.toml $REPO/mutants.toml $REPO/.mutants.toml $REPO/.config/mutants.toml $REPO/.cargo/mutants $REPO/cargo-mutants.toml $MEMBER/.cargo/mutants.toml $MEMBER/mutants.toml $OUTER/.cargo/mutants.toml $HOME/.cargo/mutants.toml $HOME/.config/mutants.toml $HOME/.config/cargo-mutants/mutants.toml $REPO/.cargo/Mutants.TOML )
clean() { local g; for g in $ALL; do rm -f "${g:?}"; done; rm -rf "${REPO:?}/.config" "${REPO:?}/.cargo" "${MEMBER:?}/.cargo" "${OUTER:?}/.cargo" "${REPO:?}/mutants.out" "${MEMBER:?}/mutants.out"; }
# put FILE: the canary excludes every source file, so `cargo mutants --list` prints nothing when FILE is read
put() { mkdir -p "${1:h}"; printf 'exclude_globs = ["**/*.rs"]\n' > "$1"; }
go() { local dir=$1; shift; echo "\$ (cd ${dir#$FX/} && cargo mutants $*)" | strip; (cd $dir && cargo mutants "$@" > $S/out/09.raw 2>&1; local rc=$?; strip < $S/out/09.raw > $S/out/09.tmp; echo "    -> exit $rc; mutants listed: $(grep -c 'replace ' $S/out/09.tmp)$(grep -E '^(error|Error|warning|WARN)|caught|missed|PWNED' $S/out/09.tmp | head -4 | sed 's/^/ ; /' | tr -d '\n')"); }
echo; echo "## A. baseline"; clean; go $REPO --list
echo; echo "## B. one candidate at a time, cwd = workspace root"
for f in $ALL; do clean; put "$f"; echo "# only $f" | strip; go $REPO --list; done
echo; echo "## C. other working directories (repo/.cargo/mutants.toml is the canary)"
clean; put $REPO/.cargo/mutants.toml; echo "# cwd = member"; go $MEMBER --list; echo "# cwd outside, --dir repo"; go $S --dir $REPO --list; echo "# cwd outside, --manifest-path"; go $S --manifest-path $REPO/Cargo.toml --list
clean; put $MEMBER/.cargo/mutants.toml; echo "# only member/.cargo/mutants.toml, cwd = member"; go $MEMBER --list
echo; echo "## D. explicit arguments with repo/.cargo/mutants.toml (canary) present"
clean; put $REPO/.cargo/mutants.toml; mkdir -p $FX/trusted; printf 'timeout_multiplier = 3.0\n' > $FX/trusted/mutants.toml
echo "# D1 --no-config"; go $REPO --no-config --list
echo "# D2 --config <file outside the tree>"; go $REPO --config $FX/trusted/mutants.toml --list
echo "# D3 --config <file that does not exist>"; go $REPO --config $FX/trusted/nope.toml --list
echo "# D4 -f '**/*.rs' on the command line (does an explicit --file beat exclude_globs of the file?)"; go $REPO -f '**/*.rs' --list
echo; echo "## E. other inputs"
clean; printf '\n[package.metadata.mutants]\nexclude_globs = ["**/*.rs"]\n[package.metadata.cargo-mutants]\nexclude_globs = ["**/*.rs"]\n' >> $MEMBER/Cargo.toml; printf '\n[workspace.metadata.mutants]\nexclude_globs = ["**/*.rs"]\n[workspace.metadata.cargo-mutants]\nexclude_globs = ["**/*.rs"]\n' >> $REPO/Cargo.toml
echo "# E1 [package.metadata.*] / [workspace.metadata.*] tables in Cargo.toml"; go $REPO --list
sed -i '' '/^\[package.metadata.mutants\]/,$d' $MEMBER/Cargo.toml; sed -i '' '/^\[workspace.metadata.mutants\]/,$d' $REPO/Cargo.toml
printf '#[mutants::skip]\npub fn f(a: u32) -> u32 {\n    a + 1\n}\n' > $MEMBER/src/lib.rs; echo "# E2 #[mutants::skip] on the function (no dependency on the mutants crate declared)"; go $REPO --list
printf 'pub fn f(a: u32) -> u32 {\n    a + 1\n}\n' > $MEMBER/src/lib.rs
echo "# E3 a real run, no test covers f: baseline verdict"; go $REPO
clean; mkdir -p $REPO/.cargo; printf 'additional_cargo_test_args = ["--", "--list"]\nerror_values = []\ntest_tool = "cargo"\n' > $REPO/.cargo/mutants.toml; echo "# E3b a config that only changes how tests are run (additional_cargo_test_args)"; go $REPO
clean; mkdir -p $REPO/.cargo; printf 'exclude_re = [".*"]\n' > $REPO/.cargo/mutants.toml; echo "# E3c a real run with exclude_re = [\".*\"]"; go $REPO
clean; echo 'member/src/lib.rs' > $REPO/.gitignore; echo "# E4 .gitignore lists member/src/lib.rs (--list, then a real run)"; go $REPO --list; go $REPO; rm -f "${REPO:?}/.gitignore"
clean; echo 'member/src/lib.rs' > $REPO/.ignore; echo "# E5 .ignore lists member/src/lib.rs"; go $REPO --list; rm -f "${REPO:?}/.ignore"
echo; echo "## F. file-lookup trace of 'cargo mutants --list', no config, cwd = member: mutants-named paths probed"
clean; export FTRACE_OUT=$S/out/09-mutants.trace; rm -f "${FTRACE_OUT:?}"
(cd $MEMBER && ftrace cargo mutants --list >/dev/null 2>&1)
grep -iE 'mutants' $FTRACE_OUT | grep -vE 'tools/bin|/target/' | awk '{ $1=""; print }' | strip | sort | uniq -c | sed 's/^/    /'
clean; rm -f "${S:?}/out/09.tmp" "${S:?}/out/09.raw"
