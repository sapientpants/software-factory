# Probe answers

The answers to the probes of SPEC 23.2, one section per probe, added when the probe is answered. Each section says when and how the answer was observed, what was observed, and what is still open.

## P-16: bubblewrap and `/dev/kvm` on GitHub-hosted runners

Answered 2026-11-19 for #18, by workflow runs on `sapientpants/sf-scratch`. The documentary research behind the question is [13-github-runner-sandboxing](https://github.com/sapientpants/software-factory/blob/b6b9e0da7d2c7733a85045d395a849ca19b38a98/docs/research/13-github-runner-sandboxing.md) (#10); "report 13" below.

**Answer.** Hosted Linux runners of both architectures run bubblewrap with user namespaces once a setup step lifts Ubuntu's AppArmor restriction. On the 24.04 images they also do so in a network namespace without egress, but not inside a container under Docker's default confinement. `/dev/kvm` is present on x86-64 runners, on Azure and on AWS, and absent on arm64 runners. No guest was booted.

### Method

Two workflows on the branch `probe/p-16` of the scratch repository, at commit [`1f934b9`](https://github.com/sapientpants/sf-scratch/tree/1f934b934c5d073f7953b596f3beeec04c25ffaf/.github/workflows). They use no actions and `permissions: {}`; each is one shell step that prints a result line per test. `probe-p16.yml` did not change after the branch's first commit, `50e15d3`.

| Workflow | Labels | Runs |
|---|---|---|
| `probe-p16.yml` | `ubuntu-24.04`, `ubuntu-26.04`, `ubuntu-24.04-arm`, `ubuntu-26.04-arm` | [44474409661](https://github.com/sapientpants/sf-scratch/actions/runs/44474409661) (attempts 1 and 2), [44474946848](https://github.com/sapientpants/sf-scratch/actions/runs/44474946848), [44598159862](https://github.com/sapientpants/sf-scratch/actions/runs/44598159862), [44600448388](https://github.com/sapientpants/sf-scratch/actions/runs/44600448388): 20 jobs |
| `probe-p16-offline.yml` | `ubuntu-24.04`, `ubuntu-24.04-arm` | [44598159752](https://github.com/sapientpants/sf-scratch/actions/runs/44598159752), before the by-address egress test was added, and [44600448415](https://github.com/sapientpants/sf-scratch/actions/runs/44600448415) (attempts 1 and 2): 6 jobs. A first run, [44474947402](https://github.com/sapientpants/sf-scratch/actions/runs/44474947402), had defects in the probe script and is used only where stated. |

Of the 10 x86-64 jobs of `probe-p16.yml`, 3 ran on AWS (`ubuntu-26.04` once, `ubuntu-24.04` twice; instance type `m8id.xlarge`) and 7 on Azure. All 10 arm64 jobs ran on Azure. Of the 6 jobs of `probe-p16-offline.yml`, one x86-64 job ran on AWS and the rest on Azure. Every test below had the same outcome in every job that ran it. Actions logs expire, so the results are copied here.

### The images

| | `ubuntu-24.04`, `ubuntu-24.04-arm` | `ubuntu-26.04`, `ubuntu-26.04-arm` |
|---|---|---|
| OS | Ubuntu 24.04.5 LTS | Ubuntu 26.04.1 LTS |
| Kernels seen | `7.0.0-1017-azure`, `7.0.0-1014-aws` | `7.0.0-1019-azure`, `7.0.0-1020-azure`, `7.0.0-1016-aws` |
| `bubblewrap` | Not installed; `apt-get install` gives 0.9.0 | Installed, 0.11.1 |
| AppArmor profile for `bwrap` | None | `/etc/apparmor.d/bwrap-userns-restrict`, loaded as `bwrap` and `unpriv_bwrap` |
| `kernel.apparmor_restrict_unprivileged_userns` | 1 | 1 |

### bubblewrap

The tests, each as the runner's unprivileged user:

- **basic:** `bwrap --unshare-all --die-with-parent --ro-bind / / --proc /proc --dev /dev --tmpfs /tmp -- true`
- **caps:** the same with `--uid 0 --gid 0 --cap-add ALL`, running `mount -t tmpfs tmpfs /mnt` inside
- **nested:** basic, running a second `bwrap --unshare-all` inside
- **copy:** basic, with a copy of the `bwrap` binary at another path
- **unshare:** `unshare --user --map-root-user --mount -- mount -t tmpfs tmpfs /mnt`, without bubblewrap

| Image | State | basic | caps | nested | copy | unshare |
|---|---|---|---|---|---|---|
| 24.04 | As provisioned | fail | fail | fail | fail | fail |
| 24.04 | sysctl set to 0 | ok | ok | ok | ok | ok |
| 26.04 | As provisioned | ok | fail | fail | fail | fail |
| 26.04 | sysctl set to 0 | ok | fail | fail | ok | ok |
| 26.04 | Profile unloaded, sysctl 1 | fail | fail | fail | fail | fail |
| 26.04 | Profile unloaded, sysctl set to 0 | ok | ok | ok | ok | ok |

On 24.04 the failure is `bwrap: loopback: Failed RTM_NEWADDR: Operation not permitted`, with `apparmor="DENIED" operation="capable" profile="unprivileged_userns"` in the kernel log. On 26.04 with the profile loaded, AppArmor denies capabilities to a process inside the sandbox, whatever the sysctl says; the kernel log shows `profile="unpriv_bwrap"` denying `sys_admin` and `dac_override`. So on 26.04 an escape test could pass because of AppArmor and not because of the factory's sandbox policy, unless the profile is unloaded.

The setup that made every test pass:

```yaml
# ubuntu-24.04, ubuntu-24.04-arm
- run: sudo apt-get update && sudo apt-get install -y bubblewrap
- run: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
```

```yaml
# ubuntu-26.04, ubuntu-26.04-arm
- run: |
    sudo apparmor_parser -R /etc/apparmor.d/bwrap-userns-restrict
    sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
```

### bubblewrap without network access

VAL-6 asks CI to build and test without network access, in a container. Observed on `ubuntu-24.04` and `ubuntu-24.04-arm` (Docker 28.0.4), after the setup above. **loopback** binds and connects on `127.0.0.1`. **egress** is `curl https://github.com` and, in the four later jobs, also `curl https://1.1.1.1`; both succeeded on the runner itself. "none" means the first failed to resolve the name and the second failed to connect.

| Where the tests ran | basic | nested | loopback | egress |
|---|---|---|---|---|
| A network namespace on the runner holding only `lo` (`ip netns`), as the runner's user | ok | ok | ok | none |
| `docker run --network none`, default confinement | fail | fail | ok | none |
| … with `--security-opt seccomp=unconfined` | fail | fail | ok | none |
| … with `--security-opt apparmor=unconfined` | fail | fail | ok | none |
| … with both | fail | fail | ok | none |
| … with both and `--security-opt systempaths=unconfined` | ok | ok | ok | none |
| … with `--privileged` | ok | ok | ok | none |

The container was `ubuntu:24.04` with `bubblewrap` installed, running as an unprivileged user. Under default confinement the failure is `bwrap: No permissions to create new namespace`; with seccomp and AppArmor both lifted it is `bwrap: Can't mount proc on /newroot/proc: Operation not permitted`. Of the option sets tried, bubblewrap ran only in the two that remove the seccomp filter, the AppArmor profile and the masked system paths together. The arm64 job of the first run gave the same container results.

The network-namespace row was observed in all 6 jobs, the AWS-hosted one included. The command that entered the namespace as the runner's user:

```sh
sudo ip netns add offline
sudo ip netns exec offline ip link set lo up
sudo ip netns exec offline setpriv --reuid="$(id -u)" --regid="$(id -g)" --init-groups -- <command>
```

In the first run, `sudo -u "$USER"` in place of `setpriv` printed `sudo: unable to resolve host` on an AWS-hosted job, where the host name does not resolve without DNS.

A test that expects an internet host to be unreachable passes in such a namespace whatever the sandbox does (DEV-6).

### `/dev/kvm`

| Runners | Observed |
|---|---|
| x86-64, both images, Azure (7 jobs) and AWS (3 jobs) | `/dev/kvm` exists as `crw-rw---- root kvm`, and the runner's user cannot open it. After the udev rule from GitHub's changelog (report 13, section 3) it is `crw-rw-rw-`, and `KVM_GET_API_VERSION` and `KVM_CREATE_VM` succeed. No guest was booted. |
| arm64, both images, Azure (10 jobs) | No `/dev/kvm`. |

### Against report 13

- Its inference about 26.04 (unload the profile, then set the sysctl) is confirmed, as is the single sysctl on 24.04.
- `bubblewrap` is preinstalled on the 26.04 images and not on the 24.04 images; the report found no statement either way.
- `/dev/kvm` exists on AWS-hosted x86-64 runners; no source said so.
- `ubuntu-24.04-arm` has no `/dev/kvm`, as the maintainer said and against the user report in finding 30.

### Still open

- Whether a Firecracker or Cloud Hypervisor guest boots on a hosted runner. Not tried. GitHub does not support nested virtual machines there (report 13, finding 21), so the design does not depend on it (DEV-7).
- The network namespace and the container on the 26.04 images. Not tried; the workflow pins 24.04 (DEV-5).
- The factory's own sandbox, with its seccomp filter, in place of these stand-in commands, and a whole `cargo xtask check` inside the network namespace. The first CI run is that observation.

## P-7: capture from a hostile work tree

Answered 2026-11-19 for #6, by a prototype of the git commands of SPEC 9.4 run against a hostile-tree suite. The prototype and the output of its three runs are on the branch `prototype/capture-hostile-tree`, at commit [`97c07f3`](https://github.com/sapientpants/software-factory/tree/97c07f35141201fd02ae4edddfba315ffc08ddd5/crates/factory-git/prototype/capture). It is throwaway code and not the implementation.

**Answer.** The design of 9.4 holds. Scan, sandboxed staging into a quarantine, host-side ingest, `commit-tree` and `checkout-index` gave the right candidate or the right rejection in every case, and nothing from a workspace executed. The ignore rules are the `.gitignore` files of the workspace as they stand at capture, read by `git ls-files --others --exclude-standard`, and no other source. Seven points of 9.4 had to be made exact or changed; they are listed under "What changed in the spec".

### Method

`capture_proto.py` builds a mirror that also holds an unmerged branch of another run. For each case it makes a `host.git` (WS-10) and a workspace (WS-1), makes the workspace hostile in one way, and runs capture as four layers:

1. the scan, by `lstat` only;
2. the listing of the change set by the staging git, checked by the host against the scan's inventory;
3. staging, the tree and the pack;
4. ingest into `host.git`.

It carries on past the first rejection, so the verdict of every layer on every case is seen. Hostile git configuration and hooks name a canary script that leaves a file behind when it runs.

| Run | git | The staging git ran |
|---|---|---|
| macOS 27.0.1, APFS (not case-sensitive) | 2.56.1 | without a sandbox |
| Debian 13.7 in Docker on arm64, as an unprivileged user | 2.47.3 | without a sandbox |
| The same | 2.47.3 | under bubblewrap 0.12.0 with `--unshare-all`: the workspace read-only, `quarantine/` writable except for its `config`, `host.git/objects` read-only, and nothing else of the run directory |

The bubblewrap arguments were written by hand for the prototype, which started one instance per git command. It is not the factory's sandbox and has no seccomp filter. The three runs agree except where a difference is stated. In the bubblewrap run the canary script is outside the sandbox and could not have run, so "nothing executed" rests on the two runs without a sandbox. Four single observations cited below are not in those runs; `extra_checks.sh` shows them, on git 2.56.1, 2.47.3 and 2.39.5 (Debian 12).

### The commands

Every git process gets an environment of exactly these variables: `PATH`, `HOME=/nonexistent`, `LC_ALL=C`, `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_ATTR_NOSYSTEM=1`, `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0`, `GIT_NO_REPLACE_OBJECTS=1`, `GIT_NO_LAZY_FETCH=1`. Every command also carries `--attr-source=4b825dc642cb6eb9a060e54bf8d69288fbee4904`, the empty tree. Below, `git` stands for git with both.

```sh
# Host, before the sandbox starts. quarantine/ is a scratch repository.
git init -q --bare --template= quarantine
#   write quarantine/config (below)
#   write quarantine/objects/info/alternates: one line, the path of host.git/objects

# Sandbox. S is git --git-dir=quarantine --work-tree=workspace, with GIT_INDEX_FILE=quarantine/index
S read-tree <previous tree>
S update-index -q --refresh
S diff-files -z --raw --no-renames --ignore-submodules=all   # tracked paths: M, D or T
S ls-files -z --others --exclude-standard                    # untracked paths, ignore rules applied
#   host: check both lists against the scan's inventory, and stop here on a rejection
S update-index -z --force-remove --stdin                     # stdin: the D paths
S update-index -z --add --stdin                              # stdin: every other listed path
S diff-files ...; S ls-files ...                             # the two listings again: both must be empty now
S write-tree                                                 # stdout: the tree name
S pack-objects --revs --stdout --local -q                    # stdin: the tree name; stdout: a pack

# Host. ingest/ is a new, empty object directory (incoming/ in the prototype). H is git --git-dir=host.git,
# I is H with GIT_OBJECT_DIRECTORY=ingest GIT_ALTERNATE_OBJECT_DIRECTORIES=host.git/objects
I index-pack --stdin --strict --max-input-size=<cap>         # stdin: the pack
I cat-file -t <tree>                                         # must print "tree"
I ls-tree -r -t -z -l --full-tree <tree>                     # modes, sizes and names
I diff-tree -r -z --raw --no-renames --no-abbrev <previous tree> <tree>
#   host: move ingest/pack/* into host.git/objects/pack/, the .idx file last; delete ingest/
H commit-tree --no-gpg-sign <tree> -p <parent> -F -          # identity and dates in GIT_AUTHOR_* and GIT_COMMITTER_*

# Export, into a new and empty export/<sha>
GIT_INDEX_FILE=<temporary index> H read-tree <candidate>
GIT_INDEX_FILE=<temporary index> H --work-tree=export/<sha> checkout-index -a
```

`quarantine/config`, the only configuration file the staging git reads, and read-only to it (SEC-6):

```ini
[core]
	repositoryformatversion = 0
	bare = false
	fileMode = true
	symlinks = true
	ignoreCase = false
	precomposeUnicode = false
	protectHFS = true
	protectNTFS = true
	autocrlf = false
	safecrlf = false
	fsmonitor = false
	untrackedCache = false
	hooksPath = /dev/null
	attributesFile = /dev/null
	excludesFile = /dev/null
	logAllRefUpdates = false
[gc]
	auto = 0
[maintenance]
	auto = false
[submodule]
	recurse = false
```

### What each layer did

"Stops it" is the first layer that rejected. The last column is what the later layers did when the prototype carried on regardless.

| Case | Stops it | Later layers |
|---|---|---|
| Edits, a new file, a deletion, a new symlink, a file that became a directory and a directory that became a file | Nothing: candidate | Every path of the tree has in the export the bytes and the executable bit it has in the workspace. An empty directory is not in the tree. |
| Executable bits | Nothing: candidate | File modes `0755` and `4755` give `100755`; `0644` and `0666` give `100644`. A change of mode alone is listed as a modification. |
| A file the staging user cannot read | Staging | `update-index --add`: `open("src/locked.rs"): Permission denied`, exit 128 |
| The root `.git` made hostile: `core.fsmonitor`, `core.hooksPath`, `core.pager`, a filter driver, `diff.external`, aliases, hooks, `info/exclude` holding `*`, `info/attributes`, a FIFO | Nothing: candidate | None of it is read. A new file is captured although `info/exclude` ignores everything. |
| The root `.git` replaced by a gitfile | Nothing: candidate | Not read |
| A nested repository: a `.git` directory with or without commits, a gitfile, a `.git` symlink | Scan | Git lists the directory as one path, `vendor/nested/`, which the listing check rejects as not a file. `update-index --add` prints `Ignoring path vendor/nested/` and exits 0. |
| A `.git` file that names no repository; under bubblewrap also the gitfile and the symlink, whose targets are outside the sandbox | Scan | Git descends into the directory, stages the other files and leaves `.git` out without a message. No later layer notices. |
| A nested repository in an ignored directory | Scan | Git lists nothing. No later layer notices. |
| A directory named `.GIT` (Linux only), `.git` with a trailing space, `git~1`, `.g<U+200C>it`; a file named `x\.git\config` | Scan | Git lists the files inside as ordinary untracked files. `update-index --add` prints `Ignoring path ...` for each and exits 0. |
| A directory that holds `HEAD`, `config`, `objects/` and `refs/` but is not named `.git` | Nothing: candidate | Its files are staged like any others (see "Still open"). |
| Symlinks to `/etc`, to a directory outside the workspace, to a directory inside it, and to nothing; a tracked directory replaced by a symlink to a directory that holds a file of the same name | Nothing: candidate | Each symlink is one `120000` entry. The tracked files that were behind the replaced directory are deletions. No content from behind a symlink entered `host.git`. |
| An untracked FIFO or Unix socket, at the top level or in an ignored directory | Scan | Git lists nothing. No later layer notices. |
| A tracked file replaced by a FIFO | Scan | Listed as modified; the listing check rejects it; `update-index --add`: `unsupported file type`, exit 128 |
| Hard links, between two workspace paths and to a file outside the workspace | Nothing: candidate | Two entries with one blob. The export holds separate files. The content of the outside file is captured. |
| `.gitattributes` with a filter whose driver is defined in the workspace's `.git/config`, `eol`, `working-tree-encoding`, `ident`, `export-ignore`, `-diff` and the Git LFS line | Nothing: candidate | See "Attributes" below |
| An agent-edited `.gitignore` | Nothing: candidate | See "Ignore rules" below |
| `.gitignore` replaced by a symlink to a file outside that ignores everything | Nothing: candidate | Not followed: `warning: unable to access '.gitignore': Too many levels of symbolic links`. The symlink is staged as a type change. |
| A 6 MiB untracked file | Listing check | Staged anyway, ingest rejects the blob by its size |
| A 6 MiB file in an ignored directory | Nothing: candidate | Not listed |
| A sparse file of 1 TiB apparent size | Listing check | Rejected by its `lstat` size. Nothing read it. |
| 11 new files of 4.9 MiB | Listing check | Staged anyway, `index-pack`: `pack exceeds maximum allowed size` |
| Two names that differ only in case, in one directory (Linux only) | Nothing in 9.4 | Git stages both, and the tree holds both. The prototype's ingest carried a check of its own that rejects them; the spec has no such rule (see "Still open"). |
| Names with a newline, a tab, a quote, a leading space, a leading dash, `--stdin`, bytes that are not UTF-8 (Linux only), 255 bytes | Nothing: candidate | All captured and exported |

### Findings

**The scan is the only layer that sees what git leaves out.** Git lists neither an untracked FIFO or socket, nor an entry named `.git` that it does not take for a repository, nor anything in an ignored directory. A capture without the scan builds a candidate that lacks them and reports nothing. The scan must also come first: with `.gitignore` replaced by a FIFO and the scan skipped, `ls-files` blocked until the prototype's timeout killed it.

**`update-index` exits 0 on a path it refuses.** For a nested repository and for every name equivalent to `.git` it prints `Ignoring path ...` on standard error and carries on, so its exit status says nothing about whether the listed paths were staged. The prototype runs the two listing commands again after staging; anything they still list was not staged. `git add -A` is no substitute: on a nested repository it prints `warning: adding embedded git repository` and stages a gitlink.

**Deletions need `--force-remove`, and go first.** When a tracked directory is replaced by a symlink, `update-index --remove` refuses the files that were in it (`'src/lib.rs' is beyond a symbolic link`, exit 128). `--force-remove` for the deleted paths, in a first invocation, handles that case and the file-to-directory and directory-to-file swaps (`extra_checks.sh`, check 2).

**Nothing from a workspace executed, with one exception that was constructed on purpose.** The canary stayed untouched in every case above. The exception: a base tree with a gitlink, and at that path a nested repository whose `HEAD` is the commit the gitlink names. `diff-files --ignore-submodules=none` then starts `git status` inside the nested repository, and that process ran the nested repository's `core.fsmonitor` command (`extra_checks.sh`, check 4, where that setting is the only hostile one). With `--ignore-submodules=all` nothing ran. The scan rejects such a workspace, and claim refuses a base tree with a submodule entry (8.2).

**Ignore rules.** In the staging git the only source is the `.gitignore` files in the workspace. `quarantine/info/exclude` does not exist, `core.excludesFile` is `/dev/null`, and the workspace's own `.git/info/exclude` is never read. With `.gitignore` edited to drop `target/`, add `hidden/` and add `!build/shown.log`:

| Path | In the candidate |
|---|---|
| `.gitignore` itself | Yes, as a modification |
| `hidden/tool.rs`, newly ignored | No |
| `target/debug/app`, no longer ignored | Yes |
| `build/shown.log`, un-ignored by the `!` rule | Yes |
| `build/keep.log`, tracked at base, matches `*.log`, modified | Yes, with the modification |
| `vendor/self/blob.rs` and `vendor/self/.gitignore`, where that file holds `*` | No, neither |

So an agent's edit to an ignore file takes effect in the same capture. A root `.gitignore` edit is itself in the candidate, where 11.3 flags it. A new `.gitignore` that ignores its own directory leaves no trace in the candidate. What it hides stays in the workspace and reaches no gate. An ignore rule never removes or freezes a tracked path.

**Attributes.** With the empty attribute source, staging stores the workspace's bytes and the export writes the blob's bytes, so in every case each path of the candidate had in the export the bytes it had in the workspace. The filter driver was not run and the Git LFS pointer file was staged as the text it is. With in-tree attributes instead:

| Staging | Export | Files of the export that differ from the workspace |
|---|---|---|
| Empty tree | Empty tree | None |
| In-tree | Empty tree | `crlf.txt` (`eol=crlf`), `id.txt` (`ident`), `utf16.txt` (`working-tree-encoding=UTF-16`) |
| In-tree | In-tree | `id.txt`; on macOS also `utf16.txt` |

- The workspace checkout has to use the same source as staging. A base commit with `eol=crlf` and `working-tree-encoding` files, checked out with in-tree attributes and staged with the empty source, gave a change set of those two files in a workspace nobody had touched. With the empty source at both ends the change set was empty.
- `checkout-index` applies the candidate's own `.gitattributes` unless the source is overridden: a blob holding `one\ntwo\n` under `eol=crlf` was exported as `one\r\ntwo\r\n`. The sentence in 9.4 that the export applies no attributes was not true of the command as it was written there.
- The override has to be the `--attr-source` option. git 2.39.5 (Debian 12) ignores the variable `GIT_ATTR_SOURCE` without a message and applies the in-tree attributes; given the option it exits with `unknown option` (`extra_checks.sh`, check 1).
- The export keeps a file marked `export-ignore`; `git archive` of the same commit omits it.
- A `.rs` file under `*.rs -diff` shows as `Binary files differ` when the candidate's attributes apply, and as a text diff under the empty source (VAL-5a).

**Limits.** The listing check needs no access to the workspace: the scan recorded each path's type and size. The sizes at ingest come from the blobs (`ls-tree -l`), so they hold even if the listing lied. `--max-input-size` bounds what `index-pack` will read.

**A subverted staging git.** The host takes two things from the sandbox, a tree name and a pack on standard output, and opens nothing in `quarantine/`. Hostile values were fed to ingest directly:

| Returned by "staging" | Ingest |
|---|---|
| A tree with an entry named `.git`, `.GIT` or `git~1` | `index-pack --strict`: `hasDotgit: contains '.git'` |
| A tree with a `.gitmodules` symlink | `index-pack --strict`: `gitmodulesSymlink` |
| A tree with a gitlink | Passes `index-pack`; the mode walk rejects `160000` |
| A tree that names a blob absent from the pack and from `host.git` | `index-pack --strict`: `did not receive expected object` |
| A tree that names the blob of another run's branch | The same: the blob is in the mirror but not in `host.git` |
| A truncated pack; ten bytes that are not a pack | `index-pack`: `early EOF` |
| A tree name that is not 40 hex digits | Rejected before any git command |

After these ten rejections `host.git/objects/pack` held the clone's pack and nothing else, because the pack is indexed into `ingest/` and moves only when every check has passed.

**Another run's objects.** The blob of the other run's branch is in the mirror, and in neither a new workspace nor `host.git`.

**The commit.** `commit-tree` with the same tree, parent, message, identity and dates gave the same commit twice. A capture of an untouched workspace gave the previous tree and an empty pack, which `index-pack` accepts.

**`ls-tree --format` quotes paths under `-z`.** `git ls-tree -z --format='...%(path)'` printed `"tab\tname"`, quoted, on 2.39.5, 2.47.3 and 2.56.1 (`extra_checks.sh`, check 3). `git ls-tree -z -l` prints the raw bytes. This matters for WS-8, which relies on NUL-delimited output being unquoted.

**Case.** On Linux two names that differ only in case are staged and committed without complaint. On macOS neither they nor a `.GIT` directory can be created next to their counterpart, so those cases need a case-sensitive filesystem. A tree made by hand with a symlink `A` and a directory `a`, exported on macOS: `checkout-index` failed with `cannot create directory at 'a': File exists`, and nothing was written through the symlink.

### What changed in the spec

1. `quarantine/` is a repository with a host-written configuration, not only an object directory. What the staging git may read and write stays as SEC-6 had it (9.1, 9.4 step 2).
2. The scan records the type and size of every entry, and step 1 says why it runs first. The listing is `diff-files` and `ls-files --others --exclude-standard`; the ignore sources are stated; the host checks the listing against the scan's inventory (9.4 steps 1 and 2).
3. Staging removes with `--force-remove` first, and lists again afterwards (9.4 step 2).
4. The sandbox hands back a tree name and a pack. Ingest indexes the pack into `ingest/` with `index-pack --strict` and moves it into `host.git` after the tree walk (9.1, 9.4 step 3).
5. Ingest recomputes the size limits from the blobs (9.4 step 3).
6. No in-tree attribute is applied by the workspace checkout, by staging or by the export, through `--attr-source` (WS-11, cited from WS-1, 9.4 and the threat table of 13.1).
7. WS-9 names the cases this probe added.

### Still open

- The factory's own sandbox in place of the hand-written bubblewrap arguments, and one instance for the whole step in place of one per git command.
- Case collisions. WS-9 lists them in the suite, and 9.4 does not say what capture does with two names that differ only in case; git accepts them. A rule at ingest would walk the whole tree, so a base tree that already holds a collision would fail every capture unless claim (8.2) or onboarding (20.1) screened for it, as they do for submodule entries. Not decided here.
- git 2.42 to 2.46. The floor in 4.4 is 2.42 and `--attr-source` needs 2.40, but nothing older than 2.47.3 ran the suite.
- The scan in Rust. The prototype's test for names equivalent to `.git` is an approximation of git's `is_hfs_dotgit` and `is_ntfs_dotgit`; the implementation has to match them.
- A directory with the layout of a bare repository, under a name other than `.git`, passes capture. Git started inside that directory of a checkout would use its `config` unless `safe.bareRepository` is `explicit`. Whether 11.3 should flag it is not decided here.
- Cost. A new index holds no file times, so every capture reads every tracked file. That, and the 300 s limit, were not measured on a real repository.
- A repository that uses `eol`, `ident` or `working-tree-encoding`. Under WS-11 its workspace holds the blobs' bytes, and the agent's own git, which reads the in-tree attributes, may report such files as modified. Not tried. The "clean round-trip checkout" that 20.1 asks for at onboarding does not say which attribute source it means; under WS-11 it holds for every repository.
