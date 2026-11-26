#!/bin/bash
# Probe P-40, part 1: which directories of a checkout git takes for a bare
# repository when it is started inside them, what it then runs from their
# `config`, and what `safe.bareRepository = explicit` changes.
# Run as: bare.sh <scratch dir>. Every git process gets a cleared environment,
# a home of its own, no system configuration and the file <scratch>/bare/global
# as its global configuration.
set -u
S=$(cd "$1" && pwd -P)
W=$S/bare
GITBIN=$(dirname "$(command -v git)")
rm -rf "$W" && mkdir -p "$W/home"
: >"$W/global"
: >"$W/canary.log"
cat >"$W/canary.sh" <<CANARY
#!/bin/sh
echo "\$1" >>"$W/canary.log"
CANARY
chmod +x "$W/canary.sh"

# SYS names a system configuration for the cases that have one, and GD a
# value for GIT_DIR.
SYS=
GD=
g() {
  local sys=GIT_CONFIG_NOSYSTEM=1
  [ -n "$SYS" ] && sys=GIT_CONFIG_SYSTEM=$SYS
  env -i PATH="$GITBIN:/usr/bin:/bin" HOME="$W/home" "$sys" ${GD:+GIT_DIR=$GD} \
    GIT_CONFIG_GLOBAL="$W/global" GIT_TERMINAL_PROMPT=0 \
    GIT_AUTHOR_NAME=p GIT_AUTHOR_EMAIL=p@example.invalid \
    GIT_COMMITTER_NAME=p GIT_COMMITTER_EMAIL=p@example.invalid git "$@"
}

# hostile_config <file>: a config that names the canary in an alias and as the
# fsmonitor, and gives the repository a work tree, so that `git status` runs.
hostile_config() {
  cat >"$1" <<CONFIG
[core]
	repositoryformatversion = 0
	bare = false
	worktree = ..
	fsmonitor = $W/canary.sh fsmonitor
[alias]
	st = !$W/canary.sh alias
CONFIG
}

# run <dir> <git arguments...>: one git command started in <dir>. Prints the
# exit status, the first line of what it printed, and what the canary logged.
run() {
  local dir=$1
  shift
  : >"$W/canary.log"
  local out
  out=$(cd "$dir" && g "$@" >"$W/out.txt" 2>&1; echo "exit=$?")
  local first
  first=$(head -1 "$W/out.txt" | sed "s|$W|<w>|g")
  local ran
  ran=$(sort -u "$W/canary.log" | tr '\n' ' ')
  printf '  %-28s %-8s canary: %-16s %s\n' "git $*" "$out" "${ran:-none}" "$first"
}

# three <dir>: the three commands of every case.
three() {
  run "$1" rev-parse --git-dir
  run "$1" st
  run "$1" status --porcelain
}

echo "## git"
g --version
if [ "$(uname)" = Darwin ]; then
  echo "file system of the scratch directory: $(diskutil info "$(df "$S" | awk 'NR==2 {print $1}')" | sed -n 's/ *File System Personality: *//p')"
else
  echo "file system of the scratch directory: $(stat -f -c %T "$S")"
fi
: >"$S/CaseProbe"
if [ -e "$S/caseprobe" ]; then echo "it takes caseprobe for CaseProbe"; else echo "it does not take caseprobe for CaseProbe"; fi

g init -q -b main "$W/outer"
O=$W/outer
echo base >"$O/README.md"

echo
echo "## A. Layouts, each a directory of the work tree of outer/. '.' or a path under the directory means that git took it for a repository; <w>/outer/.git means that it did not."

mk() { mkdir -p "$O/$1"; }

echo "A1 HEAD (ref: refs/heads/main), config, objects/keep, refs/keep"
mk a1/objects; mk a1/refs; echo 'ref: refs/heads/main' >"$O/a1/HEAD"; : >"$O/a1/objects/keep"; : >"$O/a1/refs/keep"; hostile_config "$O/a1/config"
three "$O/a1"

echo "A2 the same without config"
mk a2/objects; mk a2/refs; echo 'ref: refs/heads/main' >"$O/a2/HEAD"; : >"$O/a2/objects/keep"; : >"$O/a2/refs/keep"
run "$O/a2" rev-parse --git-dir

echo "A3 objects and refs are symbolic links to '.'"
mk a3; echo 'ref: refs/heads/main' >"$O/a3/HEAD"; ln -s . "$O/a3/objects"; ln -s . "$O/a3/refs"; hostile_config "$O/a3/config"
three "$O/a3"

echo "A4 objects and refs are regular files, mode 0755"
mk a4; echo 'ref: refs/heads/main' >"$O/a4/HEAD"; : >"$O/a4/objects"; : >"$O/a4/refs"; chmod 755 "$O/a4/objects" "$O/a4/refs"; hostile_config "$O/a4/config"
three "$O/a4"

echo "A5 objects and refs are regular files, mode 0644"
mk a5; echo 'ref: refs/heads/main' >"$O/a5/HEAD"; : >"$O/a5/objects"; : >"$O/a5/refs"; chmod 644 "$O/a5/objects" "$O/a5/refs"; hostile_config "$O/a5/config"
three "$O/a5"

echo "A6 HEAD and commondir (../a6common), which holds config, objects/keep and refs/keep and no HEAD"
mk a6; mk a6common/objects; mk a6common/refs; echo 'ref: refs/heads/main' >"$O/a6/HEAD"; echo '../a6common' >"$O/a6/commondir"
: >"$O/a6common/objects/keep"; : >"$O/a6common/refs/keep"; hostile_config "$O/a6common/config"
three "$O/a6"
echo "   started in a6common itself:"
run "$O/a6common" rev-parse --git-dir

echo "A7 the names in another letter case: head, Objects/keep, REFS/keep, Config"
mk a7/Objects; mk a7/REFS; echo 'ref: refs/heads/main' >"$O/a7/head"; : >"$O/a7/Objects/keep"; : >"$O/a7/REFS/keep"; hostile_config "$O/a7/Config"
three "$O/a7"

echo "A8 HEAD holds 40 hex digits"
mk a8/objects; mk a8/refs; echo '1111111111111111111111111111111111111111' >"$O/a8/HEAD"; : >"$O/a8/objects/keep"; : >"$O/a8/refs/keep"; hostile_config "$O/a8/config"
three "$O/a8"

echo "A9 HEAD is a symbolic link to refs/heads/main"
mk a9/objects; mk a9/refs; ln -s refs/heads/main "$O/a9/HEAD"; : >"$O/a9/objects/keep"; : >"$O/a9/refs/keep"; hostile_config "$O/a9/config"
three "$O/a9"

echo "A10 HEAD holds the word hello"
mk a10/objects; mk a10/refs; echo hello >"$O/a10/HEAD"; : >"$O/a10/objects/keep"; : >"$O/a10/refs/keep"; hostile_config "$O/a10/config"
three "$O/a10"

echo "A11 HEAD, config and objects/keep, no refs"
mk a11/objects; echo 'ref: refs/heads/main' >"$O/a11/HEAD"; : >"$O/a11/objects/keep"; hostile_config "$O/a11/config"
three "$O/a11"

echo "A12 the layout of A1, git started two directories below it"
mk a12/objects; mk a12/refs; mk a12/sub/deep; echo 'ref: refs/heads/main' >"$O/a12/HEAD"; : >"$O/a12/objects/keep"; : >"$O/a12/refs/keep"; : >"$O/a12/sub/deep/x"; hostile_config "$O/a12/config"
three "$O/a12/sub/deep"

echo "A13 the layout at the root of a checkout, beside its .git: a second repository, root/, with HEAD, config, objects/keep and refs/keep"
g init -q -b main "$W/root"
mkdir -p "$W/root/objects" "$W/root/refs" "$W/root/sub"; echo 'ref: refs/heads/main' >"$W/root/HEAD"; : >"$W/root/objects/keep"; : >"$W/root/refs/keep"; : >"$W/root/sub/x"; hostile_config "$W/root/config"
three "$W/root"
echo "   started in root/sub:"
run "$W/root/sub" rev-parse --git-dir
echo "   the same tree without its .git, as an export holds it:"
cp -R "$W/root" "$W/root-export" && rm -rf "$W/root-export/.git"
run "$W/root-export" rev-parse --git-dir

echo
echo "## B. The layouts committed and cloned"
(cd "$O" && g add -A 2>&1 | sed "s|$W|<w>|g"; echo "git add -A: exit=$?")
(cd "$O" && g commit -q -m base; echo "git commit: exit=$?")
(cd "$O" && g ls-tree -r --name-only HEAD | grep -E '^a(1|3|4|6|7)(common)?/' | tr '\n' ' '; echo)
(cd "$O" && g ls-tree -r HEAD a3 a4 | sed 's/^/  /')
g clone -q "file://$O" "$W/clone" 2>&1 | sed "s|$W|<w>|g"
echo "in the clone:"
for v in a1 a3 a4 a6 a7; do echo "$v"; three "$W/clone/$v"; done

echo
echo "## C. safe.bareRepository = explicit in the global configuration"
printf '[safe]\n\tbareRepository = explicit\n' >"$W/global"
for v in a1 a3 a4 a6 a7 a9 a12/sub/deep; do echo "$v"; three "$O/$v"; done
echo "a1, whose own config also says [safe] bareRepository = all"
printf '[safe]\n\tbareRepository = all\n' >>"$O/a1/config"
three "$O/a1"

echo
echo "## D. The setting in other places"
: >"$W/global"
echo "D1 on the command line, -c safe.bareRepository=explicit"
run "$O/a1" -c safe.bareRepository=explicit rev-parse --git-dir
echo "D2 in the configuration of outer/ alone (outer/.git/config)"
(cd "$O" && g config safe.bareRepository explicit)
run "$O/a1" st
run "$O/a3" st
(cd "$O" && g config --unset safe.bareRepository)
echo "D3 in the system configuration, the global one empty"
printf '[safe]\n\tbareRepository = explicit\n' >"$W/system"
SYS=$W/system
run "$O/a3" rev-parse --git-dir
run "$O/a3" st
SYS=

echo
echo "## E. What else changes under explicit (global)"
printf '[safe]\n\tbareRepository = explicit\n' >"$W/global"
echo "E1 an ordinary repository, at its root and in a subdirectory"
run "$O" status --porcelain
mkdir -p "$O/plain/dir" && : >"$O/plain/dir/f"
run "$O/plain/dir" rev-parse --show-toplevel
echo "E2 inside the .git directory of an ordinary repository"
run "$O/.git" rev-parse --git-dir
run "$O/.git/refs" rev-parse --git-dir
echo "E3 a linked worktree"
(cd "$O" && g worktree add -q "$W/wt" -b wt 2>&1 | head -1)
run "$W/wt" status --porcelain
echo "E4 a bare repository of the operator's own, started inside it"
g init -q --bare -b main "$W/real.git"
run "$W/real.git" rev-parse --git-dir
run "$W/real.git" --git-dir=. rev-parse --git-dir
echo "   with GIT_DIR=. in the environment:"
GD=.
run "$W/real.git" rev-parse --git-dir
GD=
echo "E5 a bare repository as a remote, by path"
(cd "$O" && g push -q "$W/real.git" main 2>&1 | head -2 | sed "s|$W|<w>|g"; echo "  push to a path: exit=${PIPESTATUS[0]}")
(g clone -q "$W/real.git" "$W/c2" 2>&1 | head -2 | sed "s|$W|<w>|g"; echo "  clone from a path: exit=${PIPESTATUS[0]}")
(cd "$W/c2" && g fetch -q origin 2>&1 | head -2 | sed "s|$W|<w>|g"; echo "  fetch from a path: exit=${PIPESTATUS[0]}")
(g ls-remote "$W/real.git" 2>&1 | head -1 | sed "s|$W|<w>|g"; echo "  ls-remote of a path: exit=${PIPESTATUS[0]}")
