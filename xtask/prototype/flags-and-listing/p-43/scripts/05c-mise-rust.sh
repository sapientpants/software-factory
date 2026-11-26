#!/bin/zsh
# mise, part 3: the chain mise -> rustup -> cargo, with rust 1.100.0 installed under the scratch home's mise
# (mise install rust@1.100.0 was run once with HOME, CARGO_HOME and RUSTUP_HOME inside the scratch directory).
source ${0:A:h}/env.sh
export RUSTUP_HOME=$S/rustup-home; unset RUSTUP_TOOLCHAIN
M=/opt/homebrew/bin/mise; SH=${S:?}/home
mkfx mise-rust
E=(env -i HOME=$HOME PATH=$S/tools/bin:/opt/homebrew/bin:/usr/bin:/bin RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME MISE_AUTO_INSTALL=0)
reset() { rm -rf "${SH:?}/.local/state/mise/trusted-configs"; }
t() { echo "# $1"; shift; reset; (cd $REPO && $E "$@" </dev/null 2>&1 | strip | grep -vE 'Version: |Run with --verbose|^Trust them' | head -8 | sed 's/^/    /'; echo "    [exit ${pipestatus[1]}]"); }
echo "## versions"; $M --version; $E $M ls 2>&1 | strip
printf '[tools]\nrust = "1.100.0"\n' > $REPO/mise.toml
echo; echo "## A. the protected mise.toml alone ([tools] rust = \"1.100.0\")"
t "A1 mise exec -- rustc --version" $M exec -- rustc --version
t "A2 mise exec -- /usr/bin/env | grep -E 'RUSTUP|CARGO_HOME'" sh -c "$M exec -- /usr/bin/env | grep -E '^(RUSTUP|CARGO_HOME)'"
t "A3 mise env | grep RUSTUP_TOOLCHAIN" sh -c "$M env | grep RUSTUP"
echo; echo "## B. toolchain files beside it, under mise exec (canary-b is rustc 1.97.1)"
printf '[toolchain]\nchannel = "canary-b"\n' > $REPO/rust-toolchain.toml; t "B1 rust-toolchain.toml = canary-b: mise exec -- rustc --version" $M exec -- rustc --version
echo canary-b > $REPO/rust-toolchain; t "B2 plus rust-toolchain = canary-b: mise exec -- cargo --version" $M exec -- cargo --version
mkdir -p $REPO/evil-tc/bin; printf '#!/bin/sh\necho "PWNED: evil-tc/bin/cargo ran with: $*"\n' > $REPO/evil-tc/bin/cargo; chmod +x $REPO/evil-tc/bin/cargo
printf '[toolchain]\npath = "%s/evil-tc"\n' $REPO > $REPO/rust-toolchain; t "B3 rust-toolchain: [toolchain] path = <repo>/evil-tc: mise exec -- cargo clippy" $M exec -- cargo clippy
echo "# B4 control: the same tree without mise (rustup proxy called directly, RUSTUP_TOOLCHAIN unset)"; (cd $REPO && env -i HOME=$HOME PATH=$CARGO_HOME/bin:/usr/bin:/bin RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME cargo clippy 2>&1 | strip | head -3 | sed 's/^/    /')
rm -f "${REPO:?}/rust-toolchain" "${REPO:?}/rust-toolchain.toml"; rm -rf "${REPO:?}/evil-tc"
echo; echo "## C. unprotected mise names beside the protected mise.toml, under mise exec"
printf '[tools]\nrust = "canary-b"\n' > $REPO/.mise.toml; t "C1 .mise.toml: [tools] rust = \"canary-b\": mise ls --current; mise exec -- rustc --version" sh -c "$M ls --current; $M exec -- rustc --version"
rm -f "${REPO:?}/.mise.toml"; echo "rust canary-b" > $REPO/.tool-versions; t "C2 .tool-versions: rust canary-b" sh -c "$M ls --current; $M exec -- rustc --version"
rm -f "${REPO:?}/.tool-versions"; printf '[env]\nRUSTUP_TOOLCHAIN = "canary-b"\n' > $REPO/mise.local.toml; t "C3 mise.local.toml: [env] RUSTUP_TOOLCHAIN = \"canary-b\" (needs trust; exec grants it): mise exec -- rustc --version" $M exec -- rustc --version
t "C3b same with MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none" env MISE_OVERRIDE_CONFIG_FILENAMES=mise.toml MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES=none $M exec -- rustc --version
t "C3c same with MISE_PARANOID=1" env MISE_PARANOID=1 $M exec -- rustc --version
rm -f "${REPO:?}/mise.local.toml"
echo; echo "## D. shims (mise reshim in the scratch home; the shim directory first on PATH)"
$E $M reshim 2>&1 | strip; ls $SH/.local/share/mise/shims | tr '\n' ' '; echo
SE=(env -i HOME=$HOME PATH=$SH/.local/share/mise/shims:/opt/homebrew/bin:/usr/bin:/bin RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME MISE_AUTO_INSTALL=0)
s() { echo "# $1"; shift; reset; (cd $REPO && $SE "$@" </dev/null 2>&1 | strip | grep -vE 'Version: |Run with --verbose|^Trust them' | head -8 | sed 's/^/    /'; echo "    [exit ${pipestatus[1]}]"); echo "    trust store now: [$(cd $SH/.local/state/mise/trusted-configs 2>/dev/null && print -l *(N) | tr '\n' ' ')]"; }
s "D1 shim, protected mise.toml only: rustc --version" rustc --version
printf '[env]\nRUSTUP_TOOLCHAIN = "canary-b"\nCANARY = "1"\n' > $REPO/mise.local.toml; s "D2 shim, plus untrusted mise.local.toml [env] RUSTUP_TOOLCHAIN=canary-b: rustc --version" rustc --version
rm -f "${REPO:?}/mise.local.toml"; echo canary-b > $REPO/rust-toolchain; s "D3 shim, plus rust-toolchain = canary-b: rustc --version" rustc --version
rm -f "${REPO:?}/rust-toolchain"; mkdir -p $REPO/evil/bin; printf '#!/bin/sh\necho "PWNED: evil/bin/cargo ran with: $*"\n' > $REPO/evil/bin/cargo; chmod +x $REPO/evil/bin/cargo
printf 'node path:%s/evil\n' $REPO > $REPO/.tool-versions; s "D4 shim, plus .tool-versions: node path:<repo>/evil (evil/bin/cargo is a script): cargo clippy" cargo clippy
rm -f "${REPO:?}/.tool-versions"; rm -rf "${REPO:?}/evil"; reset
