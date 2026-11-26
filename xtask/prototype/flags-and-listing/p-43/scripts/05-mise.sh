#!/bin/zsh
# mise: every configuration file name it loads in a project directory, order, trust, and the MISE_* variables.
# HOME is the scratch home, so mise's config, data, state and cache directories are all inside the scratch directory.
source ${0:A:h}/env.sh
export RUSTUP_HOME=$S/rustup-home; unset RUSTUP_TOOLCHAIN
M=/opt/homebrew/bin/mise
mkfx mise
SH=${S:?}/home                      # the scratch home, spelled without $HOME for every removal below
untrust_all() { rm -rf "${SH:?}/.local/state/mise/trusted-configs"; }
untrust_all; rm -rf "${SH:?}/.config/mise"
echo "## versions"; $M --version; echo "MISE_* variables in this run: $(env | grep -c '^MISE_')"
$M doctor 2>&1 | strip | sed -n '/^dirs:/,/^$/p'
POS=( mise.toml .mise.toml mise.local.toml .mise.local.toml mise/config.toml .mise/config.toml .config/mise.toml .config/mise/config.toml .config/mise/mise.toml
      mise/config.local.toml .mise/config.local.toml .config/mise.local.toml .config/mise/config.local.toml .config/mise/mise.local.toml
      mise/conf.d/a.toml .mise/conf.d/a.toml .config/mise/conf.d/a.toml mise/conf.d/sub/mise.toml .tool-versions )
NEG=( .rtx.toml rust-toolchain.toml rust-toolchain .rust-version mise.ci.toml .mise.ci.toml mise.ci.local.toml .config/mise/config.ci.toml .config/mise/conf.d/a.ci.toml mise.json mise.yaml .config/mise/tools.toml config.toml .mise-version mise.lock Mise.toml )
RC=( evil.toml .miserc.toml .miserc.local.toml miserc.toml .config/miserc.toml .mise.ci.local.toml )
clean() { local d g; for d in $REPO $MEMBER $OUTER; do for g in $POS $NEG $RC; do rm -f "${d:?}/${g:?}"; done; rm -rf "${d:?}/mise" "${d:?}/.mise" "${d:?}/.config" "${d:?}/evil-tc"; done; }
# put DIR NAME N: the file asks for rust 1.N.0 in whatever syntax its name implies. A tools-only file needs no trust.
put() { local f="$1/$2"; mkdir -p "${f:h}"; case $2 in
    .tool-versions) echo "rust 1.$3.0" > $f;; rust-toolchain.toml) printf '[toolchain]\nchannel = "1.%s.0"\n' $3 > $f;; rust-toolchain|.rust-version|.mise-version) echo "1.$3.0" > $f;;
    mise.lock) printf '[[tools.rust]]\nversion = "1.%s.0"\nbackend = "core:rust"\n' $3 > $f;; *) printf '[tools]\nrust = "1.%s.0"\n' $3 > $f;; esac; }
cur() { local dir=$1; shift; echo "\$ (cd ${dir#$FX/} && $* mise ls --current)" | strip; (cd $dir && env "$@" $M ls --current 2>&1 | strip | grep -v 'Version: \|Run with --verbose' | sed 's/^/    /'); }
echo; echo "## A. baseline: no file"; clean; cur $REPO
echo; echo "## B. one candidate at a time, cwd = workspace root (each asks for a distinct rust version; the source column names the file)"
n=10; for f in $POS $NEG; do n=$((n+1)); clean; put $REPO $f $n; echo; echo "# only $f (rust 1.$n.0)"; cur $REPO; done
echo; echo "## C. every loaded name present at once: order of precedence"
clean; n=10; for f in $POS; do n=$((n+1)); put $REPO $f $n; echo "    $f -> rust 1.$n.0"; done
echo "\$ mise cfg"; (cd $REPO && $M cfg 2>&1 | strip | sed 's/^/    /')
cur $REPO
echo "# knock-out: delete the winner, ask again, until none is left"
for i in {1..25}; do w=$(cd $REPO && $M ls --current 2>/dev/null | awk '{print $(NF-1)}' | head -1); [[ "$w" == $REPO/* && -f "$w" ]] || break; echo "    winner: ${w#$REPO/}"; rm -f "${w:?}"; done
echo; echo "## D. across directories"
clean; put $REPO mise.toml 21; put $MEMBER .mise.toml 22; put $OUTER mise.local.toml 23
echo "# repo/mise.toml=1.21.0, member/.mise.toml=1.22.0, outer/mise.local.toml=1.23.0"; cur $REPO; cur $MEMBER
echo "\$ (cd member && mise cfg)"; (cd $MEMBER && $M cfg 2>&1 | strip | sed 's/^/    /')
clean; put $OUTER .tool-versions 23; echo "# only outer/.tool-versions (above the git root)"; cur $MEMBER
clean; mkdir -p $SH/.config/mise; printf '[tools]\nrust = "1.30.0"\n' > $SH/.config/mise/config.toml; put $REPO .config/mise.toml 21
echo "# global \$HOME/.config/mise/config.toml=1.30.0 and repo/.config/mise.toml=1.21.0"; cur $REPO; rm -rf "${SH:?}/.config/mise"
echo; echo "## E. MISE_* variables against siblings (tools-only files: mise.toml=1.100.0 is the protected one)"
setup() { clean; put $REPO mise.toml 100; put $REPO .mise.toml 21; put $REPO mise.local.toml 22; put $REPO .config/mise.toml 23; put $REPO .tool-versions 24; put $REPO .config/mise/conf.d/a.toml 25; put $MEMBER .mise.local.toml 26; }
setup; echo "# E0 control: no variable"; cur $REPO; cur $MEMBER
echo "# E1 MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml"; cur $REPO MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml; cur $MEMBER MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml
echo "# E2 plus MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none"; cur $REPO MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none; cur $MEMBER MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none
put $MEMBER mise.toml 27; echo "# E2b same variables, and a member/mise.toml=1.27.0 exists"; cur $MEMBER MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none; rm -f "${MEMBER:?}/mise.toml"
echo "# E3 MISE_OVERRIDE_CONFIG_FILENAMES=<absolute path of repo/mise.toml>"; cur $MEMBER MISE_OVERRIDE_CONFIG_FILENAMES=$REPO/mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none
echo "# E4 MISE_CONFIG_FILE=<repo/mise.toml> (is there such a variable?)"; cur $REPO MISE_CONFIG_FILE=$REPO/mise.toml
echo "# E5 MISE_GLOBAL_CONFIG_FILE=<repo/mise.toml>"; cur $REPO MISE_GLOBAL_CONFIG_FILE=$REPO/mise.toml
echo "# E6 MISE_IGNORED_CONFIG_PATHS=<repo> (the whole tree ignored)"; cur $MEMBER MISE_IGNORED_CONFIG_PATHS=$REPO
echo "# E7 MISE_IGNORED_CONFIG_PATHS=<repo> with MISE_GLOBAL_CONFIG_FILE=<a file outside the tree asking 1.100.0>"; mkdir -p $FX/trusted; printf '[tools]\nrust = "1.100.0"\n' > $FX/trusted/mise.toml; cur $MEMBER MISE_IGNORED_CONFIG_PATHS=$REPO MISE_GLOBAL_CONFIG_FILE=$FX/trusted/mise.toml
echo "# E8 MISE_RUST_VERSION=1.100.0 (per-tool variable)"; cur $MEMBER MISE_RUST_VERSION=1.100.0
echo "# E9 MISE_CEILING_PATHS=<repo>, cwd = member (files at the ceiling and above are not read?)"; cur $MEMBER MISE_CEILING_PATHS=$REPO
echo "# E10 MISE_NO_CONFIG=1"; cur $REPO MISE_NO_CONFIG=1
echo "# E11 MISE_ENV=ci with mise.ci.toml=1.31.0 and .mise.ci.local.toml=1.32.0 present"; put $REPO mise.ci.toml 31; put $REPO .mise.ci.local.toml 32; cur $REPO MISE_ENV=ci; cur $REPO
echo; echo "## F. .miserc.toml in the tree (read before the config files; needs no trust)"
setup
echo 'override_config_filenames = ["evil.toml"]' > $REPO/.miserc.toml; put $REPO evil.toml 66; echo "# F1 repo/.miserc.toml: override_config_filenames = [\"evil.toml\"], evil.toml=1.66.0"; cur $REPO
echo "# F1b same, with MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml in the environment"; cur $REPO MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml
echo 'ignored_config_paths = ["'$REPO'/mise.toml"]' > $REPO/.miserc.toml; rm -f "${REPO:?}/.mise.toml" "${REPO:?}/mise.local.toml" "${REPO:?}/.tool-versions" "${MEMBER:?}/.mise.local.toml"; rm -rf "${REPO:?}/.config"; echo "# F2 .miserc.toml: ignored_config_paths = [<repo/mise.toml>] (only mise.toml=1.100.0 present otherwise)"; cur $REPO
echo "# F2b same, with MISE_IGNORED_CONFIG_PATHS=/nonexistent in the environment"; cur $REPO MISE_IGNORED_CONFIG_PATHS=/nonexistent
echo 'env = ["ci"]' > $REPO/.miserc.toml; put $REPO mise.ci.toml 31; echo "# F3 .miserc.toml: env = [\"ci\"], mise.ci.toml=1.31.0"; cur $REPO; echo "# F3b same with MISE_ENV=prod in the environment"; cur $REPO MISE_ENV=prod
rm -f "${REPO:?}/.miserc.toml" "${REPO:?}/mise.ci.toml"
for rc in .miserc.local.toml .config/miserc.toml miserc.toml; do mkdir -p $REPO/${rc:h}; echo 'override_config_filenames = ["evil.toml"]' > $REPO/$rc; echo "# F4 the same override in repo/$rc"; cur $REPO; rm -f "${REPO:?}/${rc:?}"; done
echo 'override_config_filenames = ["evil.toml"]' > $OUTER/.miserc.toml; echo "# F5 the override in outer/.miserc.toml (above the git root), cwd = member"; cur $MEMBER; rm -f "${OUTER:?}/.miserc.toml"
echo 'override_config_filenames = ["evil.toml"]' > $MEMBER/.miserc.toml; put $MEMBER evil.toml 67; echo "# F6 the override in member/.miserc.toml, cwd = workspace root, then cwd = member"; cur $REPO; cur $MEMBER; rm -f "${MEMBER:?}/.miserc.toml" "${MEMBER:?}/evil.toml"
clean
echo; echo "## G. .miserc.toml against the operator's variables (MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none throughout)"
V=(MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none)
clean; put $REPO mise.toml 100
echo "# G0 control"; cur $REPO $V
echo 'ignored_config_paths = ["'$REPO'/mise.toml"]' > $REPO/.miserc.toml; echo "# G1 .miserc.toml: ignored_config_paths = [<repo/mise.toml>]"; cur $REPO $V
echo "# G1b plus MISE_IGNORED_CONFIG_PATHS=/nonexistent"; cur $REPO $V MISE_IGNORED_CONFIG_PATHS=/nonexistent
echo 'env = ["ci"]' > $REPO/.miserc.toml; put $REPO mise.ci.toml 31; echo "# G2 .miserc.toml: env = [\"ci\"], mise.ci.toml=1.31.0"; cur $REPO $V
echo "# G2b plus MISE_ENV= (set, empty)"; cur $REPO $V MISE_ENV=
rm -f "${REPO:?}/mise.ci.toml"
echo 'ceiling_paths = ["'$REPO'"]' > $REPO/.miserc.toml; echo "# G3 .miserc.toml: ceiling_paths = [<repo>]"; cur $REPO $V; cur $MEMBER $V
echo 'ceiling_paths = ["'$MEMBER'"]' > $MEMBER/.miserc.toml; rm -f "${REPO:?}/.miserc.toml"; echo "# G4 member/.miserc.toml: ceiling_paths = [<member>], cwd = member"; cur $MEMBER $V; rm -f "${MEMBER:?}/.miserc.toml"
printf 'idiomatic_version_file_enable_tools = ["rust"]\nno_config = true\nglobal_config_file = "%s/evil.toml"\n' $REPO > $REPO/.miserc.toml; put $REPO evil.toml 66; echo "# G5 .miserc.toml: no_config / global_config_file / idiomatic_version_file_enable_tools"; cur $REPO $V
echo 'no_such_key = 1' > $REPO/.miserc.toml; echo "# G6 .miserc.toml with an unknown key"; cur $REPO $V
echo "# G7 MISE_NO_CONFIG... is there a variable that stops .miserc.toml being read? trace of names opened with the variables set:"
echo 'ignored_config_paths = ["'$REPO'/mise.toml"]' > $REPO/.miserc.toml
export FTRACE_OUT=$S/out/05-mise-rc.trace; rm -f "${FTRACE_OUT:?}"; (cd $REPO && env $V MISE_IGNORED_CONFIG_PATHS=/nonexistent DYLD_INSERT_LIBRARIES=$S/trace/libftrace.dylib $M ls --current >/dev/null 2>&1)
grep -F "$REPO/" $FTRACE_OUT | grep -v '/member\|/.git' | awk '{ $1=""; print }' | strip | sort | uniq -c | sed 's/^/    /'
clean
echo; echo "## H. file-lookup trace in an empty project directory (mise ls --current; mise env; mise tasks ls): names probed in the workspace root"
export FTRACE_OUT=$S/out/05-mise.trace; rm -f "${FTRACE_OUT:?}"
(cd $REPO && ftrace $M ls --current >/dev/null 2>&1; ftrace $M env >/dev/null 2>&1; ftrace $M tasks ls >/dev/null 2>&1)
grep -F "$REPO/" $FTRACE_OUT | grep -v '/member\|/.git' | awk '{ $1=""; print }' | strip | sort | uniq -c | sed 's/^/    /'
echo "# names probed outside the tree (home, /etc):"; awk '{ $1=""; print }' $FTRACE_OUT | grep -E "^ [a-z]+ [A-Z/]+ ($HOME/.config|/etc/mise)" | strip | sort -u | sed 's/^/    /'
