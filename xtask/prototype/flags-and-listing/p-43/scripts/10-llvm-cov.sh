#!/bin/zsh
# cargo-llvm-cov: inputs read unasked. Runs against the scratch rustup home, the only one with llvm-tools installed
# (cargo-llvm-cov would otherwise ask rustup to add the component to the user's toolchain).
source ${0:A:h}/env.sh
export RUSTUP_HOME=$S/rustup-home RUSTUP_TOOLCHAIN=1.100.0 CARGO_TARGET_DIR=$S/target/llvmcov
mkfx llvmcov
echo "## versions"; cargo llvm-cov --version; cargo --version; rustc --version
src() { printf 'pub fn covered() -> u32 {\n    1\n}\n%spub fn uncovered() -> u32 {\n    2\n}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(super::covered(), 1);\n    }\n}\n' "$1" > $MEMBER/src/lib.rs; }
clean() { rm -rf "${REPO:?}/.cargo" "${REPO:?}/.config"; local g; for g in llvm-cov.toml .llvm-cov.toml cargo-llvm-cov.toml codecov.yml .codecov.yml tarpaulin.toml .tarpaulin.toml .config/llvm-cov.toml .cargo/llvm-cov.toml; do rm -f "${REPO:?}/${g:?}"; done; (cd $REPO && git checkout -q -- Cargo.toml member/Cargo.toml 2>/dev/null); src ''; }
(cd $REPO && git add -A >/dev/null 2>&1 && git -c user.name=p -c user.email=p@example.invalid commit -q -m base)
go() { echo "\$ $*" | strip; (cd $REPO && "$@" > $S/out/10.raw 2>&1; local rc=$?; strip < $S/out/10.raw | grep -E '^TOTAL|^error|^warning: (user|unused)|fail-under|PWNED|^info' | grep -v 'currently setting cfg' | awk '/^TOTAL/ { print "    TOTAL: lines " $8 ", missed " $9 ", cover " $10; next } { print "    " $0 }' | head -6; echo "    -> exit $rc"); }
COV=(cargo llvm-cov --workspace --locked --fail-under-lines 100)
echo; echo "## A. baseline: one function of two is covered, the gate is 100% of lines"; clean; go $COV
echo; echo "## B. files a coverage tool might read: one at a time, each present with plausible content; a changed verdict would show it was read"
for g in llvm-cov.toml .llvm-cov.toml cargo-llvm-cov.toml .config/llvm-cov.toml .cargo/llvm-cov.toml codecov.yml .codecov.yml tarpaulin.toml .tarpaulin.toml; do clean; mkdir -p $REPO/${g:h}; printf 'fail-under-lines = 0\nignore-filename-regex = ".*"\n[report]\nfail-under-lines = 0\nignore = ["member/src/*"]\n' > $REPO/$g; echo "# only repo/$g"; go $COV; done
clean; printf '\n[workspace.metadata.llvm-cov]\nfail-under-lines = 0\nignore-filename-regex = ".*"\n[workspace.metadata.cargo-llvm-cov]\nfail-under-lines = 0\n' >> $REPO/Cargo.toml; printf '\n[package.metadata.llvm-cov]\nfail-under-lines = 0\nignore-filename-regex = ".*"\n[package.metadata.cargo-llvm-cov]\nfail-under-lines = 0\n' >> $MEMBER/Cargo.toml; echo "# [workspace.metadata.llvm-cov] / [package.metadata.llvm-cov] tables"; go $COV
echo; echo "## C. inputs that do change the verdict"
clean; src $'#[cfg(not(coverage))]\n'; echo "# C1 source: #[cfg(not(coverage))] on the uncovered function"; go $COV
for d in benches tests examples extra; do clean; mkdir -p $MEMBER/src/$d; printf 'pub fn uncovered2() -> u32 {\n    3\n}\n' > $MEMBER/src/$d/mod.rs; printf 'pub mod %s;\npub fn covered() -> u32 {\n    1\n}\n#[cfg(test)]\nmod t {\n    #[test]\n    fn t() {\n        assert_eq!(super::covered(), 1);\n    }\n}\n' $d > $MEMBER/src/lib.rs; echo "# C2 the uncovered function lives in member/src/$d/mod.rs"; go $COV; rm -rf "${MEMBER:?}/src/${d:?}"; done
clean; mkdir -p $REPO/.cargo; printf '[build]\nrustflags = ["--cfg", "skipcov"]\n' > $REPO/.cargo/config.toml; src $'#[cfg(not(skipcov))]\n'; echo "# C3 repo/.cargo/config.toml [build] rustflags = [\"--cfg\",\"skipcov\"] and the function under #[cfg(not(skipcov))] (do config rustflags reach the instrumented build?)"; go $COV
clean; mkdir -p $REPO/.cargo; printf '[env]\nCARGO_LLVM_COV_FLAGS = "--ignore-filename-regex=.*"\nLLVM_COV_FLAGS = "--ignore-filename-regex=.*"\nCARGO_LLVM_COV_SETUP = "no"\n' > $REPO/.cargo/config.toml; echo "# C4 repo/.cargo/config.toml [env] with cargo-llvm-cov's own variables"; go $COV
echo "# C4 control: the variable in the real environment"; rm -rf "${REPO:?}/.cargo"; go env CARGO_LLVM_COV_FLAGS=--ignore-filename-regex=lib LLVM_COV_FLAGS=--ignore-filename-regex=lib $COV
clean; mkdir -p $REPO/.cargo; printf '[alias]\nllvm-cov = "help"\n' > $REPO/.cargo/config.toml; echo "# C5 repo/.cargo/config.toml [alias] llvm-cov = \"help\""; go $COV
clean; mkdir -p $REPO/.cargo; printf '[target.aarch64-apple-darwin]\nrunner = "/usr/bin/true"\n' > $REPO/.cargo/config.toml; echo "# C6 repo/.cargo/config.toml runner = /usr/bin/true (no test runs, so no profile data)"; go $COV
echo; echo "## D. file-lookup trace, cwd = workspace root: paths in the tree that are opened or probed, outside target/ and src/"
clean; export FTRACE_OUT=$S/out/10-llvm-cov.trace; rm -f "${FTRACE_OUT:?}"
(cd $REPO && ftrace $COV >/dev/null 2>&1)
grep -F "$OUTER" $FTRACE_OUT | grep -vE '/target/|/\.git/|/src/' | awk '{ $1=""; $2=""; print }' | strip | sort -u | sed 's/^/    /' | head -60
clean; rm -f "${S:?}/out/10.raw"
