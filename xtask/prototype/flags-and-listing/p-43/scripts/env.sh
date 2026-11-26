# Common environment for every probe script. Sourced, never executed.
export S=<scratch>/p43-tools
export REAL_HOME=<home>
export HOME=$S/home                      # fake home: nothing under the real $HOME is read or written
export CARGO_HOME=$S/home/.cargo
export RUSTUP_HOME=<home>/.rustup    # real toolchains, read only
export RUSTUP_TOOLCHAIN=1.100.0           # pinned unless a rustup test unsets it
export CARGO_TARGET_DIR=$S/target/shared
export PATH=$S/tools/bin:<home>/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin
export CARGO_TERM_COLOR=never NO_COLOR=1
unset MISE_SHELL __MISE_DIFF __MISE_SESSION __MISE_ORIG_PATH
# ftrace CMD...: run CMD with the file-lookup interposer; trace goes to $FTRACE_OUT
ftrace() { DYLD_INSERT_LIBRARIES=$S/trace/libftrace.dylib "$@"; }
# run LABEL CMD...: print the command, run it, print the exit code
run() { echo; echo "\$ $*"; "$@" 2>&1; echo "[exit $?]"; }
# mkfx NAME: a throwaway tree  $S/fx/NAME/outer/repo (git root + workspace root)/member (crate)
mkfx() {
  local d=$S/fx/$1; rm -rf "$d"; mkdir -p "$d/outer/repo/member/src"
  printf '[workspace]\nmembers = ["member"]\nresolver = "3"\n' > "$d/outer/repo/Cargo.toml"
  printf '[package]\nname = "member"\nversion = "0.1.0"\nedition = "2024"\nlicense = "MIT"\npublish = false\n' > "$d/outer/repo/member/Cargo.toml"
  printf 'pub fn f() -> u32 {\n    1\n}\n' > "$d/outer/repo/member/src/lib.rs"
  (cd "$d/outer/repo" && git init -q . && cargo generate-lockfile 2>/dev/null)
  FX=$d; OUTER=$d/outer; REPO=$d/outer/repo; MEMBER=$d/outer/repo/member
}
# strip ANSI colour escapes and shorten the fixture / home prefixes
strip() { sed $'s/\e\\[[0-9;]*m//g; s/\e(B//g' | sed "s|$FX/||g; s|$HOME|\$HOME|g; s|$S|\$S|g"; }
cd "$S"   # never run a tool from the directory the probe was started in
