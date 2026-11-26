#!/bin/zsh
# cargo-nextest: configuration files read unasked, what --config-file does, other inputs.
source ${0:A:h}/env.sh
mkfx nextest
export CARGO_TARGET_DIR=$S/target/nextest
echo "## versions"; cargo nextest --version | head -1; cargo --version
printf 'pub fn f() -> u32 {\n    1\n}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn passes() {\n        assert_eq!(super::f(), 1);\n    }\n    #[test]\n    fn fails() {\n        assert_eq!(super::f(), 2);\n    }\n}\n' > $MEMBER/src/lib.rs
APPSUP="$HOME/Library/Application Support/nextest"
ALL=( $REPO/.config/nextest.toml $REPO/nextest.toml $REPO/.nextest.toml $REPO/.config/nextest/config.toml $REPO/.cargo/nextest.toml $REPO/.nextest/config.toml
      $MEMBER/.config/nextest.toml $MEMBER/nextest.toml $OUTER/.config/nextest.toml $FX/.config/nextest.toml $HOME/.config/nextest.toml $HOME/.config/nextest/config.toml "$APPSUP/config.toml" $REPO/.config/Nextest.TOML )
clean() { local g; for g in $ALL; do rm -f "${g:?}"; done; rm -rf "${REPO:?}/.cargo" "${REPO:?}/.config" "${MEMBER:?}/.config" "${OUTER:?}/.config"; }
# put FILE: the canary. default-filter keeps only the passing test, so a run that would fail (exit 100) passes (exit 0) when FILE is read.
put() { mkdir -p "${1:h}"; printf '[profile.default]\ndefault-filter = "test(passes)"\n' > "$1"; }
go() { local dir=$1; shift; echo "\$ (cd ${dir#$FX/} && $*)" | strip; (cd $dir && "$@" 2>&1 | strip | grep -E '^ *(Summary|error|warning|Cancelling|info)|SETUP|WRAPPER|PWNED|^error' | sort -u | head -8 | sed 's/^/    /'; echo "    -> exit ${pipestatus[1]}"); }
NX=(cargo nextest run --workspace --locked)
echo; echo "## A. baseline (one passing, one failing test; no config)"; clean; go $REPO $NX
echo; echo "## B. one candidate at a time, cwd = workspace root"
for f in $ALL; do clean; put "$f"; echo; echo "# only $f" | strip; go $REPO $NX; done
echo; echo "## C. cwd = member, and --manifest-path from outside"
clean; put $REPO/.config/nextest.toml; echo "# repo/.config/nextest.toml present"; go $MEMBER $NX; go $S cargo nextest run --workspace --locked --manifest-path $REPO/Cargo.toml
clean; put $MEMBER/.config/nextest.toml; echo "# only member/.config/nextest.toml, cwd = member"; go $MEMBER $NX
echo; echo "## D. explicit arguments with repo/.config/nextest.toml (the canary) present"
clean; put $REPO/.config/nextest.toml; mkdir -p $FX/trusted; printf '[profile.default]\nfail-fast = true\n' > $FX/trusted/nextest.toml
echo "# D1 --config-file <file outside the tree>"; go $REPO $NX --config-file $FX/trusted/nextest.toml
echo "# D2 --ignore-default-filter"; go $REPO $NX --ignore-default-filter
echo "# D3 --tool-config-file t:<trusted file> (tool config sits below the repository config)"; go $REPO $NX --tool-config-file t:$FX/trusted/nextest.toml
echo "# D4 NEXTEST_CONFIG_FILE=<trusted> (is there such a variable?)"; go $REPO env NEXTEST_CONFIG_FILE=$FX/trusted/nextest.toml $NX
echo "# D5 -E 'all()' on the command line"; go $REPO $NX -E 'all()'
echo "# D6 --profile ci where repo/.config/nextest.toml defines [profile.ci] default-filter"; printf '[profile.ci]\ndefault-filter = "test(passes)"\n' > $REPO/.config/nextest.toml; go $REPO $NX --profile ci
echo; echo "## E. other keys of the repository config"
clean; mkdir -p $REPO/.config
printf '[[profile.default.overrides]]\nfilter = "test(fails)"\nretries = 2\n' > $REPO/.config/nextest.toml; echo "# E1 per-test override (retries; shows the file steers single tests)"; go $REPO $NX
printf 'experimental = ["setup-scripts"]\n[scripts.setup.s]\ncommand = ["/bin/sh", "-c", "echo SETUP-SCRIPT-RAN >&2"]\n[[profile.default.scripts]]\nfilter = "all()"\nsetup = "s"\n' > $REPO/.config/nextest.toml; echo "# E2 a setup script declared and switched on by the file itself"; go $REPO $NX
printf 'experimental = ["wrapper-scripts"]\n[scripts.wrapper.w]\ncommand = "/usr/bin/true"\n[[profile.default.scripts]]\nfilter = "all()"\nrun-wrapper = "w"\n' > $REPO/.config/nextest.toml; echo "# E3 a wrapper script /usr/bin/true around every test"; go $REPO $NX
printf 'nextest-version = { required = "99.0.0" }\n' > $REPO/.config/nextest.toml; echo "# E4 nextest-version required = 99.0.0"; go $REPO $NX
printf '[profile.default]\ndefault-filter = "test(passes)"\nno_such_key = 1\n' > $REPO/.config/nextest.toml; echo "# E5 an unknown key in the file"; go $REPO $NX
echo; echo "## F. inputs that are not nextest's own file"
clean; mkdir -p $REPO/.cargo; printf '[target.aarch64-apple-darwin]\nrunner = "/usr/bin/true"\n' > $REPO/.cargo/config.toml; echo "# F1 repo/.cargo/config.toml [target.aarch64-apple-darwin] runner = \"/usr/bin/true\""; go $REPO $NX
printf '[env]\nNEXTEST_RETRIES = "3"\nNEXTEST_NO_TESTS = "pass"\n' > $REPO/.cargo/config.toml; echo "# F2 repo/.cargo/config.toml [env] NEXTEST_* (does nextest take its own options from cargo's [env]?)"; go $REPO $NX
rm -rf "${REPO:?}/.cargo"
printf '\n[workspace.metadata.nextest]\ndefault-filter = "test(passes)"\n[workspace.metadata.nextest.profile.default]\ndefault-filter = "test(passes)"\n' >> $REPO/Cargo.toml; printf '\n[package.metadata.nextest]\ndefault-filter = "test(passes)"\n[package.metadata.nextest.profile.default]\ndefault-filter = "test(passes)"\n' >> $MEMBER/Cargo.toml
echo "# F3 [workspace.metadata.nextest] / [package.metadata.nextest] tables in Cargo.toml"; go $REPO $NX
sed -i '' '/^\[workspace.metadata.nextest\]/,$d' $REPO/Cargo.toml; sed -i '' '/^\[package.metadata.nextest\]/,$d' $MEMBER/Cargo.toml
echo "# F4 user config: \$HOME/.config/nextest/config.toml with a repository-style key (default-filter)"; clean; put $HOME/.config/nextest/config.toml; go $REPO $NX; rm -rf "${S:?}/home/.config/nextest"
echo; echo "## G. file-lookup trace, no config: nextest-named paths probed"
clean; export FTRACE_OUT=$S/out/06-nextest.trace; rm -f "${FTRACE_OUT:?}"
(cd $REPO && ftrace cargo nextest run --workspace --locked >/dev/null 2>&1)
grep -E 'nextest\.toml|nextest/config\.toml|/\.config/' $FTRACE_OUT | grep -vE '/target/|tools/bin|/toolchains/|/git/' | awk '{ $1=""; print }' | strip | sort | uniq -c | sed 's/^/    /'
