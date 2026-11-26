#!/bin/zsh
# mise, part 2: trust. The trust store (in the scratch home) is emptied before every case, because
# `mise exec` was seen to add to it.
source ${0:A:h}/env.sh
export RUSTUP_HOME=$S/rustup-home; unset RUSTUP_TOOLCHAIN
M=/opt/homebrew/bin/mise
mkfx mise-trust
SH=${S:?}/home
reset() { rm -rf "${SH:?}/.local/state/mise/trusted-configs"; }
wipe() { local g; for g in mise.toml .mise.toml mise.local.toml .tool-versions .miserc.toml rust-toolchain.toml rust-toolchain .rust-version; do rm -f "${REPO:?}/${g:?}" "${MEMBER:?}/${g:?}"; done; rm -rf "${REPO:?}/.config" "${REPO:?}/.mise" "${REPO:?}/evil" "${REPO:?}/mise-tasks"; }
store() { echo "    trust store now: [$(cd $SH/.local/state/mise/trusted-configs 2>/dev/null && print -l *(N) | tr '\n' ' ')]"; }
# t LABEL CMD...: fresh trust store, run CMD in the workspace root with a minimal environment and no terminal
t() { echo "# $1"; shift; reset; (cd $REPO && env -i HOME=$HOME PATH=$PATH RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME "$@" </dev/null 2>&1 | strip | grep -vE 'Version: |Run with --verbose|^Trust them' | head -8 | sed 's/^/    /'; echo "    [exit ${pipestatus[1]}]"); store; }
# k: the same without emptying the trust store first
k() { echo "# $1"; shift; (cd $REPO && env -i HOME=$HOME PATH=$PATH RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME "$@" </dev/null 2>&1 | strip | grep -vE 'Version: |Run with --verbose|^Trust them' | head -8 | sed 's/^/    /'; echo "    [exit ${pipestatus[1]}]"); store; }
echo "## versions"; $M --version
echo; echo "## A. which content an untrusted file may carry (commands that only read: mise ls --current, mise env)"
wipe; printf '[tools]\nrust = "1.21.0"\n' > $REPO/.mise.toml;            t "A1 .mise.toml, [tools] only: mise ls --current" $M ls --current
printf 'rust 1.21.0\n' > $REPO/.tool-versions; rm -f "${REPO:?}/.mise.toml"; t "A2 .tool-versions: mise ls --current" $M ls --current; rm -f "${REPO:?}/.tool-versions"
printf '[env]\nCANARY = "from-.mise.toml"\n' > $REPO/.mise.toml;         t "A3 .mise.toml with [env]: mise env" $M env
                                                                          t "A3b same file: mise ls --current" $M ls --current
printf '[tasks.x]\nrun = "echo TASK-RAN"\n' > $REPO/.mise.toml;          t "A4 .mise.toml with [tasks]: mise ls --current" $M ls --current
                                                                          t "A4b same file: mise tasks ls" $M tasks ls
printf '[hooks]\nenter = "echo HOOK"\n' > $REPO/.mise.toml;              t "A5 .mise.toml with [hooks]: mise ls --current" $M ls --current
printf '[tools]\nrust = "{{ exec(command=\\"echo 1.21.0\\") }}"\n' > $REPO/.mise.toml; t "A6 .mise.toml with a template exec() in [tools]: mise ls --current" $M ls --current
printf '[settings]\nidiomatic_version_file_enable_tools = ["rust"]\n' > $REPO/.mise.toml; t "A7 .mise.toml with [settings] only: mise ls --current" $M ls --current
printf '[settings]\nyes = true\ntrusted_config_paths = ["/"]\nparanoid = false\n[env]\nCANARY = "x"\n' > $REPO/.mise.toml; t "A8 .mise.toml tries to trust itself through [settings]: mise env" $M env
printf '[env]\nCANARY = "x"\n' > $REPO/.mise.toml; printf 'yes = true\ntrusted_config_paths = ["/"]\n' > $REPO/.miserc.toml; t "A9 .miserc.toml: yes = true, trusted_config_paths = [\"/\"]; .mise.toml has [env]: mise env" $M env; rm -f "${REPO:?}/.miserc.toml"
echo; echo "## B. commands that run something, in a directory whose only config is an untrusted .mise.toml with [env] CANARY"
printf '[env]\nCANARY = "from-untrusted-.mise.toml"\n' > $REPO/.mise.toml
t "B1 mise exec -- /usr/bin/env | grep CANARY" sh -c "$M exec -- /usr/bin/env | grep -E 'CANARY|ERROR|WARN'"
k "B1b straight after, same trust store: mise env | grep CANARY" sh -c "$M env | grep -E 'CANARY|ERROR'"
k "B1c mise trust --show" $M trust --show
t "B2 MISE_PARANOID=1 mise exec -- /usr/bin/env | grep CANARY" env MISE_PARANOID=1 sh -c "$M exec -- /usr/bin/env 2>&1 | grep -E 'CANARY|ERROR|WARN'"
t "B3 MISE_YES=0 mise exec" env MISE_YES=0 sh -c "$M exec -- /usr/bin/env 2>&1 | grep -E 'CANARY|ERROR|WARN'"
t "B4 MISE_IGNORED_CONFIG_PATHS=<repo> mise exec" env MISE_IGNORED_CONFIG_PATHS=$REPO sh -c "$M exec -- /usr/bin/env 2>&1 | grep -E 'CANARY|ERROR|WARN'; echo exec-finished"
t "B5 MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml mise exec (the untrusted file is .mise.toml)" env MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml sh -c "$M exec -- /usr/bin/env 2>&1 | grep -E 'CANARY|ERROR|WARN'; echo exec-finished"
t "B6 MISE_TRUSTED_CONFIG_PATHS=/nonexistent mise exec" env MISE_TRUSTED_CONFIG_PATHS=/nonexistent sh -c "$M exec -- /usr/bin/env 2>&1 | grep -E 'CANARY|ERROR|WARN'"
printf '[env]\nCANARY = "x"\n[tasks.x]\nrun = "echo TASK-RAN with CANARY=$CANARY"\n' > $REPO/.mise.toml
t "B7 mise run x (a task defined in the untrusted file)" $M run x
t "B8 mise tasks ls" $M tasks ls
printf '[env]\nCANARY = "{{ exec(command=\\"echo TEMPLATE-EXEC-RAN\\") }}"\n' > $REPO/.mise.toml
t "B9 [env] with a template exec(): mise exec -- /usr/bin/env | grep CANARY" sh -c "$M exec -- /usr/bin/env 2>&1 | grep -E 'CANARY|ERROR|WARN'"
printf '[env]\nRUSTC_WRAPPER = "/usr/bin/true"\nCLIPPY_CONF_DIR = "/tmp"\n_.path = ["./evil/bin"]\n' > $REPO/.mise.toml; mkdir -p $REPO/evil/bin; printf '#!/bin/sh\necho "PWNED: evil/bin/cargo ran with: $*"\n' > $REPO/evil/bin/cargo; chmod +x $REPO/evil/bin/cargo
t "B10 untrusted .mise.toml: [env] _.path = [\"./evil/bin\"] (holds a script named cargo), RUSTC_WRAPPER, CLIPPY_CONF_DIR: mise exec -- cargo clippy --workspace" $M exec -- cargo clippy --workspace
t "B10b same: mise exec -- /usr/bin/env | grep -E 'RUSTC_WRAPPER|CLIPPY'" sh -c "$M exec -- /usr/bin/env | grep -E 'RUSTC_WRAPPER|CLIPPY'"
echo; echo "## C. what an existing trust covers"
wipe; printf '[tools]\nrust = "1.100.0"\n[env]\nCANARY = "protected-mise.toml"\n' > $REPO/mise.toml
t "C0 before trust: mise env" $M env
k "C1 mise trust (workspace root, no argument)" $M trust
k "C1b mise trust --show" $M trust --show
k "C2 after trust: mise env | grep CANARY" sh -c "$M env | grep CANARY"
printf '[env]\nCANARY = "protected-mise.toml-CHANGED-AFTER-TRUST"\nEVIL = "1"\n' > $REPO/mise.toml; k "C3 the trusted file's content changes: mise env | grep -E 'CANARY|EVIL'" sh -c "$M env | grep -E 'CANARY|EVIL'"
printf '[env]\nCANARY = "protected-mise.toml"\n' > $REPO/mise.toml
for sib in mise.local.toml .mise.toml .mise.local.toml .config/mise.toml .config/mise/conf.d/x.toml .mise/config.toml; do
  mkdir -p $REPO/${sib:h}; printf '[env]\nCANARY = "sibling-%s"\nSIBLING = "loaded"\n' $sib > $REPO/$sib; k "C4 a NEW sibling $sib with [env] beside the trusted mise.toml: mise env" sh -c "$M env 2>&1 | grep -E 'CANARY|SIBLING|not trusted'"; rm -f "${REPO:?}/${sib:?}"
done
rm -rf "${REPO:?}/.config" "${REPO:?}/.mise"
printf '[env]\nCANARY = "member-.mise.toml"\n' > $MEMBER/.mise.toml; echo "# C5 a new member/.mise.toml with [env], cwd = member, workspace root trusted"; (cd $MEMBER && env -i HOME=$HOME PATH=$PATH $M env 2>&1 | strip | grep -E 'CANARY|not trusted' | sed 's/^/    /'); rm -f "${MEMBER:?}/.mise.toml"
k "C6 MISE_PARANOID=1, trusted file unchanged: mise env" env MISE_PARANOID=1 sh -c "$M env 2>&1 | grep -E 'CANARY|not trusted'"
echo; echo "## D. variables that grant trust (untrusted .mise.toml with [env])"
wipe; printf '[env]\nCANARY = "sibling-.mise.toml"\n' > $REPO/.mise.toml
t "D0 control: mise env" sh -c "$M env 2>&1 | grep -E 'CANARY|not trusted'"
t "D1 MISE_YES=1 mise env" env MISE_YES=1 sh -c "$M env 2>&1 | grep -E 'CANARY|not trusted'"
t "D2 MISE_TRUSTED_CONFIG_PATHS=<repo> mise env" env MISE_TRUSTED_CONFIG_PATHS=$REPO sh -c "$M env 2>&1 | grep -E 'CANARY|not trusted'"
t "D3 CI=true mise env" env CI=true sh -c "$M env 2>&1 | grep -E 'CANARY|not trusted'"
echo; echo "## E. idiomatic version files, enabled by the operator (MISE_IDIOMATIC_VERSION_FILE_ENABLE_TOOLS=rust)"
wipe; printf '[toolchain]\nchannel = "1.41.0"\n' > $REPO/rust-toolchain.toml
t "E1 only rust-toolchain.toml=1.41.0, default settings: mise ls --current" $M ls --current
t "E2 same, with the variable" env MISE_IDIOMATIC_VERSION_FILE_ENABLE_TOOLS=rust $M ls --current
echo 1.42.0 > $REPO/rust-toolchain; t "E3 plus rust-toolchain=1.42.0" env MISE_IDIOMATIC_VERSION_FILE_ENABLE_TOOLS=rust $M ls --current
echo 1.43.0 > $REPO/.rust-version; t "E4 plus .rust-version=1.43.0" env MISE_IDIOMATIC_VERSION_FILE_ENABLE_TOOLS=rust $M ls --current
printf '[tools]\nrust = "1.100.0"\n' > $REPO/mise.toml; t "E5 plus mise.toml=1.100.0" env MISE_IDIOMATIC_VERSION_FILE_ENABLE_TOOLS=rust $M ls --current
echo; echo "## F. a tools-only (needs no trust) file that points a tool at a directory in the tree"
wipe; mkdir -p $REPO/evil/bin; printf '#!/bin/sh\necho "PWNED: evil/bin/cargo ran with: $*"\n' > $REPO/evil/bin/cargo; chmod +x $REPO/evil/bin/cargo
printf '[tools]\nnode = "path:%s/evil"\n' $REPO > $REPO/.mise.toml
t "F1 .mise.toml: [tools] node = \"path:<repo>/evil\" (evil/bin/cargo is a script): mise ls --current" $M ls --current
t "F1b same: MISE_AUTO_INSTALL=0 mise exec -- cargo clippy --workspace" env MISE_AUTO_INSTALL=0 $M exec -- cargo clippy --workspace
printf 'node path:%s/evil\n' $REPO > $REPO/.tool-versions; rm -f "${REPO:?}/.mise.toml"
t "F2 the same request in .tool-versions: mise exec -- cargo clippy --workspace" env MISE_AUTO_INSTALL=0 $M exec -- cargo clippy --workspace
t "F2b same with MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none" env MISE_AUTO_INSTALL=0 MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none $M exec -- cargo --version
wipe; reset
