#!/bin/zsh
# cargo doc / rustdoc: inputs read unasked.
source ${0:A:h}/env.sh
mkfx doc
export CARGO_TARGET_DIR=$S/target/doc
echo "## versions"; cargo --version; rustdoc --version
src() { printf '%s/// See [`nope`].\npub fn f() -> u32 {\n    1\n}\n' "$1" > $MEMBER/src/lib.rs; }
clean() { rm -rf "${REPO:?}/.cargo" "${MEMBER:?}/.cargo"; local g; for g in rustdoc.toml .rustdoc.toml doc.toml .config/rustdoc.toml .cargo/rustdoc.toml; do rm -f "${REPO:?}/${g:?}"; done; (cd $REPO && git checkout -q -- Cargo.toml member/Cargo.toml 2>/dev/null); src ''; }
(cd $REPO && git add -A >/dev/null 2>&1 && git -c user.name=p -c user.email=p@example.invalid commit -q -m base)
go() { echo "\$ $*" | strip; (cd $REPO && touch member/src/lib.rs && "$@" > $S/out/11.raw 2>&1; local rc=$?; strip < $S/out/11.raw | grep -E '^(error|warning)|PWNED|Documenting|Generated' | grep -v 'could not document' | sort -u | head -5 | sed 's/^/    /'; echo "    -> exit $rc"); }
DOC=(env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked)
echo; echo "## A. baseline: a broken intra-doc link, RUSTDOCFLAGS=-Dwarnings in the environment"; clean; go $DOC
echo "# without the variable"; go cargo doc --workspace --no-deps --locked
echo; echo "## B. candidate config files (none is expected to exist for rustdoc): each holds lint-silencing text"
for g in rustdoc.toml .rustdoc.toml doc.toml .config/rustdoc.toml .cargo/rustdoc.toml; do clean; mkdir -p $REPO/${g:h}; printf 'cap-lints = "allow"\n[lints]\nbroken_intra_doc_links = "allow"\n' > $REPO/$g; echo "# only repo/$g"; go $DOC; done
echo; echo "## C. inputs that change the verdict"
clean; mkdir -p $REPO/.cargo; printf '[build]\nrustdocflags = ["--cap-lints", "allow"]\n' > $REPO/.cargo/config.toml
echo "# C1 repo/.cargo/config.toml [build] rustdocflags = [\"--cap-lints\",\"allow\"], RUSTDOCFLAGS=-Dwarnings in the environment"; go $DOC
echo "# C1b the same file, no variable in the environment (the gate is then expected in the file itself)"; printf '[build]\nrustdocflags = ["-Dwarnings"]\n' > $REPO/.cargo/config.toml; go cargo doc --workspace --no-deps --locked
echo "# C1c [build] rustdocflags = [\"-Dwarnings\"] in repo/.cargo/config.toml and [build] rustdocflags = [\"--cap-lints\",\"allow\"] in member/.cargo/config.toml, cwd = member"; mkdir -p $MEMBER/.cargo; printf '[build]\nrustdocflags = ["--cap-lints", "allow"]\n' > $MEMBER/.cargo/config.toml; echo "\$ (cd member && cargo doc --no-deps --locked)"; (cd $MEMBER && touch src/lib.rs && cargo doc --no-deps --locked >/dev/null 2>&1; echo "    -> exit $?")
clean; printf '\n[lints.rustdoc]\nbroken_intra_doc_links = "allow"\n' >> $MEMBER/Cargo.toml; echo "# C2 member/Cargo.toml [lints.rustdoc] broken_intra_doc_links = \"allow\""; go $DOC
clean; printf '\n[lib]\ndoc = false\n' >> $MEMBER/Cargo.toml; echo "# C3 member/Cargo.toml [lib] doc = false"; go $DOC
clean; printf '\n[package.metadata.docs.rs]\nrustdoc-args = ["--cap-lints", "allow"]\n' >> $MEMBER/Cargo.toml; echo "# C4 member/Cargo.toml [package.metadata.docs.rs] rustdoc-args"; go $DOC
clean; src $'#![allow(rustdoc::broken_intra_doc_links)]\n'; echo "# C5 source: #![allow(rustdoc::broken_intra_doc_links)]"; go $DOC
clean; mkdir -p $REPO/.cargo; printf '[build]\nrustdoc = "/usr/bin/true"\n' > $REPO/.cargo/config.toml; echo "# C6 repo/.cargo/config.toml [build] rustdoc = \"/usr/bin/true\""; go $DOC
echo "# C6b the same with RUSTDOC=rustdoc in the environment"; go env RUSTDOC=rustdoc $DOC
clean; mkdir -p $REPO/.cargo; printf '[env]\nRUSTDOCFLAGS = { value = "--cap-lints allow", force = true }\n' > $REPO/.cargo/config.toml; echo "# C7 repo/.cargo/config.toml [env] RUSTDOCFLAGS forced (does cargo itself take its flags from [env]?)"; go $DOC
clean; mkdir -p $REPO/.cargo; printf '[alias]\ndoc = "help"\n' > $REPO/.cargo/config.toml; echo "# C8 repo/.cargo/config.toml [alias] doc = \"help\" (doc is built in)"; go $DOC
echo; echo "## D. file-lookup trace: paths in the tree opened or probed, outside target/"
clean; export FTRACE_OUT=$S/out/11-doc.trace; rm -f "${FTRACE_OUT:?}"
(cd $REPO && touch member/src/lib.rs && RUSTDOCFLAGS=-Dwarnings ftrace cargo doc --workspace --no-deps --locked >/dev/null 2>&1)
grep -F "$OUTER" $FTRACE_OUT | grep -vE '/target/|/\.git/' | awk '{ $1=""; $2=""; print }' | strip | sort -u | sed 's/^/    /' | head -40
clean; rm -f "${S:?}/out/11.raw"
