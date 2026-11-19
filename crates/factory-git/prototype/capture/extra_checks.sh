#!/bin/sh
# PROTOTYPE. Four single observations the probe record cites that capture_proto.py does not print.
#   sh extra_checks.sh                                         on this machine
#   docker run --rm -v "$PWD":/p:ro sf-p7-proto sh /p/extra_checks.sh
#   docker run --rm -v "$PWD":/p:ro debian:bookworm-slim sh -c 'apt-get update -qq && apt-get install -y -qq git >/dev/null && sh /p/extra_checks.sh'
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null HOME=/nonexistent LC_ALL=C
export GIT_AUTHOR_NAME=p GIT_AUTHOR_EMAIL=p@example.invalid GIT_COMMITTER_NAME=p GIT_COMMITTER_EMAIL=p@example.invalid
EMPTY=4b825dc642cb6eb9a060e54bf8d69288fbee4904
T=$(mktemp -d) && cd "$T" || exit 1
echo "== $(git --version) on $(uname -s)"

echo "-- 1. GIT_ATTR_SOURCE against --attr-source, staging a CRLF file under eol=crlf"
git init -q a && cd a
printf 'a.txt text eol=crlf\n' > .gitattributes; printf 'one\r\n' > a.txt
bytes() { git cat-file blob :a.txt 2>/dev/null | od -An -c | tr -s ' ' | tr -d '\n'; }
GIT_ATTR_SOURCE=$EMPTY git add a.txt; printf 'variable: exit %s, blob bytes:%s\n' "$?" "$(bytes)"
git read-tree --empty
git --attr-source=$EMPTY add a.txt 2>&1 | head -1; printf 'option: blob bytes:%s\n' "$(bytes)"
cd ..

echo "-- 2. update-index --remove against --force-remove for a path now behind a symlink"
git init -q b && cd b
mkdir src other; echo x > src/lib.rs; echo y > other/lib.rs; git add src; git commit -q -m base
rm -r src; ln -s other src
printf 'src/lib.rs\0' | git update-index -z --remove --stdin; echo "--remove: exit $?"
printf 'src/lib.rs\0' | git update-index -z --force-remove --stdin; echo "--force-remove: exit $?, index now holds: [$(git ls-files)]"
cd ..

echo "-- 3. ls-tree -z, with --format and with -l, on a name that holds a tab"
git init -q c && cd c
printf x > "$(printf 'tab\tname')"; git add -A; git commit -q -m base
printf -- '--format: '; git ls-tree -r -z --format='%(objectmode)%x09%(path)' HEAD | od -An -c | tr -s ' ' | tr -d '\n'; echo
printf -- '-l:       '; git ls-tree -r -z -l HEAD | od -An -c | tr -s ' ' | tr -d '\n'; echo
cd ..

echo "-- 4. a gitlink in the index, a nested repository at its path whose only hostile setting is core.fsmonitor"
printf '#!/bin/sh\necho fired >> "%s/CANARY"\n' "$T" > canary.sh; chmod +x canary.sh
git init -q d && cd d
git init -q sub; (cd sub && echo i > i.txt && git add i.txt && git commit -q -m inner)
git update-index --add --cacheinfo "160000,$(git -C sub rev-parse HEAD),sub"
git -C sub config core.fsmonitor "$T/canary.sh"
git diff-files -z --raw --ignore-submodules=all >/dev/null 2>&1; echo "--ignore-submodules=all:  canary $([ -e "$T/CANARY" ] && echo FIRED || echo untouched)"
git diff-files -z --raw --ignore-submodules=none >/dev/null 2>&1; echo "--ignore-submodules=none: canary $([ -e "$T/CANARY" ] && echo FIRED || echo untouched)"
