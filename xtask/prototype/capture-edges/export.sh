#!/bin/bash
# Probe P-40, part 3: what the export commands of SPEC 9.4 leave when they
# are run a second time on a directory that a verify has written to.
# Run as: export.sh <scratch dir>.
set -u
S=$(cd "$1" && pwd -P)
W=$S/export
GITBIN=$(dirname "$(command -v git)")
EMPTY=4b825dc642cb6eb9a060e54bf8d69288fbee4904
umask 022
rm -rf "$W" && mkdir -p "$W/home"
g() {
  env -i PATH="$GITBIN:/usr/bin:/bin" HOME="$W/home" GIT_CONFIG_NOSYSTEM=1 \
    GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0 ${IDX:+GIT_INDEX_FILE=$IDX} \
    GIT_AUTHOR_NAME=p GIT_AUTHOR_EMAIL=p@example.invalid \
    GIT_COMMITTER_NAME=p GIT_COMMITTER_EMAIL=p@example.invalid git "$@"
}
IDX=
g --version
g init -q -b main "$W/src"
cd "$W/src" || exit 1
mkdir -p src tests
echo 'pub fn answer() -> u32 { 41 }' >src/lib.rs
echo 'assert_eq!(answer(), 42);' >tests/t.rs
echo '#!/bin/sh' >run.sh && chmod 755 run.sh
g add -A && g commit -q -m candidate
CAND=$(g rev-parse HEAD)
g clone -q --bare "$W/src" "$W/host.git"
cd "$W" || exit 1
H="--git-dir=$W/host.git --attr-source=$EMPTY"

# export_into <dir> [-f]: the two commands of 9.4.
export_into() {
  local dir=$1 force=${2:-}
  IDX=$W/index.$RANDOM
  g $H read-tree "$CAND"
  g $H --work-tree="$dir" checkout-index -a $force 2>&1 | sed "s|$W|<w>|g; s/^/    /"
  echo "    checkout-index -a $force: exit=${PIPESTATUS[0]}"
  rm -f "$IDX"; IDX=
}
mode() { if [ "$(uname)" = Darwin ]; then stat -f %Sp "$1"; else stat -c %A "$1"; fi; }
state() {
  (cd "$1" && for f in src/lib.rs tests/t.rs run.sh planted.rs; do
    if [ -e "$f" ]; then printf '    %-12s %s  %s\n' "$f" "$(mode "$f")" "$(head -1 "$f")"; else printf '    %-12s absent\n' "$f"; fi
  done)
}
# verify_writes <dir>: what a verify command of a subverted candidate may do.
verify_writes() {
  echo 'assert_eq!(answer(), 41);' >"$1/tests/t.rs"   # the failing test, made to pass
  rm "$1/src/lib.rs" && echo 'pub fn answer() -> u32 { 41 } // rewritten' >"$1/src/lib.rs"
  chmod 644 "$1/run.sh"
  echo 'mod planted;' >"$1/planted.rs"                 # a file the candidate does not hold
}

echo
echo "## A. A first export, into a new and empty directory"
mkdir -p "$W/e1"; export_into "$W/e1"; state "$W/e1"
echo
echo "## B. A verify writes to it; then the same two commands on the same directory"
verify_writes "$W/e1"; echo "  after the verify:"; state "$W/e1"
echo "  the second export:"; export_into "$W/e1"; state "$W/e1"
echo
echo "## C. The same with checkout-index -f"
mkdir -p "$W/e2"; export_into "$W/e2" >/dev/null; verify_writes "$W/e2"
echo "  the second export:"; export_into "$W/e2" -f; state "$W/e2"
echo
echo "## D. A directory of its own for the second use"
mkdir -p "$W/e3"; export_into "$W/e3"; state "$W/e3"
