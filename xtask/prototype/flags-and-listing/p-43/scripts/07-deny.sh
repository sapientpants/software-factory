#!/bin/zsh
# cargo-deny: configuration files read unasked, order, what --config does. Run for the release named in $1 (default: the scratch 0.22.1).
source ${0:A:h}/env.sh
DENY=${1:-$S/tools/bin/cargo-deny}
mkfx deny-${DENY:h:h:t}
echo "## versions"; $DENY --version; cargo --version; echo "binary: $DENY" | sed "s|<home>|~|"
ALL=( $REPO/deny.toml $REPO/.deny.toml $REPO/.cargo/deny.toml $MEMBER/deny.toml $MEMBER/.deny.toml $MEMBER/.cargo/deny.toml $OUTER/deny.toml $OUTER/.deny.toml $OUTER/.cargo/deny.toml $FX/deny.toml
      $HOME/deny.toml $HOME/.deny.toml $HOME/.cargo/deny.toml $HOME/.config/cargo-deny/deny.toml $HOME/.config/deny.toml $REPO/.config/deny.toml $REPO/cargo-deny.toml $REPO/.cargo-deny.toml $REPO/.deny $REPO/Deny.TOML )
clean() { local g; for g in $ALL; do rm -f "${g:?}"; done; rm -rf "${REPO:?}/.cargo" "${REPO:?}/.config" "${MEMBER:?}/.cargo" "${OUTER:?}/.cargo"; }
# put FILE LICENSE: FILE allows MIT (the crate's licence; with no config it is rejected, exit 4) plus one licence nobody uses, which the
# "license-not-encountered" warning then prints together with the path of the file that was read.
put() { mkdir -p "${1:h}"; printf '[licenses]\nallow = ["MIT", "%s"]\n' ${2:-Zlib} > "$1"; }
go() { local dir=$1; shift; echo "\$ (cd ${dir#$FX/} && cargo deny $*)" | strip; (cd $dir && $DENY -L info "$@" 2>&1 | strip | grep -E 'using config|unable to find|^(error|warning)|┌─|allow = |FAILED|ok$' | sed -E 's/^[0-9-]+ [0-9:]+ //' | head -8 | sed 's/^/    /'; echo "    -> exit ${pipestatus[1]}"); }
# 0.22 takes --config before the subcommand, 0.18 after it
if $DENY --help 2>&1 | grep -q -- '--config'; then cfg() { local d=$1 c=$2; shift 2; go $d --config $c check "$@"; }; else cfg() { local d=$1 c=$2; shift 2; go $d check --config $c "$@"; }; fi
echo "where --config goes in this release: $($DENY --help 2>&1 | grep -q -- '--config' && echo 'before the subcommand (cargo deny --config F check)' || echo 'after it (cargo deny check --config F)')"
echo; echo "## A. baseline, no config"; clean; go $REPO check licenses
echo; echo "## B. one candidate at a time, cwd = workspace root"
for f in $ALL; do clean; put "$f"; echo; echo "# only $f" | strip; go $REPO check licenses; done
echo; echo "## C. precedence in one directory (each file allows a different unused licence)"
clean; put $REPO/deny.toml Zlib; put $REPO/.deny.toml ISC; put $REPO/.cargo/deny.toml 0BSD; echo "# deny.toml=Zlib, .deny.toml=ISC, .cargo/deny.toml=0BSD"; go $REPO check licenses
rm -f "${REPO:?}/deny.toml"; echo "# .deny.toml=ISC, .cargo/deny.toml=0BSD"; go $REPO check licenses
echo "# is a second file merged? .deny.toml allows only Apache-2.0, .cargo/deny.toml allows MIT"; printf '[licenses]\nallow = ["Apache-2.0"]\n' > $REPO/.deny.toml; go $REPO check licenses
clean; put $REPO/.cargo/deny.toml 0BSD; put $REPO/.config/deny.toml BSL-1.0; echo "# .cargo/deny.toml=0BSD, .config/deny.toml=BSL-1.0"; go $REPO check licenses
echo; echo "## D. across directories"
clean; put $REPO/deny.toml Zlib; put $MEMBER/.deny.toml ISC; put $OUTER/.cargo/deny.toml 0BSD
echo "# repo/deny.toml=Zlib, member/.deny.toml=ISC, outer/.cargo/deny.toml=0BSD; cwd = workspace root"; go $REPO check licenses
echo "# same, cwd = member"; go $MEMBER check licenses
echo "# same, cwd outside the tree, --manifest-path repo/Cargo.toml"; go $S --manifest-path $REPO/Cargo.toml check licenses
echo "# same, cwd = workspace root, --manifest-path member/Cargo.toml"; go $REPO --manifest-path $MEMBER/Cargo.toml check licenses
rm -f "${REPO:?}/deny.toml" "${MEMBER:?}/.deny.toml"; echo "# only outer/.cargo/deny.toml=0BSD (above the git root), cwd = member"; go $MEMBER check licenses
echo; echo "## E. --config with siblings present"
clean; put $REPO/deny.toml Zlib; printf '[licenses]\nallow = ["MIT", "ISC"]\n[licenses.private]\nignore = true\n' > $REPO/.deny.toml; put $MEMBER/.deny.toml 0BSD; mkdir -p $FX/trusted; printf '[licenses]\nallow = ["Apache-2.0"]\n' > $FX/trusted/deny.toml
echo "# E1 --config deny.toml (relative, the protected file), .deny.toml and member/.deny.toml present"; cfg $REPO deny.toml licenses
echo "# E2 same from cwd = member with --config ../deny.toml"; cfg $MEMBER ../deny.toml licenses
echo "# E3 --config <file outside the tree that allows only Apache-2.0>"; cfg $REPO $FX/trusted/deny.toml licenses
echo "# E4 --config pointing at a file that does not exist"; cfg $REPO $FX/trusted/nope.toml licenses
echo "# E5 the other placement of --config in this release"; go $REPO check --config deny.toml licenses; go $REPO --config deny.toml check licenses
echo; echo "## F. other inputs"
clean; printf '\n[package.metadata.cargo-deny]\nlicenses = { allow = ["MIT"] }\n[package.metadata.deny.licenses]\nallow = ["MIT"]\n' >> $MEMBER/Cargo.toml; printf '\n[workspace.metadata.cargo-deny.licenses]\nallow = ["MIT"]\n[workspace.metadata.deny.licenses]\nallow = ["MIT"]\n' >> $REPO/Cargo.toml
echo "# F1 [package.metadata.cargo-deny] / [package.metadata.deny] / [workspace.metadata.*] tables, no config file"; go $REPO check licenses
sed -i '' '/^\[package.metadata.cargo-deny\]/,$d' $MEMBER/Cargo.toml; sed -i '' '/^\[workspace.metadata.cargo-deny.licenses\]/,$d' $REPO/Cargo.toml
echo "# F2 the protected deny.toml sets [licenses.private] ignore = true and allows nothing; member has publish = false (control), then the attacker ... nothing to add: recorded for the Cargo.toml side"
printf '[licenses]\nallow = []\n[licenses.private]\nignore = true\n' > $REPO/deny.toml; go $REPO check licenses
echo "# F3 full 'cargo deny check' with the canary deny.toml (advisory database is fetched into the scratch CARGO_HOME)"; put $REPO/deny.toml Zlib; go $REPO check
echo; echo "## F2. the exceptions file (deny.exceptions.toml and its other names): the protected deny.toml allows only Apache-2.0, so member (MIT) is rejected"
EXC=( deny.exceptions.toml .deny.exceptions.toml .cargo/deny.exceptions.toml .config/deny.exceptions.toml )
xclean() { local g; clean; for g in $EXC; do rm -f "${REPO:?}/${g:?}" "${MEMBER:?}/${g:?}" "${OUTER:?}/${g:?}"; done; rm -rf "${REPO:?}/.cargo" "${REPO:?}/.config"; printf '[licenses]\nallow = ["Apache-2.0"]\n' > $REPO/deny.toml; }
xput() { mkdir -p "${1:h}"; printf 'exceptions = [{ allow = ["MIT"], crate = "member" }]\n' > "$1"; }
xclean; echo "# control: deny.toml alone"; go $REPO check licenses
for x in $EXC; do xclean; xput $REPO/$x; echo "# plus repo/$x holding an exception for member"; go $REPO check licenses; done
xclean; xput $MEMBER/deny.exceptions.toml; echo "# plus member/deny.exceptions.toml, cwd = workspace root"; go $REPO check licenses
xclean; xput $OUTER/deny.exceptions.toml; echo "# plus outer/deny.exceptions.toml (a directory above the config)"; go $REPO check licenses
xclean; xput $REPO/.deny.exceptions.toml; echo "# plus repo/.deny.exceptions.toml, with --config deny.toml given explicitly"; cfg $REPO deny.toml licenses
xclean; xput $REPO/.deny.exceptions.toml; mkdir -p $FX/trusted; printf '[licenses]\nallow = ["Apache-2.0"]\n' > $FX/trusted/deny.toml; echo "# plus repo/.deny.exceptions.toml, with --config <the same policy in a file outside the tree>"; cfg $REPO $FX/trusted/deny.toml licenses
xclean; rm -f "${REPO:?}/deny.toml"; xput $REPO/deny.exceptions.toml; echo "# an exceptions file with no config file at all"; go $REPO check licenses
for g in $EXC; do rm -f "${REPO:?}/${g:?}" "${MEMBER:?}/${g:?}" "${OUTER:?}/${g:?}"; done
echo; echo "## G. file-lookup trace, no config, cwd = member: deny-named paths probed"
clean; export FTRACE_OUT=$S/out/07-deny-${DENY:h:h:t}.trace; rm -f "${FTRACE_OUT:?}"
(cd $MEMBER && DYLD_INSERT_LIBRARIES=$S/trace/libftrace.dylib $DENY check licenses >/dev/null 2>&1)
grep -iE 'deny' $FTRACE_OUT | grep -vE '/bin/|advisory' | awk '{ $1=""; print }' | sed "s|$FX/||g; s|$HOME|\$HOME|g; s|$S|\$S|g" | grep -E 'deny[^/]*\.toml$' | sort | uniq -c | sed 's/^/    /'
