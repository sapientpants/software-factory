#!/bin/bash
# Probe P-40, part 2: what a diff of two trees shows for a file that moved,
# with rename detection as each command has it by default, switched on and
# switched off. Run as: renames.sh <scratch dir>.
set -u
S=$(cd "$1" && pwd -P)
W=$S/renames
GITBIN=$(dirname "$(command -v git)")
rm -rf "$W" && mkdir -p "$W/home"
g() {
  env -i PATH="$GITBIN:/usr/bin:/bin" HOME="$W/home" GIT_CONFIG_NOSYSTEM=1 \
    GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0 \
    GIT_AUTHOR_NAME=p GIT_AUTHOR_EMAIL=p@example.invalid GIT_AUTHOR_DATE='2026-11-26T00:00:00Z' \
    GIT_COMMITTER_NAME=p GIT_COMMITTER_EMAIL=p@example.invalid GIT_COMMITTER_DATE='2026-11-26T00:00:00Z' git "$@"
}
g --version
g init -q -b main "$W/r"
cd "$W/r" || exit 1
mkdir -p .github/workflows tools src
for i in $(seq 1 40); do echo "lint-$i = \"deny\""; done >clippy.toml
for i in $(seq 1 30); do echo "  step-$i: run"; done >.github/workflows/ci.yml
for i in $(seq 1 30); do echo "fn gen_$i() {}"; done >build.rs
for i in $(seq 1 30); do echo "dep-$i = \"1\""; done >Cargo.toml
echo cfg >tools/config.toml
ln -s tools cfg
echo keep >src/lib.rs
g add -A && g commit -q -m base
BASE=$(g rev-parse HEAD)

# The candidate: four moves and a copy.
mkdir -p notes docs vendor
g mv clippy.toml notes/lints.txt                 # moved, bytes unchanged
g mv .github/workflows/ci.yml docs/ci-example.yml # moved out of the workflow directory
g mv build.rs src/gen.rs && echo "fn extra() {}" >>src/gen.rs  # moved and edited
cp Cargo.toml vendor/Cargo.toml                  # copied, the original stays
g mv cfg cfg2                                    # a symbolic link under another name
g add -A && g commit -q -m candidate
CAND=$(g rev-parse HEAD)

H="--git-dir=$W/r/.git"
show() {
  echo "\$ git $*" | sed "s|$H|--git-dir=<repo>|; s|$BASE|<base>|; s|$CAND|<candidate>|"
  g "$@" | sed 's/^/  /'
  echo
}
cd "$W" || exit 1
echo
echo "## A. diff-tree, the plumbing command"
show $H diff-tree -r --raw --abbrev=7 $BASE $CAND
show $H diff-tree -r --raw --abbrev=7 -M $BASE $CAND
show $H diff-tree -r --raw --abbrev=7 -C --find-copies-harder $BASE $CAND
show $H diff-tree -r --raw --abbrev=7 --no-renames $BASE $CAND
echo "## B. diff, the porcelain command, on the same two trees"
show $H diff --raw --abbrev=7 $BASE $CAND
show $H diff --raw --abbrev=7 --no-renames $BASE $CAND
echo "## C. With diff.renames = copies in the configuration of the repository"
g $H config diff.renames copies
show $H diff-tree -r --raw --abbrev=7 $BASE $CAND
show $H diff --raw --abbrev=7 $BASE $CAND
show $H diff --raw --abbrev=7 --no-renames $BASE $CAND
g $H config --unset diff.renames
echo "## D. The lines that each form counts (added, deleted, path)"
show $H diff-tree -r --numstat --no-renames $BASE $CAND
show $H diff-tree -r --numstat -M $BASE $CAND
echo "## E. The names alone, as a rule that reads paths would take them"
show $H diff-tree -r --name-status --no-renames $BASE $CAND
show $H diff-tree -r --name-status -M $BASE $CAND
