#!/bin/sh
# Probe P-27 (SPEC 23.2), the git half: which command lines make git skip its
# hooks, and what git hands a hook. Throwaway code. Run it with no argument;
# it works in a fresh temporary directory and prints one line per command.
set -u

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
git --version

# --- Part 1: a repository whose every hook prints a line and fails. ---------
repo=$work/failing
git init -q -b main "$repo"
cd "$repo" || exit 1
git config user.name probe
git config user.email probe@example.invalid
mkdir hooks
for hook in pre-commit commit-msg pre-push; do
    printf '#!/bin/sh\necho HOOK-RAN >&2\nexit 1\n' > "hooks/$hook"
    chmod +x "hooks/$hook"
done
git config core.hooksPath hooks
echo content > file
echo message > "$work/message"

# Runs "$@" with one file staged and nothing committed, and says what the
# hooks did.
commit() {
    git update-ref -d HEAD 2>/dev/null
    git add file
    out=$("$@" 2>&1)
    code=$?
    if printf '%s' "$out" | grep -q HOOK-RAN; then
        result="hook ran"
    elif [ "$code" -eq 0 ]; then
        result="hook skipped"
    else
        result="exit code $code: $(printf '%s' "$out" | head -1)"
    fi
    printf '%-66s %s\n' "$*" "$result"
}

echo "== git commit"
commit git commit -m x
commit git commit --no-verify -m x
commit git commit -n -m x
commit git commit -nm x
commit git commit -anm x
commit git commit -qn -m x
commit git commit -m x -n
commit git commit -F ../message -n
commit git commit -nF ../message
commit git commit --no-verif -m x
commit git commit --no-veri -m x
commit git commit --no-ver -m x
commit git commit --no-ve -m x
commit git commit --no-v -m x
commit git commit --no-verify --verify -m x
commit git commit -m x --verify --no-verify
commit git commit -m x --veri
commit git commit -m -n
commit git commit -mnaming
commit git commit -F../message
commit git commit --message -n
commit git commit --message=-n
commit git commit --mess -n
commit git commit -m x -- -n
commit git -C . commit -n -m x
commit "$(command -v git)" commit -n -m x
commit git --no-pager -c user.name=n commit -m x

commit git commit --mess -- -n
commit git commit --mes x -n

echo "== git commit, written for the shell"
commit sh -c 'git 2>/dev/null commit -n -m x'
commit bash -c 'git commit -F <(echo message) -n'
commit bash -c "git commit -m x \$'-n'"
commit bash -c 'echo $((1<<2)) >/dev/null
git commit -n -m x'
commit bash -c 'N=-n; git commit $N -m x'

echo "== core.hooksPath given for one command"
commit git -c core.hooksPath=/dev/null commit -m x
commit git -c CORE.HooksPath=/dev/null commit -m x
commit git --config-env=core.hooksPath=HOME commit -m x
commit git --config-env core.hooksPath=HOME commit -m x
commit env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit -m x
commit env "GIT_CONFIG_PARAMETERS='core.hooksPath'='/dev/null'" git commit -m x

echo "== git push"
git init -q --bare "$work/remote.git"
git remote add o "$work/remote.git"
git update-ref -d HEAD 2>/dev/null
git add file
git -c core.hooksPath=/dev/null commit -q -m base
push() {
    out=$("$@" 2>&1)
    code=$?
    if printf '%s' "$out" | grep -q HOOK-RAN; then
        result="hook ran"
    elif [ "$code" -eq 0 ]; then
        result="hook skipped"
    else
        result="exit code $code: $(printf '%s' "$out" | head -1)"
    fi
    printf '%-66s %s\n' "$*" "$result"
}
push git push o main
push git push -n o main
push git push --no-verify -n o main
push git push --no-veri -n o main
push git push --no-ver -n o main
push git push -n o main --no-verify
push git push --no-verify --verify -n o main
git -C "$work/remote.git" config receive.advertisePushOptions true
push git push -o --no-verify -n o main
push git push -o -- --no-verify -n o main
push git push --push-opt -- --no-verify -n o main

# --- Part 2: a repository whose hook reports what it was handed. -------------
echo "== what a hook is handed"
repo=$work/reporting
git init -q -b main "$repo"
cd "$repo" || exit 1
git config user.name probe
git config user.email probe@example.invalid
mkdir .githooks
cat > .githooks/pre-commit <<'HOOK'
#!/bin/sh
echo "  directory: $(pwd | sed "s|.*/tmp\.[^/]*/|<work>/|")"
env | grep '^GIT_' | grep -v '^GIT_CONFIG_GLOBAL\|^GIT_CONFIG_NOSYSTEM\|^GIT_AUTHOR' | sort | sed -e 's|=.*/tmp\.[^/]*/|=<work>/|' -e 's/^/  /'
HOOK
chmod +x .githooks/pre-commit
git config core.hooksPath .githooks
echo a > a
git add .
echo "-- from the top of the work tree"
git commit -q -m one
mkdir sub
echo b > sub/b
git add .
echo "-- from a subdirectory"
(cd sub && git commit -q -m two)
echo "-- from a linked worktree, with the relative core.hooksPath"
git worktree add -q "$work/linked" -b side
(cd "$work/linked" && echo c > c && git add c && git commit -q -m three)
echo "-- from the top, with a path argument"
echo more >> a
git commit -q -m four a
echo "-- with the hook's executable bit removed"
chmod -x .githooks/pre-commit
echo d > d
git add d
git commit -q -m five
echo "  exit code $?"
echo "-- git rev-parse --local-env-vars"
git rev-parse --local-env-vars | tr '\n' ' '
echo
