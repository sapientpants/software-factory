# Probe P-40: an embedded repository layout, a moved file in a diff, a second export, and two names with one fold

Throwaway scripts and their output for issue #89, run on 2026-11-26 on the
operator's Mac (macOS 27.0.1, arm64, APFS that is not case-sensitive, git
2.56.1) and in a Debian 13.8 container on the same machine (arm64, overlayfs,
git 2.47.3, as an unprivileged user). The record is the section of P-40 in
`docs/research/probes.md`.

Each script takes one argument, a scratch directory, and works below a
directory of its own in it, which it removes first. Every git process gets a
cleared environment, a home in the scratch directory, no system configuration
and a global configuration that the script names.

- `bare.sh` makes fourteen directories inside the work tree of a repository,
  A1 to A12, A14 and A15, each with some of the files of a git directory under
  a name other than `.git`, and starts three git commands in each:
  `rev-parse --git-dir`, an alias and `status`. A13 is a repository of its own
  with those files at its root, and a copy of it without its `.git`. The `config` of each names a canary script as an alias
  and as the fsmonitor, and the output says for each command whether the
  canary ran. The script then commits and clones the directories, and repeats
  the commands with `safe.bareRepository = explicit` in the global
  configuration, on the command line, in the repository's own configuration
  and in a system configuration. Its last part runs what an operator does
  with repositories of their own under that setting.
- `renames.sh` makes a base commit and a candidate that moves four files and
  copies one, and prints the diff of the two trees from `git diff-tree` and
  from `git diff`: by default, with detection asked for, with `--no-renames`,
  and with `diff.renames` set in the repository.
- `export.sh` runs the two export commands of SPEC 9.4 into a new directory,
  writes to that directory as a verify command could, and runs the two
  commands again on it, without and with `-f`.
- `case.sh` builds seven trees with `git mktree`, each with two paths that the
  fold of SPEC VAL-5d takes for one, so that no file system has to hold both.
  For each it runs `fsck --strict`, `index-pack --strict` on a pack of it and
  the export commands, and lists what the export holds. It then clones a
  branch that holds the first tree. Its last part exports a tree that holds
  the entries of a repository layout in two directories, `fix` and `FIX`, and
  starts git in what the export made of them.

On the Mac, with the scratch directory on the Mac's own volume:

```sh
for s in bare renames export case; do ./$s.sh <scratch> >results/$s-macos.txt 2>&1; done
```

In the container, with this directory mounted read-only at `/probe` and the
scratch directory on the container's own file system:

```sh
docker run --rm -v "$PWD":/probe:ro debian:13 bash -c '
  apt-get update -qq && apt-get install -y -qq git
  useradd -m p
  su p -c "mkdir /tmp/s && cd /tmp && /probe/<script>.sh /tmp/s 2>&1"'
```

The kept output is that of the last run of each script: `renames.sh` and
`export.sh` at 08:46 UTC on the Mac and 08:47 in the container, `bare.sh` and
`case.sh` at 09:22 and 09:23. Earlier runs the same day are not kept. In them
`export.sh` had a `stat` call that GNU `stat` reads otherwise, and `case.sh`
printed a notice of `fsck` that differs between the two versions of git and
is now left out. A review of the record then found that it said more than
`bare.sh` had run, and the script was changed for it: the cases A13 to A15
are new, as are A7 and part C of `case.sh`; parts B, C and D of `bare.sh` now
run on every directory that git took for a repository, where they had run on
some; and part B prints the exit status of `git add` and of `git commit`,
where it had printed that of a `sed` behind them, and lists every entry of the
commit. In the cases that both runs hold, no outcome differed.

Nothing ran in the factory's sandbox, and nothing ran on the server.
