#!/bin/bash
# Probe P-40, part 4: what git makes of a tree in which two names of one
# directory differ only in a spelling that SPEC VAL-5d folds, and what the
# export commands of SPEC 9.4 leave of it on the file system they run on.
# The trees are built from blobs with `git mktree`, so no file system has to
# hold the two names. Run as: case.sh <scratch dir>.
set -u
S=$(cd "$1" && pwd -P)
W=$S/case
GITBIN=$(dirname "$(command -v git)")
EMPTY=4b825dc642cb6eb9a060e54bf8d69288fbee4904
rm -rf "$W" && mkdir -p "$W/home"
g() {
  env -i PATH="$GITBIN:/usr/bin:/bin" HOME="$W/home" GIT_CONFIG_NOSYSTEM=1 LC_ALL=C \
    GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0 ${IDX:+GIT_INDEX_FILE=$IDX} \
    GIT_AUTHOR_NAME=p GIT_AUTHOR_EMAIL=p@example.invalid GIT_AUTHOR_DATE='2026-11-26T00:00:00Z' \
    GIT_COMMITTER_NAME=p GIT_COMMITTER_EMAIL=p@example.invalid GIT_COMMITTER_DATE='2026-11-26T00:00:00Z' git "$@"
}
IDX=
g --version
: >"$S/CaseProbe"
if [ -e "$S/caseprobe" ]; then echo "the scratch directory takes caseprobe for CaseProbe"; else echo "the scratch directory does not take caseprobe for CaseProbe"; fi
g init -q --bare -b main "$W/host.git"
H="--git-dir=$W/host.git"
blob() { printf '%s\n' "$1" | g $H hash-object -w --stdin; }
# tree <mode> <type> <name> <object> ...: one tree from its entries, in the order git wants them.
tree() {
  while [ $# -gt 0 ]; do printf '%s %s %s\t%s\0' "$1" "$2" "$4" "$3"; shift 4; done | LC_ALL=C sort -z -t "$(printf '\t')" -k2 | g $H mktree -z
}
commit() { g $H commit-tree -m "$2" "$1"; }

UPPER=$(blob 'the file named in upper case')
LOWER=$(blob 'the file named in lower case')
SUB_A=$(tree 100644 blob a.rs "$LOWER")
SUB_B=$(tree 100644 blob b.rs "$UPPER")
LINK=$(printf 'elsewhere' | g $H hash-object -w --stdin)
ESZETT=$(printf 'stra\xc3\x9fe.txt')
NFC=$(printf '\xc3\xa9.md')       # U+00E9
NFD=$(printf 'e\xcc\x81.md')      # e, U+0301
LONG_S=$(printf 'AGENT\xc5\xbf.md') # U+017F

# check <label> <tree>: fsck, index-pack --strict on a pack of it, and the export of 9.4.
# The notice that fsck prints about a repository without a branch is left out.
check() {
  local label=$1 t=$2 c
  echo
  echo "### $label"
  g $H ls-tree -r -t "$t" | sed 's/^/  /'
  c=$(commit "$t" "$label")
  g $H fsck --strict --no-dangling 2>&1 | grep -v '^notice: ' | sed 's/^/  fsck: /'; echo "  fsck --strict: exit=${PIPESTATUS[0]}"
  rm -rf "$W/ingest.git" && g init -q --bare "$W/ingest.git"
  echo "$c" | g $H pack-objects --revs --stdout -q | g --git-dir="$W/ingest.git" index-pack --stdin --strict 2>&1 | sed 's/^/  index-pack: /'
  echo "  index-pack --strict: exit=${PIPESTATUS[2]}"
  rm -rf "$W/export" && mkdir "$W/export"
  IDX=$W/index
  rm -f "$IDX"
  g $H --attr-source=$EMPTY read-tree "$c"
  g $H --attr-source=$EMPTY --work-tree="$W/export" checkout-index -a 2>&1 | sed "s|$W|<w>|g; s/^/  checkout-index: /"
  echo "  checkout-index -a: exit=${PIPESTATUS[0]}"
  IDX=
  echo "  the export holds:"
  (cd "$W/export" && find . -mindepth 1 \( -type f -o -type l \) | LC_ALL=C sort | while IFS= read -r f; do
    if [ -L "$f" ]; then printf '    %s -> %s\n' "$f" "$(readlink "$f")"; else printf '    %s: %s\n' "$f" "$(head -1 "$f")"; fi
  done)
}

echo
echo "## A. Trees with two names that have one fold"
check "A1 README.md and readme.md" "$(tree 100644 blob README.md "$UPPER" 100644 blob readme.md "$LOWER")"
check "A2 directories src and SRC" "$(tree 040000 tree SRC "$SUB_B" 040000 tree src "$SUB_A")"
check "A3 a symbolic link Docs and a directory docs" "$(tree 120000 blob Docs "$LINK" 040000 tree docs "$SUB_A")"
check "A4 strasse.txt in two spellings, full case folding" "$(tree 100644 blob STRASSE.txt "$UPPER" 100644 blob "$ESZETT" "$LOWER")"
check "A5 e-acute as one code point and as two" "$(tree 100644 blob "$NFC" "$UPPER" 100644 blob "$NFD" "$LOWER")"
check "A6 AGENTS.md and the same with a long s" "$(tree 100644 blob AGENTS.md "$UPPER" 100644 blob "$LONG_S" "$LOWER")"

echo
echo "## B. A clone of a branch that holds A1, and its status"
T=$(tree 100644 blob README.md "$UPPER" 100644 blob readme.md "$LOWER")
g $H update-ref refs/heads/main "$(commit "$T" collide)"
g clone -q "$W/host.git" "$W/clone" 2>&1 | sed "s|$W|<w>|g; s/^/  /"
echo "  clone: exit=${PIPESTATUS[0]}"
(cd "$W/clone" && g status --porcelain | sed 's/^/  status: /'; ls | sed 's/^/  holds: /')
