# Probe P-39: where rustup puts a toolchain, and whether its directory works alone

Throwaway scripts and their output for issue #88, run on 2026-11-26 on the
operator's Mac (macOS 27.0.1, arm64) with rustup 1.29.3. The record is the
section of P-39 in `docs/research/probes.md`.

- `install.sh` installs three toolchains into a rustup home of its own, below
  an empty scratch directory, and lists what rustup made: the directories, the
  file `lib/rustlib/components` as components and a target come and go, and the
  names that the channel manifest renames. Its last case runs
  `rustup --version` in a directory that holds a `rust-toolchain.toml`, with a
  second, empty rustup home. Its output is `results/install.txt`.
- `use.sh` builds, tests, lints and formats the crate of `case/` with the `bin`
  of such a toolchain first on a cleared `PATH`, with no rustup: in place,
  from a copy under another path, from that copy without write permission and
  through a symbolic link. Its output is `results/use.txt`.
- `case/` is that crate. Its `rust-toolchain.toml` names another version than
  the toolchain that builds it.

Both scripts ran once, in this order, on one scratch directory:

```sh
./install.sh ~/.cargo/bin/rustup <scratch>
./use.sh <scratch>
```

Nothing ran on Linux, and nothing ran in a sandbox.
