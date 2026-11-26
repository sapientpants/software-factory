#!/bin/zsh
# typos: configuration files read unasked, layering, ignore files, what --config / --isolated do.
source ${0:A:h}/env.sh
mkfx typos
echo "## versions"; typos --version
# five misspellings, one per line, in three files. A config file that is read accepts exactly one of them, so the words still reported tell which files were read.
W=(teh recieve seperate occured definately)
body() { printf '// teh\n// recieve\n// seperate\n// occured\n// definately\npub fn f() -> u32 {\n    1\n}\n'; }
body > $MEMBER/src/lib.rs; printf '# notes\n\nteh recieve seperate occured definately\n' > $REPO/NOTES.md
ALL=( typos.toml _typos.toml .typos.toml .config/typos.toml .cargo/typos.toml typos.yaml .typos .typosrc pyproject.toml member/typos.toml member/_typos.toml member/.typos.toml member/src/typos.toml )
clean() { local g; for g in $ALL .ignore .gitignore .typosignore .rgignore .gitattributes member/.ignore member/.gitignore .git/info/exclude .hidden.md; do rm -f "${REPO:?}/${g:?}"; done; rm -rf "${REPO:?}/.config" "${REPO:?}/.cargo" "${REPO:?}/sub"; rm -f "${OUTER:?}/typos.toml" "${OUTER:?}/.typos.toml" "${OUTER:?}/.ignore" "${OUTER:?}/.gitignore" "${FX:?}/typos.toml" "${S:?}/home/typos.toml" "${S:?}/home/.typos.toml" "${S:?}/home/.config/typos.toml" "${S:?}/home/.config/typos/typos.toml" "${S:?}/home/.gitconfig" "${S:?}/home/.config/git/ignore" "${S:?}/home/.gitignore_global"
  (cd $REPO && git checkout -q -- Cargo.toml member/Cargo.toml 2>/dev/null); }
(cd $REPO && git add -A >/dev/null 2>&1 && git -c user.name=p -c user.email=p@example.invalid commit -q -m base)
# put FILE WORD: FILE accepts WORD
put() { mkdir -p "${1:h}"; case ${1:t} in pyproject.toml) printf '[tool.typos.default.extend-words]\n%s = "%s"\n' $2 $2 > "$1";; *) printf '[default.extend-words]\n%s = "%s"\n' $2 $2 > "$1";; esac; }
# go DIR ARGS...: run typos, print which of the five words are still reported per file, and the exit code
go() { local dir=$1; shift; echo "\$ (cd ${dir#$FX/} && typos $*)" | strip; (cd $dir && typos --format brief "$@" 2>&1 | strip | awk -F'[:`]' '/should be/ { f[$1]=f[$1]" "$6 } !/should be/ { print "    " $0 } END { for (k in f) print "    reported in " k ":" f[k] }' | sort; echo "    -> exit ${pipestatus[1]}"); }
echo; echo "## A. baseline (every word reported in every file)"; clean; go $REPO
echo; echo "## B. one candidate at a time, cwd = workspace root (the candidate accepts 'teh')"
for f in $ALL; do clean; put $REPO/$f teh; echo; echo "# only repo/$f"; go $REPO; done
for f in $OUTER/typos.toml $OUTER/.typos.toml $FX/typos.toml $HOME/typos.toml $HOME/.typos.toml $HOME/.config/typos.toml $HOME/.config/typos/typos.toml; do clean; put $f teh; echo; echo "# only $f" | strip; go $REPO; done
clean; printf '\n[workspace.metadata.typos.default.extend-words]\nteh = "teh"\n' >> $REPO/Cargo.toml; echo; echo "# only [workspace.metadata.typos] in repo/Cargo.toml"; go $REPO
clean; printf '\n[package.metadata.typos.default.extend-words]\nteh = "teh"\n' >> $MEMBER/Cargo.toml; echo; echo "# only [package.metadata.typos] in member/Cargo.toml"; go $REPO
echo; echo "## C. several at once: which is read, are they layered?"
clean; put $REPO/typos.toml teh; put $REPO/_typos.toml recieve; put $REPO/.typos.toml seperate; printf '\n[workspace.metadata.typos.default.extend-words]\noccured = "occured"\n' >> $REPO/Cargo.toml; put $REPO/pyproject.toml definately
echo "# repo: typos.toml=teh, _typos.toml=recieve, .typos.toml=seperate, Cargo.toml=occured, pyproject.toml=definately"; go $REPO
rm -f "${REPO:?}/typos.toml"; echo "# typos.toml removed"; go $REPO
rm -f "${REPO:?}/_typos.toml"; echo "# _typos.toml removed"; go $REPO
rm -f "${REPO:?}/.typos.toml"; echo "# .typos.toml removed"; go $REPO
clean; put $REPO/typos.toml teh; put $REPO/member/.typos.toml recieve; put $OUTER/.typos.toml seperate
echo "# repo/typos.toml=teh, member/.typos.toml=recieve, outer/.typos.toml=seperate; cwd = workspace root"; go $REPO
echo "# same, cwd = member"; go $MEMBER
echo "# same, cwd = outer, argument repo"; go $OUTER repo
clean; printf '\n[package.metadata.typos.default.extend-words]\nteh = "teh"\n' >> $MEMBER/Cargo.toml; echo "# only [package.metadata.typos] in member/Cargo.toml, cwd = member"; go $MEMBER
clean; put $REPO/typos.toml teh; echo "# only repo/typos.toml=teh, cwd = member (no config file in member but its Cargo.toml)"; go $MEMBER
clean; put $REPO/typos.toml teh; echo "# only repo/typos.toml=teh, cwd = member/src"; go $MEMBER/src
echo; echo "## D. explicit arguments (repo/typos.toml=teh is the protected file; siblings: repo/.typos.toml? no - member/.typos.toml=recieve, [package.metadata.typos]=seperate, outer/.typos.toml=occured)"
clean; put $REPO/typos.toml teh; put $REPO/member/.typos.toml recieve; printf '\n[package.metadata.typos.default.extend-words]\nseperate = "seperate"\n' >> $MEMBER/Cargo.toml; put $OUTER/.typos.toml occured; mkdir -p $FX/trusted; put $FX/trusted/typos.toml definately
echo "# D0 control"; go $REPO
echo "# D1 --config typos.toml"; go $REPO --config typos.toml
echo "# D2 --isolated"; go $REPO --isolated
echo "# D3 --isolated --config typos.toml"; go $REPO --isolated --config typos.toml
echo "# D4 --config <file outside the tree accepting 'definately'>"; go $REPO --config $FX/trusted/typos.toml
echo "# D5 --isolated --config <that file>"; go $REPO --isolated --config $FX/trusted/typos.toml
echo; echo "## E. files that take paths out of the check (no config; NOTES.md and member/src/lib.rs both hold all five words)"
ig() { clean; mkdir -p "${1:h}"; echo "$2" > "$1"; echo "# $1 contains '$2'" | strip; shift 2; go $REPO "$@"; }
ig $REPO/.ignore 'NOTES.md'
ig $REPO/.gitignore 'NOTES.md'
ig $REPO/.git/info/exclude 'NOTES.md'
ig $REPO/.typosignore 'NOTES.md'
ig $REPO/.rgignore 'NOTES.md'
ig $REPO/member/.ignore 'src/'
ig $OUTER/.ignore 'NOTES.md'
ig $OUTER/.gitignore 'NOTES.md'
ig $REPO/.gitattributes 'NOTES.md linguist-generated=true -diff binary'
clean; mkdir -p $HOME/.config/git; echo 'NOTES.md' > $HOME/.config/git/ignore; echo "# \$HOME/.config/git/ignore contains 'NOTES.md' (git's global excludes file)"; go $REPO; rm -f "${S:?}/home/.config/git/ignore"
clean; printf 'teh\n' > $REPO/.hidden.md; mkdir -p $REPO/sub; printf 'teh\n' > $REPO/sub/x.md; echo "# a hidden file .hidden.md and sub/x.md, each holding 'teh'"; go $REPO; echo "# same with --hidden"; go $REPO --hidden
echo "# E2 the ignore files against explicit arguments: repo/.ignore and repo/.gitignore both list NOTES.md"
clean; echo NOTES.md > $REPO/.ignore; echo 'member/' > $REPO/.gitignore
echo "# control"; go $REPO; echo "# --isolated"; go $REPO --isolated; echo "# --no-ignore"; go $REPO --no-ignore; echo "# --no-ignore-dot"; go $REPO --no-ignore-dot; echo "# --no-ignore-vcs"; go $REPO --no-ignore-vcs; echo "# --isolated --no-ignore --hidden"; go $REPO --isolated --no-ignore --hidden
echo "# E3 exclusion through a config key: repo/.typos.toml: [files] extend-exclude = [\"*\"]"; clean; printf '[files]\nextend-exclude = ["*"]\n' > $REPO/.typos.toml; go $REPO
echo "# E4 [default] extend-ignore-re = [\".*\"] in member/.typos.toml"; clean; printf '[default]\nextend-ignore-re = [".*"]\n' > $REPO/member/.typos.toml; go $REPO
echo; echo "## F. file-lookup trace, no config, cwd = workspace root: config- and ignore-like names probed in the workspace root and above"
clean; export FTRACE_OUT=$S/out/08-typos.trace; rm -f "${FTRACE_OUT:?}"
(cd $REPO && ftrace typos >/dev/null 2>&1)
awk '{ $1=""; print }' $FTRACE_OUT | grep -E 'typos|ignore|Cargo.toml|pyproject|exclude|gitconfig|git/config' | grep -vE 'tools/bin' | strip | sed -E 's/^ +//' | sort | uniq -c | grep -E ' (outer|\$HOME|/etc|/private/tmp/[a-z_.]*$)' | sed 's/^/    /'
clean
