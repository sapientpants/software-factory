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

- `bare.sh` makes thirteen directories inside the work tree of a repository,
  each with some of the files of a git directory under a name other than
  `.git`, and starts three git commands in each: `rev-parse --git-dir`, an
  alias and `status`. The `config` of each names a canary script as an alias
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
- `case.sh` builds six trees with `git mktree`, each with two names of one
  directory that the fold of SPEC VAL-5d takes for one, so that no file system
  has to hold both. For each it runs `fsck --strict`, `index-pack --strict` on
  a pack of it and the export commands, and lists what the export holds. It
  then clones a branch that holds the first tree.

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

The kept output is that of the last run of each script. Earlier runs the same
day had `bare.sh` without its case A13 and with two cases of its parts D and E
printed in another form, `export.sh` with a `stat` call that GNU `stat` reads
otherwise, and `case.sh` with a notice of `fsck` that differs between the two
versions of git and is now left out. No outcome differed between those runs
and the kept ones.

Nothing ran in the factory's sandbox, and nothing ran on the server.
