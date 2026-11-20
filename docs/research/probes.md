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

## P-21: what a non-bare `claude -p` loads from the working directory

Answered 2026-11-19 for #11, by live sessions of Claude Code 2.1.311 on macOS 27.0.1, signed in with a `claude setup-token` token of the operator's Max plan. The harness and the output of the pass this record is written from are on the branch `prototype/claude-code-lockdown`, at commit [`2d47e6d`](https://github.com/sapientpants/software-factory/tree/2d47e6dd88e92a5ce02aac2aada3dc4b4e521214/crates/factory-agent/prototype/lockdown). It is throwaway code and not the contract test of SPEC 19.4. The documentary research behind the question is [11-claude-code-non-bare-isolation](https://github.com/sapientpants/software-factory/blob/65ee30b3a60d249077f165da49be86fd6a4d5a20/docs/research/11-claude-code-non-bare-isolation.md) (#8); "report 11" below.

**Answer.** None of them. With `--setting-sources user`, nothing that was planted in the working directory took effect: no instruction file, skill, command, subagent, hook, `env` block, permission rule, MCP server or plugin, whether it was there at the start, sat in a subdirectory or was written while the session ran. AG-3a holds as written. Its fallback, removing files from the reviewer's export, becomes a second layer, kept until the contract test has passed on the server, because every session here ran on macOS and outside the sandbox. The host's own configuration kept working under the same flags. One thing the spec did not say: the per-execution config directory is live configuration, and a hook or skill written into it mid-session takes effect in that session.

### Method

`lockdown_probe.py` runs one session per case, 38 in all. Each case gets:

- a fresh working directory, a git repository with one commit, with the planted files of the case;
- its own `HOME` and `CLAUDE_CONFIG_DIR`, holding a user-level `settings.json` with a hook, a `CLAUDE.md` and a skill;
- a host-written `--settings` file with a hook and a host-written `--mcp-config` file, both outside the working directory;
- an environment of `PATH`, `HOME`, `CLAUDE_CONFIG_DIR`, `CLAUDE_CODE_OAUTH_TOKEN`, `TMPDIR`, `LANG`, the proxy variables and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, and nothing else.

The command line of every case is `claude -p --output-format stream-json --verbose --model haiku --session-id <uuid> --max-turns <n> --debug-file <file>` plus the flags of the case. "AG-3a" below is `--setting-sources user --mcp-config <host file> --strict-mcp-config --settings <host file>`. Every lock-down case has a control: the same working directory without the lock-down flags.

Three kinds of evidence, in order of strength:

1. Every planted hook and MCP server writes a canary file when it runs, holding its `SF_PROBE_*` environment. The planted `env` blocks set such variables.
2. Skills, commands, subagents, plugins and MCP servers are read from the lists of the `system/init` message. For anything found after the start they are read from the next `init`, which a session with stream-JSON input emits at every turn, and from `system/commands_changed`.
3. Each instruction file holds its own canary word, and the prompt asks the model to list the words it can see. This is the one part that depends on the model. In every control the model listed each planted word, so "not in context" below is not a missed answer.

Selected lines of the CLI's debug log are kept in each case's summary and are quoted where they explain a result.

### What took effect

| Planted in the working directory | No lock-down | AG-3a |
|---|---|---|
| `CLAUDE.md`, `.claude/CLAUDE.md`, `CLAUDE.local.md`, `.claude/rules/*.md` | In context | Not in context |
| `AGENTS.md`, in a tree with no `CLAUDE.md` and no `CLAUDE.local.md` | In context | Not in context |
| A skill, a command and a subagent under `.claude/` | Listed in `init` | Not listed |
| Hooks of `.claude/settings.json` (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `Stop`) and of `.claude/settings.local.json` | Ran | Did not run |
| The `env` block of both settings files | In the environment of hooks and MCP servers | Absent from it |
| `permissions.allow` in `.claude/settings.local.json` | Applied: the Bash command it names ran | Not applied: the command was denied |
| `permissions.allow` in `.claude/settings.json` | Dropped: "workspace not yet trusted" | Not applied, which says nothing |
| The server of `.mcp.json` | Started and connected | Not started |
| In a subdirectory: a `CLAUDE.md`, a skill under `.claude/skills/`, and a rule with `paths:` for it. The session reads a file there | All three loaded; the skill arrived in `system/commands_changed` | None loaded; the debug log says "Dynamic skill discovery skipped: projectSettings disabled or plugin-only policy" |
| Hooks written into `.claude/settings.json` and `.claude/settings.local.json` between two turns | Ran at the next turn | The CLI logged "Detected change" for both files; no hook ran |
| A skill written into an existing `.claude/skills/` between two turns | Listed by the CLI in `system/commands_changed` and the next `init` | Not listed |
| `.claude/skills`, `commands` and `agents` of the main checkout, with the session in a linked worktree that has no `.claude/` | Loaded | Not loaded |
| A plugin folder under `.claude/skills/` (a skill and a hook), with a trust record for the directory in `.claude.json` | Loaded | Not loaded |

Four plants did nothing in their control either, so they say nothing about the lock-down: the `permissions.allow` rule of `.claude/settings.json`, which a directory without a trust record does not get; the plugin folder without a trust record; and a `CLAUDE.md` and a `.mcp.json` written between two turns. The skill written between two turns rests on the CLI's lists alone: asked for it by name, the model named no such skill, with or without the lock-down.

Under AG-3a the host's side worked in every case that exercised it: the user-level skill and hook of the config directory, the hook of the `--settings` file and the server of the `--mcp-config` file in all of them, and the user-level `CLAUDE.md` wherever the prompt asked for canary words. A session with every flag that AG-1 then listed except `--bare`, with stream-JSON input, returned its `structured_output` and had the text of `--append-system-prompt-file` in context. With `--tools Read,Glob,Grep` its tools were those three, `StructuredOutput` and the tool of the host's MCP server.

### Each switch on its own

| Switch | Removed | Left in place |
|---|---|---|
| `--setting-sources user` | Every row of the table above that was run with it alone: instruction files, skill, command, subagent, hooks, `env` blocks and the `.mcp.json` server | The user level |
| `--strict-mcp-config` | The `.mcp.json` server | Everything else |
| `--disable-slash-commands` | Every skill and command: the project's, the user's and the 17 the CLI bundles | Subagents, instruction files, hooks, MCP servers |
| `"disableAllHooks": true` in `--settings` | Project, local and user hooks, and the hooks of the `--settings` file itself | Everything else |
| `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1` | Every instruction file, `AGENTS.md` and the user-level `CLAUDE.md` included | Skills, commands, subagents, hooks, MCP servers |
| `pluginConfigs` > `cc-plugin-agents-md@builtin` > `instructionFiles: "claude-md"` in `--settings` | `AGENTS.md` | `.claude/rules/` and everything else |
| `--safe-mode` | The repository's instruction files, skill, command, subagent and hooks, though the debug log shows the `permissions.allow` rule of `.claude/settings.local.json` still loaded. The host's items too: the `--mcp-config` server ("server ignored (safe mode)"), the `--settings` hook, the user-level files | The subscription login |
| `--restricted` | The repository's instruction files, skill, command, subagent, hooks and `env` blocks; the user level; and the tools `Bash`, `WebFetch`, `Monitor`, `CronCreate`, `RemoteTrigger` and `Workflow` | The `--settings` hook, the `--mcp-config` server, the subscription login |
| `--bare` | The login: "Not logged in · Please run /login", exit code 1, no request to any host | |

The `--safe-mode` and `--restricted` cases also passed `--mcp-config`, `--strict-mcp-config` and `--settings`, so they say nothing about `.mcp.json`. AG-3a with `--disable-slash-commands` added, and AG-3a with that flag, `disableAllHooks`, the `AGENTS.md` entry and `autoMemoryEnabled: false`, both ran and kept the `--mcp-config` server. There is no second switch for subagents or for plugins short of `--safe-mode` and `--restricted`.

### Findings

**The config directory is the `user` source, and it is live.** Under AG-3a, a `settings.json` with hooks and a new skill directory were written into `CLAUDE_CONFIG_DIR` between two turns. The hooks ran at the next turn, and the CLI announced the skill in `system/commands_changed`. The debug log names what the CLI watches there: the settings file and the skill directory. So what keeps a worker from configuring its own session is the read-only mount of AG-4, not a flag, and it has to cover more than the settings file. The effect stays inside one stage execution, since the directory is fresh for each.

**The CLI still watches the project's files under the flag.** With `--setting-sources user` the debug log lists `.claude/settings.json`, `.claude/settings.local.json` and the project's `.claude/skills`, `commands` and `agents` among the watched paths, and reports a change to them. Nothing of the change is applied.

**`AGENTS.md` is read only when no `CLAUDE.md` and no `CLAUDE.local.md` is in the tree.** With a `CLAUDE.local.md` next to it and no `CLAUDE.md`, the unlocked session had the local file in context and not `AGENTS.md`. Under AG-3a it is not read in either case, so a session gets the repository's instructions once, through AG-2a.

**Trust matters without the flag, in an unexpected direction.** In a directory with no trust record, an unlocked `-p` session runs the project's hooks, applies its `env` blocks and starts its `.mcp.json` server. It drops the `permissions.allow` rules of `.claude/settings.json` and applies those of `.claude/settings.local.json`.

**`--mcp-config` takes several values.** As the last option before the prompt it took the prompt for a second file and the CLI stopped with "MCP config file not found". That happened while the harness was built, and the failed run is not among the results. The driver sends its prompt on standard input, so this is a trap for a test, not for AG-1.

**What a non-bare session carries that the repository did not put there.** 23 built-in tools, among them `WebFetch`, `WebSearch`, `CronCreate`, `RemoteTrigger` and `Workflow`; 17 bundled skills; 5 built-in subagents; two built-in plugins. The tool allowlist of AG-1 removes the tools. The session also opens a Unix socket for injected messages (`messaging_socket_path` in `init`; not under `--restricted`) and names an auto-memory directory inside the config directory.

**Managed locations are read whatever the flags.** In every case the debug log names a managed skills directory, `/Library/Application Support/ClaudeCode/.claude/skills`. The full debug logs, which are not kept, also showed the CLI looking there for `managed-settings.json` and for managed agents and commands. That is the macOS path. Nothing was there, and the path on Linux was not observed.

**The token stays where it was put.** Its literal value was in no file that any session left behind: the config directory with its transcripts, the debug log, the stream.

### Against report 11

- Every "no" of its table is confirmed for 2.1.311, and the rows it rated medium (`CLAUDE.local.md`, skills, commands, subagents) with them.
- `AGENTS.md`, on which the documentation is silent, is not read under `--setting-sources user`.
- A plugin folder under `.claude/skills/` is stopped by the flag alone, with the trust record present.
- The read-through from a linked worktree to the main checkout respects the flag. SPEC 9.2 uses no linked worktrees in any case.
- The leak that 2.1.314 fixed, an excluded settings file "still supplying some settings, such as auto mode rules and saved permission consents", did not show for a `permissions.allow` rule in `.claude/settings.local.json`. Auto mode rules were not planted.
- `--safe-mode` works with `-p` and keeps the login, and it is unusable here: it ignores the `--mcp-config` servers and the hooks of `--settings`.

### What changed in the spec

1. AG-3a states what the flags kept out and that they apply to every role, so it no longer calls `subscription` mode a weakening of INV-10. The removal of files from the reviewer's export stays, as a second layer, until the contract test has passed on the server.
2. AG-4 makes read-only to the worker everything in the config directory that the CLI reads as configuration, not only the settings file, and the mount table of SEC-6 follows.
3. The contract test of 19.4 asserts the lock-down with the cases of this probe as its fixture. It asserted that a settings file written mid-session into the config directory has no effect; in this probe it had one, so the test now asserts that a worker cannot change that configuration.

### Still open

- Linux, and the factory's sandbox. Every session ran on macOS without one.
- Other CLI versions. The documentation still announces bare mode as the default for `-p`, and report 11 found no documented way to opt out. Report 11 also advised a floor of 2.1.314 for `subscription` mode, or accepting the leak that version fixed. Neither is done here: 2.1.314 was not run, and the floor belongs to the tested range of AG-5.
- Not planted: output styles, workflows, LSP configuration, a status line, hooks in the frontmatter of a skill or a subagent, auto mode rules.
- In the config directory only `settings.json` and `skills/` were written mid-session. That `CLAUDE.md`, `rules/`, `commands/` and `agents/` behave alike is an inference from the `user` source, and AG-4 lists them on that basis.
- AG-2's statement about bare mode and the `env` block of a project settings file. Bare mode cannot run on the subscription token, so it was not tried.

## P-22: Claude Code on a subscription token: caps, usage, plan limits and hosts

Answered 2026-11-19 for #11, by the sessions of P-21, whose Method section applies; all of them ran on a `claude setup-token` token passed as `CLAUDE_CODE_OAUTH_TOKEN`. The plan limit was not reached, on purpose. The documentary research is [12-claude-code-subscription-behaviour](https://github.com/sapientpants/software-factory/blob/616f64afcb57c3654ebea670f48534fdb4a26c3e/docs/research/12-claude-code-subscription-behaviour.md) (#9); "report 12" below.

**Answer.** Both caps work under the token. The final `result` holds tokens and API-equivalent dollars per model. A session emits one `rate_limit_event` after its first model response, with the plan window, its utilisation, a reset time in Unix seconds and the account's usage-credit state. What the CLI emits when a request is rejected at the limit was not observed. A session needs `api.anthropic.com` and no other host. SEC-11 holds as written and now names the host.

### Caps

| Case | Stream | Exit code |
|---|---|---|
| `--max-turns 1`, on a prompt that needs a tool call and then an answer | The tool call runs, then `result` with `subtype: "error_max_turns"`, `is_error: true`, `terminal_reason: "max_turns"`, `errors: ["Reached maximum number of turns (1)"]` | 1 |
| `--max-budget-usd 0.0001`, same prompt | After the first response, which cost $0.0011 by the CLI's count: `result` with `subtype: "error_max_budget_usd"`, `terminal_reason: "budget_exhausted"`, `errors: ["Reached maximum budget ($0.0001)"]` | 1 |

The budget cap is checked after a response, so it overshoots by that response, as report 12 read in the documentation. Both results carry `total_cost_usd` and `modelUsage`.

### Usage and cost in the stream

- **`result`.** `total_cost_usd`, `usage`, `modelUsage` (per model: `inputTokens`, `outputTokens`, `cacheReadInputTokens`, `cacheCreationInputTokens`, `thinkingTokens`, `costUSD`, and `costBasis: "list"`), `model_usage_since_last_result`, `num_turns`, `duration_ms`, `duration_api_ms`. All were filled in under the token. `init` reports `apiKeySource: "none"`.
- **Running totals.** In a session with stream-JSON input and two turns, the two results gave `total_cost_usd` 0.00097 and 0.00184, and `modelUsage` likewise: totals for the session. `usage` and `model_usage_since_last_result` gave each turn's own numbers.
- **`assistant` events.** Input and cache counts match the call. `output_tokens` does not: 16 and 8 for the two calls of a session whose result reports 134.
- **`message_delta`.** With `--include-partial-messages` the stream carries the API's events, and the `message_delta` that closes each call has the real count: 123 and 11 for the same two calls. It is the same with the whole command line of AG-1 and stream-JSON input: 50, 325 and 188 for three calls, and 563 in the result.

No event states how much plan quota a session used. The only plan figure is the utilisation below.

### The `rate_limit_event`

Every session that completed emitted exactly one, after its first model response, with `status: "allowed"`:

```json
{"type": "rate_limit_event",
 "rate_limit_info": {
   "status": "allowed", "resetsAt": 1795615200, "rateLimitType": "seven_day",
   "overageStatus": "rejected", "overageDisabledReason": "org_level_disabled", "isUsingOverage": false,
   "unifiedWindows": {"seven_day": {"utilization": 0.18, "resetsAt": 1795615200, "status": "allowed"}}}}
```

- `resetsAt` is in Unix seconds: the value is 2026-11-25 14:00 UTC.
- Only the seven-day window was reported. No `five_hour` entry appeared in any session.
- The event gives the usage-credit state of the account. Here credits are off (`overageStatus: "rejected"`), which is the condition under which the plan's limits bound the spend.
- A second turn of the same session emitted no further event. A session stopped by the budget cap at its first response emitted none.

### How a session ends

| Situation | Stream | Exit code |
|---|---|---|
| Completed | `result` with `subtype: "success"`, `is_error: false`, `terminal_reason: "completed"` | 0 |
| A cap | See Caps | 1 |
| No login (`--bare` with the token) | An `assistant` message, then `result` with `subtype: "success"`, `is_error: true`, `terminal_reason: "api_error"`, `result: "Not logged in · Please run /login"` | 1 |
| Model host unreachable | `init` after 18 s, then 10 `system/api_retry` events (`attempt`, `max_retries: 10`, `retry_delay_ms` from 586 to 39271, `error: "unknown"`, `error_status: null`) over 177 s, then `result` with `subtype: "success"`, `is_error: true`, `terminal_reason: "api_error"` and the error text | 1 |
| A tool call that needs approval, with `--permission-prompts none` | `system/permission_denied`, a tool result that says so, and an entry in `permission_denials` of the `result`; the session goes on | 0 |
| Stream-JSON input | After a `result` the process stays alive. It exited 0.1 s after standard input was closed | 0 |

So `subtype: "success"` does not mean success; `is_error` does. One session met a real network fault and shows the same `system/api_retry` events, four of them, before it carried on.

### Hosts

Each session ran behind a CONNECT proxy in the harness that logs the host of every request and can refuse hosts. It sees host names, not paths.

| Proxy | `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` | Requests | Session |
|---|---|---|---|
| Allows every host | Set (34 sessions) | `api.anthropic.com:443`: 3 in each of 32 sessions, two-turn ones included; 12 in the session that met a network fault; none in the `--bare` session | Completed, but for the `--bare` session |
| Allows every host | Not set | `api.anthropic.com:443`, 7; `http-intake.logs.us5.datadoghq.com:443`, 1 | Completed |
| Allows `api.anthropic.com` only | Set | `api.anthropic.com:443`, 3; nothing denied | Completed |
| Allows `api.anthropic.com` only | Not set | `api.anthropic.com:443`, 7; the Datadog host, 1, denied | Completed |
| Denies every host | Set | `api.anthropic.com:443`, 25, all denied | Failed as in the table above |

- With the switch set, two of the three connections are small and come first. The debug log names two fetches at start, "Policy limits" and "Remote settings" (remote managed settings, a 404 for this account). The third carries the model request. Both fetches go to the model host, so the allowlist cannot stop them, and with the host unreachable they hold the start up for 18 s.
- Without the switch the CLI loads a built-in telemetry plugin, opens four more connections to `api.anthropic.com` and one to Datadog. Denying the Datadog host did no harm.
- No socket of the CLI's process tree went anywhere but to the proxy. That is from samples taken every 0.3 s, so a short connection could be missed; a child that left the process tree is not followed; and a name lookup through the system resolver is not a socket of these processes.
- The debug log gives the token's scope as `user:inference` and switches off claude.ai connectors and the Chrome integration for want of other scopes.

### Against report 12

- Caps: confirmed, including that the budget cap counts the CLI's own estimate.
- Usage: confirmed, with two additions. `model_usage_since_last_result` gives a turn's own usage, and `message_delta` gives a call's real output tokens.
- `rate_limit_event`: its fields are as the SDK's type declarations have them, `resetsAt` is in seconds, and one is emitted in a session that is nowhere near a limit. The line order, `api_error_status` and exit code of a rejected request stay unobserved.
- Hosts: `api.anthropic.com` is enough for a `setup-token` session. `claude.ai`, `claude.com` and `platform.claude.com` were never asked for.
- Of the environment switches it lists, only `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` was tried.

### What changed in the spec

1. SEC-11 names `api.anthropic.com` as the model host of `subscription` mode, and AG-11 sets `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` in the environment of an agent session.
2. BUD-4 takes a call's token counts from `message_delta` and a session's dollars from its last `result`; between results the supervisor computes a running figure from the token counts and the price table of BUD-3. AG-1 gains `--include-partial-messages` for that. Of the two ways report 12 offers, this is the one that keeps the per-call records BUD-4 and ORCH-4 already ask for.
3. The token row of 13.5 says that the plan bounds the spend only while usage credits are off. The threat table of 13.1 cites it.
4. The contract test of 19.4 asserts that a session completes with only the model host reachable.
5. 8.3 says that the `rate_limit_event` with status `allowed` is not a report of a limit.

### Still open

- A session at the plan limit: the order of the lines, whether `resetsAt` is always present on a rejected request, the exit code, and what the stream shows when usage credits are on. Report 12 suggests recording the raw lines the first time the factory meets a limit.
- Why no five-hour window was reported.
- Linux. The hosts were observed with the macOS build.
- What the factory does when a session reports usage credits in use. Report 12 would stop the session. That is a decision, and it is not taken here.
- The rest of what report 12 asks of 8.3: stopping dispatch early on `allowed_warning`, leaving `CLAUDE_CODE_RETRY_WATCHDOG` unset, a pause that survives a restart, and acting on the event because the process outlives its error result. None is written back, since no session met a limit.
- No session used a subagent, whose calls report 12 says `message_delta` does not cover, and none ran in `direct_key` mode.
- The pass reported $0.04 in all, the sum of `total_cost_usd` over 38 sessions on the `haiku` alias. What share of the plan's window that is cannot be read from the stream.

## P-1: creating a ref that already exists

Answered 2026-11-20 for #13, by calls to the REST API on `sapientpants/sf-scratch`, made with `gh` 2.104.0 and `curl` on the operator's OAuth token. The factory will make this call with an installation token; none exists yet. The documentary research behind the question is `04-github-integration` ("report 04" below), which found 409 and 422 both documented for the call and `422 Reference already exists` reported but unverified.

**Answer.** `422 Unprocessable Entity`, with the message `Reference already exists`, whether the second call asks for the same SHA or another one. No call returned a 409. The status does not tell this apart from other failures: every fault in the ref name or the object that was tried got a 422 too, and only the message differed. One of those messages, `Reference update failed`, covers both an object that is not a commit and a branch name that collides with the new ref. So the claim cannot go by the status, and it should not go by a message GitHub does not document. It reads the ref after a 422, and the read decides (8.2 step 3). The read has a limit of its own: it cannot tell a ref the run made itself from one that another daemon made at the same SHA. So step 3 now takes a ref at the run's SHA for its own only when the run is replaying the create.

### Method

Each row of the table under "What the call returned" is one request, sent once under `X-GitHub-Api-Version: 2026-03-10`, the version GH-17 pins, and once under `2022-11-28`. The paired creates and the reads say for themselves which version they ran under. The server named the version in `X-GitHub-Api-Version-Selected` on every response whose headers were kept, except the 401, which carries no such header. The headers of the `curl` pairs were not kept.

```sh
gh api -i --method POST repos/sapientpants/sf-scratch/git/refs \
  -H "X-GitHub-Api-Version: <version>" -f ref=<ref> -f sha=<sha>
```

The refs follow 5.6: `refs/heads/factory/issue-9001` and its neighbours under the first version, `issue-8001` under the second. The SHAs:

| Name below | Value |
|---|---|
| The base | `a1d8965ce3a54cdee3a3190a786e6443937c8ef7`, the head of `main` |
| Another commit | `1f934b934c5d073f7953b596f3beeec04c25ffaf`, the head of `probe/p-16` |
| A tree, a blob | `15b48d174e237be5af22a12a341709e02162c134`, `1f17817c69d5eb273bb9f8de2de1d6c2cfb1cf5b`, both of the base |
| No object | `0123456789abcdef0123456789abcdef01234567` |

The scratch repository has no ruleset and no branch protection. All 43 refs the probe created were deleted afterwards, and the repository again holds only `main` and `probe/p-16`. The scripts that sent the requests and the responses are on the branch `prototype/existing-ref-probe`, at commit [`c7684be`](https://github.com/sapientpants/software-factory/tree/c7684beb75c5a06ebcaea1a3ba47ecba2d401e57/crates/factory-github/prototype/existing-ref), with the response headers cut down to status, date, API version and request id. They are throwaway code.

### What the call returned

Both API versions gave the same status and the same body in every case, apart from the issue number in a ref name. The row with a branch `factory` was run last, after the other refs had been deleted.

| Request | Status | `message` |
|---|---|---|
| A new ref at the base | 201 | None. The body is the ref, with `object.sha` and `object.type` |
| The same ref again, at the base | 422 | `Reference already exists` |
| The same ref again, at another commit | 422 | `Reference already exists` |
| The same ref again, at no object | 422 | `Object does not exist` |
| The same ref again, with `abc123` as the SHA | 422 | `Invalid request.\n\nAt least 40 characters are required; only 6 were supplied.\nabc123 does not match /^([0-9a-f]{40}(?:[0-9a-f]{24})?)$/.` |
| A new ref at no object | 422 | `Object does not exist` |
| A new ref with `abc123` as the SHA | 422 | The `Invalid request.` message above |
| A new ref without a `sha` field | 422 | `Invalid request.\n\n"sha" wasn't supplied.` |
| A new ref at a tree, and at a blob | 422 | `Reference update failed` |
| `heads/factory/issue-9002` and `factory/issue-9002` as the ref | 422 | `Reference name must start with 'refs/'.` |
| `refs/heads/factory/issue..9002`, and the same name with a space for the dots | 422 | `<the ref> is not a valid ref name.` |
| `refs/heads/factory/issue-9001/sub`, while the branch `factory/issue-9001` exists | 422 | `Reference update failed` |
| `refs/heads/factory`, while the branch `factory/issue-9001` exists | 422 | `Reference update failed` |
| `refs/heads/factory/issue-9001`, while a branch `factory` exists | 422 | `Reference update failed` |
| The first request without a credential | 401 | `Requires authentication` |

The whole body of the answer to the question:

```json
{"message":"Reference already exists","documentation_url":"https://docs.github.com/rest/git/refs#create-a-reference","status":"422"}
```

Every 422 had this shape: three fields, `status` as a string, the same `documentation_url`, and no `errors` array. The message is the only part that differs.

### Two creates at once

40 pairs of creates, each pair for one new ref and sent at the same moment on two connections. In 20 pairs both requests asked for the base; in 20, one asked for the base and one for the other commit. 20 pairs were two `gh api` processes started together, 10 under each API version. 20 were one `curl --parallel --parallel-immediate` process under `2026-03-10`.

In every pair one request got a 201 and the other a 422 with `Reference already exists`. The ref was afterwards at the SHA of the request that got the 201: read after each `gh` pair, and for the `curl` pairs in one listing after the last of them. When the two asked for different commits, either one won. No pair gave two 201s, a 409, or any other message.

Timings were taken for the `curl` pairs only. Both requests of a pair were in flight together there: each took between 0.48 and 0.83 s, and in two pairs the two answers arrived within a millisecond of each other. Whether any two requests reached the ref update on GitHub's side at the same instant cannot be seen from outside.

### Reading the ref afterwards

| Request, while `factory/issue-9001` exists | Status | Body |
|---|---|---|
| `GET /git/ref/heads/factory/issue-9001` | 200 | The ref, with `object.sha` |
| `GET /git/ref/heads/factory/issue-900` | 404 | `Not Found` |
| `GET /git/ref/heads/factory/issue-9501` | 404 | `Not Found` |
| `GET /git/ref/heads/factory` | 404 | `Not Found` |
| `GET /git/ref/heads/factory/issue-9001/sub` | 404 | `Not Found` |
| `GET /git/matching-refs/heads/factory/issue-900` | 200 | A list holding `refs/heads/factory/issue-9001` |

The first three rows were run under both API versions, the last three under `2026-03-10` only. `git/ref` matches the exact name, so issue 900 does not see the branch of issue 9001. `git/matching-refs` matches a prefix and is the wrong call for the fence. With a branch `factory` in place of `factory/issue-9001`, the read of `factory/issue-9001` returned 404 under both versions.

### Findings

**The status separates nothing.** 8.6 classes a 422 as Deterministic: no retry, escalate. At this call the same status is also the ordinary answer to a ref the run created itself before a crash, which 8.2 counts as created.

**The message does not say whose ref it is.** It is the same for the base and for another commit, and the ref itself holds nothing but a SHA. The claim needs the read to compare that SHA, and the comparison is all it has. A ref at the run's own `base_sha` that another daemon created reads exactly like the run's own ref from before a crash.

**GitHub lets one create through; step 3 lets the loser through when the SHAs match.** One 201 in each of 40 pairs, and the loser was told `Reference already exists`. That is the atomic create the fence of 8.2 is built on. In the 20 pairs where both requests asked for the same SHA, though, a read by the loser finds the ref at the SHA it asked for, and step 3 counts that as created. Two daemons that read the same default-branch head would both have gone on. The rule is older than this probe, and the step needs it only to be replayable (8.5), so it now applies only on a replay.

**A colliding branch name looks like a bad object.** Git cannot hold a branch `factory` next to a branch `factory/issue-9001`. All three creates of that kind were refused: `factory/issue-9001` while `factory` existed, `factory` while `factory/issue-9001` existed, and `factory/issue-9001/sub` while `factory/issue-9001` existed. The answer is `Reference update failed`, the message GitHub also gives for a tree or a blob, and a read of the ref that was asked for returns 404. A target repository with a branch named `factory` therefore fails every claim, and nothing in the response names the cause.

**A bad SHA is reported before an existing ref.** With a malformed SHA or one of no object, the call reports that and not the existing ref. No other fault was tried against an existing ref. The factory creates the ref at a SHA it has just read from the default branch, so this order does not hide an existing ref from it.

### Against report 04

- `422 Reference already exists` is confirmed.
- The 409 that the documentation lists for the call was not produced by any case here.

### What changed in the spec

1. 8.2 step 3 names the call. A 422 from it is not a Deterministic failure until the ref has been read by its exact name. If the ref exists, the rules that step already had apply to its SHA. If it does not, the 422 is Deterministic and the escalation carries GitHub's message.
2. Step 3 counts an existing ref at `base_sha` as created only on a replay: the attempt, which is now journalled before the create is sent, was already journalled when the daemon started, or an earlier create of the run ended in a Transient failure. A ref at `base_sha` that answers the first create of a run is foreign and aborts the claim. The operator decided this on 2026-11-20. The Branch row of 8.5 follows. The cost: a ref orphaned by a crash and the loss of the store is no longer adopted, and the issue goes to a human.
3. 8.2 says what the fence still does not stop: a run that replays the create cannot tell its own ref from one that another daemon made at the same SHA in the meantime.
4. The readiness report of 20.1 blocks on a branch named `factory`.

### Still open

- The replay rule itself. It is a rule for the daemon, which does not exist yet, so nothing here exercised it.
- An installation token. Every call here used the operator's OAuth token. What the App gets when it lacks a permission was not observed, and nor was whether its answers differ in any other case.
- A ruleset or branch protection that restricts creating `factory/**`. The scratch repository has neither.
- What produces the documented 409.
- The force-update of a ref left by a `cancelled` run (8.2) and the lease of a reset (7.6). They are `PATCH` calls and no part of this probe.
- GraphQL `createRef`. Not tried; GH-17 uses REST for writes.

## P-24: bubblewrap and `/dev/kvm` on the server

Answered 2026-11-20 for #7, by a second and a third run of the script behind [14-server-facts](14-server-facts.md) ("report 14" below). The second run followed the operator installing `bubblewrap` and Claude Code on the server, and was again the operator's own account, unprivileged, over SSH. The third was the factory's system user, which the operator had created in between as a member of group `kvm`.

**Answer.** Yes to all three, with stand-in commands and not the factory's sandbox. As the factory's user, with bubblewrap 0.12.0 from Debian 13, the five tests of P-16 pass on the server as provisioned, with no sysctl changed and no AppArmor profile unloaded, and the sandbox holds no network interface but `lo`. A sandbox that bubblewrap is given a seccomp filter for runs under that filter. The user opens `/dev/kvm`, and `KVM_GET_API_VERSION` and `KVM_CREATE_VM` succeed. No guest was booted.

### Method

The script is the one of report 14, changed after its first run to print nothing that identifies the server and to look for tools under `~/.cargo/bin` and `~/.local/bin`. It and its output are on the branch `prototype/server-facts`, at commit [`e2be280`](https://github.com/sapientpants/software-factory/tree/e2be2808996b7a293c90c04343e67a76be31da62/crates/factory-sandbox/prototype/server-facts); the output of the second run is `results/server-2026-11-20-second.txt`. That run was at 08:53 UTC, 24 minutes after the run of report 14. The script changes nothing on the server.

Between the first and the second run the operator installed the Debian package `bubblewrap` and Claude Code. Nothing else is known to have changed: the kernel, the sysctls, the list of security modules and the files in `/etc/apparmor.d` read the same in both outputs.

### bubblewrap, as the operator's account

`bwrap` is `0.12.0-1~deb13u1`, at `/usr/bin/bwrap` with mode `-rwxr-xr-x`, so it is not setuid and relies on unprivileged user namespaces. The tests are those of P-16, with the same arguments:

| | basic | caps | nested | copy | unshare |
|---|---|---|---|---|---|
| The server, as provisioned, in the second and the third run | ok | ok | ok | ok | ok |
| Hosted runner, Ubuntu 24.04, as provisioned (P-16) | fail | fail | fail | fail | fail |
| Hosted runner, Ubuntu 26.04, as provisioned (P-16) | ok | fail | fail | fail | fail |

Inside the basic sandbox, `/proc/net/dev` listed `lo` and nothing else, in both runs.

What follows:

- **The namespaces of level `process` (13.3) are available to an unprivileged account on the server.** `--unshare-all` takes a user, PID, IPC and network namespace among others. No step like the one DEV-6 needs on the hosted runners is needed here.
- **No AppArmor profile confines bubblewrap there (inference).** `caps` and `nested` pass, and on Ubuntu 26.04 they fail for as long as the `bwrap` profile is loaded (P-16). So a sandbox-escape test that passes on the server passes because of the sandbox's own policy. The loaded profiles were not listed, which needs root.
- **Nothing in 13.3 has to change on this evidence.** The evidence is stand-in commands; the factory's sandbox does not exist yet.

### What else the second run showed

- **Claude Code 2.1.311** is at `~/.local/bin/claude` of the operator's account. It is the version P-21 and P-22 ran on. The script asked it for its version only; no session ran, and whether a credential is in place was not looked at.
- **No Rust toolchain** for that account: `rustup`, `rustc` and `cargo` are neither on the `PATH` nor under `~/.cargo/bin`. Report 14 could not settle this.
- **`/var` is still full:** 104 MiB free of 22 GiB, against 64 MiB before.
- `/dev/kvm`, the sysctls, `git`, the listener on port 22 and the five HTTPS requests gave what report 14 records.

### As the factory's user

The third run was at 12:13 UTC, started by the operator on the server with `sudo -H -u sf bash /tmp/server-facts.sh`. The script is at commit [`23708d3`](https://github.com/sapientpants/software-factory/tree/23708d3382961ca59a0ea285c0b673824a454c60/crates/factory-sandbox/prototype/server-facts) of the same branch and the output is `results/server-2026-11-20-third.txt`, with one account name redacted. Since the second run the script also gives bubblewrap a seccomp filter.

Before it, the operator did two things on the server:

- **Created the user,** a system user named `sf`, and added it to group `kvm`. What the output shows of the result: a user with uid 103, and `getent group kvm` giving `kvm:x:993:sf`. Of the groups `kvm`, `sudo`, `wheel` and `docker` it is in `kvm` alone. Its home directory and its shell were not read. The spec names `sf` and the home directory `/home/sf` as defaults for this user. The operator stated the same day that `/home/sf` is a default and not a decision, and that the code is to support any home directory.
- **Removed the container images that no container used.** `/var` went from 104 MiB free to 11 GiB free of 22.

What the run showed:

| | |
|---|---|
| The five tests of P-16 | All `ok`, as in the second run |
| Network interfaces in the basic sandbox | `lo` alone |
| A seccomp filter | The script gives `bwrap --seccomp` a filter of one instruction that allows every system call, and runs `grep Seccomp /proc/self/status` inside. It printed `Seccomp: 2` and `Seccomp_filters: 1`: the process ran in filter mode with one filter attached |
| Opening `/dev/kvm` for reading and writing | `ok` |
| `KVM_GET_API_VERSION`, then `KVM_CREATE_VM` | `ok`: the version is 12 and the second call returned |
| Claude Code, Rust toolchain | Neither, for this user. The copy of Claude Code is under the operator's home directory |

`ls -l /dev/kvm` gave the same mode and owner as before with a new timestamp, later than the second run, so the device node was made again in between; the machine was probably restarted **(inference)**. The kernel, the sysctls and the list of security modules read as in the earlier runs.

What follows:

- **The server gives the factory's user what level `process` needs, and the device that level `hardened` needs (13.3).** Both rest on stand-in commands.
- **Membership of group `kvm` is enough for `/dev/kvm`.** No udev rule like the one the hosted runners need (P-16) is involved.
- **The kernel takes a seccomp filter from bubblewrap.** The filter allowed everything, so this says nothing about a filter of the factory's.

### What changed in the spec

1. The host requirements of 4.4 name defaults for the factory's user and its home directory, `sf` and `/home/sf`. CFG-4 is new: no home directory is built into the code, as the operator stated on 2026-11-20.
2. The table of 4.4 marks what the second and the third run added: `bubblewrap` is installed, the factory's user exists and can open `/dev/kvm`, an agent CLI is installed for the operator's account only, and `/var` is no longer full.
3. The provisioning sentence of 4.4 lists what is left: an agent CLI and a toolchain for the factory's user, and the data directory.
4. The P-24 row of 23.2 carries the answer.

### Still open

- **The factory's sandbox** in place of the stand-in commands, with its own seccomp filter, the mounts of SEC-6 and the proxy of SEC-10.
- **A guest.** Whether a Firecracker or Cloud Hypervisor microVM boots on the server is M2 work (D-5).
- **Which AppArmor profiles are loaded.** Not listed in any run; it needs root.
- **The user's home directory and shell on the server,** which no run read, and whether anything but the operator can become the user.
- **A Claude Code session on the server,** inside bubblewrap or outside it. Probe P-6 (#12) is the first. The CLI is installed for the operator's account and not for the factory's user.

## P-6: the agent CLI's Bash sandbox under bubblewrap

Answered 2026-11-20 for #12, for bubblewrap, by live sessions of Claude Code 2.1.311 on the server and in a container on the Mac. The outer sandbox was a stand-in built from `bwrap` arguments; the factory's sandbox does not exist yet. The microVM half of the probe was not tried.

**Answer.** It works on the server, once `socat` is installed, when the outer sandbox lets a process create a user namespace: the CLI's sandbox started inside the stand-in and confined the command further. It does not work when the outer sandbox is run with `--disable-userns`: each Bash call then fails. It does not have to be disabled in either case, because it is off unless a settings file turns it on, and with it off the Bash tool ran in the CLI's own namespaces inside both stand-ins. The setting is `sandbox.enabled`. Set to `false` in the host's `--settings` file, it turned the sandbox off on the server where the config directory had turned it on. AG-8 now sets it, since SEC-8 does not rely on that sandbox and nothing seen here is a reason to turn it on:

| | With `sandbox.enabled: true` | Seen |
|---|---|---|
| No `socat` | The CLI starts no sandbox, says so on stderr, and runs the command unsandboxed | On the server before `socat` was installed, and in the container with `socat` moved away |
| `socat`, and an outer sandbox in which a process can create a user namespace | The CLI's sandbox nests. The command wrote in the working directory, was denied a second directory that the outer sandbox left writable and `/tmp`, and was refused `api.anthropic.com` | On the server and in the container |
| `socat`, and an outer sandbox run with `--disable-userns` | Each Bash call fails with a `bwrap` error. Unless `allowUnsandboxedCommands` is `false`, the model can ask for the command to run unsandboxed, and it then does | On the server and in the container |

### Method

One script, on the branch `prototype/inner-sandbox` at commit [`d23e6b7`](https://github.com/sapientpants/software-factory/tree/d23e6b72e805cc92ed668dde11b7f858def2a352/crates/factory-agent/prototype/inner-sandbox), with the output of five runs under `results/`. It is throwaway code. The first server run and the first container run were made by the script at commit `cbe7a0c`. The later runs were made by the script at `9fc8672`, which has two more cases and lists the lines of each stream that mention a sandbox.

Each case is one `claude -p` session on a `claude setup-token` credential, on the `haiku` alias. The flags are those of AG-1 in `subscription` mode that a session of one prompt and one tool needs: `--setting-sources user`, `--strict-mcp-config`, `--output-format stream-json --verbose`, `--session-id`, `--model`, `--permission-mode dontAsk`, `--permission-prompts none`, `--tools`, `--allowedTools`, `--settings` and `--max-turns`. `--input-format stream-json`, `--mcp-config`, `--json-schema`, `--max-budget-usd`, `--append-system-prompt-file` and `--include-partial-messages` were left out. Bash is the only tool, and the permission rule allows one command, `sh ./inside.sh`, which the prompt asks for. That script prints the namespaces it runs in, its seccomp state, whether it can write in the working directory, in a second directory that the outer sandbox leaves writable and in `/tmp`, and whether `curl` reaches `https://api.anthropic.com`. It appends the same text to a file in the working directory, and the harness records the namespaces the CLI itself started in. A command is counted as sandboxed by the CLI when its namespaces differ from the CLI's, so the verdict does not rest on what the model says.

A case is an outer sandbox and the `sandbox` block of the host's `--settings` file:

| Outer sandbox | |
|---|---|
| `none` | The CLI runs as the user, with a `HOME` of its own |
| `bwrap` | `bwrap --unshare-user --unshare-pid --unshare-ipc --unshare-uts --unshare-cgroup-try`, with the machine's `/usr` and `/etc` bound read-only, a private `/proc`, `/dev` and `/tmp`, the CLI's binary bound read-only, and four directories bound read-write. The network namespace is shared, because the proxy of SEC-10 does not exist and the CLI has to reach the model endpoint. No seccomp filter |
| `nouserns` | The same with `--disable-userns`, after which nothing inside can create a user namespace. This is one way for an outer sandbox to forbid nested namespaces. A seccomp filter that does so was not tried |

| `sandbox` block | Cases |
|---|---|
| None | `none-default`, `bwrap-default`, `nouserns-default` |
| `enabled: false` | `bwrap-off`, `nouserns-off` |
| `enabled: true` | `bwrap-on`, `nouserns-on` |
| `enabled: true`, `allowUnsandboxedCommands: false` | `none-on-strict`, `bwrap-on-strict`, `nouserns-on-strict` |
| The last, with `enableWeakerNestedSandbox: true` | `bwrap-on-weaker`, `nouserns-on-weaker` |
| The last but one, with `failIfUnavailable: true` | `nouserns-on-fail` |
| None, and `enabled: true` with `allowUnsandboxedCommands: false` in the `settings.json` of the config directory | `bwrap-user-on` |
| `enabled: false`, and the same block in the config directory | `bwrap-user-on-host-off` |

The runs, all on 2026-11-20:

| | The server | The container |
|---|---|---|
| When, and which cases | 18:35 UTC, without `socat`: the first thirteen. 18:59, after the operator installed `socat`: all fifteen | 18:31 UTC: the first thirteen. 18:45: the two `user` cases and the six `nouserns` cases. 18:46, with `socat` moved out of the `PATH`: the three `on-strict` cases |
| Machine | Debian 13, kernel `6.12.116+deb13-amd64`, x86-64 (4.4) | Debian 13 in Docker Desktop on the Mac, started with `--privileged`; kernel `7.0.14-linuxkit`, arm64 |
| User | The factory's user, uid 103. The operator started the script (D-16), from a login, with `sudo -H -u sf` | A system user with no home directory |
| CLI | A copy of the operator's Claude Code 2.1.311 binary, placed in `/tmp` for each run. The factory's user has none of its own | 2.1.311 |
| `bubblewrap`, `socat` | 0.12.0; not installed at first, then Debian's 1.8.0.3 | 0.12.0; 1.8.0.3 |
| Token | The operator's, given to the script on standard input and to the sessions in the environment | The same |

In the output of the first server run the names of 13 network interfaces are replaced by `<13 names redacted>`; the script has since printed their number. The two server runs reported 0.005 and 0.011 API-equivalent dollars, and the three container runs 0.016 together.

The documentation was read the same day, through a fetch tool: the first part of <https://code.claude.com/docs/en/sandboxing> and the index of <https://code.claude.com/docs/en/settings-reference> word for word, the rest as a summary. The quotations here are from the parts read word for word. The sandboxing page says: "The sandbox is off by default", and that it is turned on with `/sandbox` or by setting `sandbox.enabled` to `true` in a settings file; that on Linux it "relies on" `bubblewrap` and `socat`; and that "when the platform doesn't support the sandbox or the dependency check at startup fails, Claude Code runs commands without sandboxing", unless `sandbox.failIfUnavailable` is `true`, which makes it "exit at startup instead". What was read names no command-line flag and no environment variable for any of this, and does not say what happens when the dependencies are present and the sandbox cannot start.

### On the server, without `socat`

The CLI's sandbox started in no case. The stand-in binds the machine's `/usr`, so what the machine lacks the sandbox lacks; in the factory's sandbox that is decided by the baseline of SEC-6. The preflight ran the outer sandboxes without the CLI, in this run and the next: both start, and a second `bwrap` inside the first succeeds under `bwrap` and fails under `nouserns` with `Creating new namespace failed: nesting depth or /proc/sys/user/max_*_namespaces exceeded (ENOSPC)`.

| Cases | Exit | `result` | The command |
|---|---|---|---|
| The three with no `sandbox` block and the two with `enabled: false` | 0 | `success` | Ran in the CLI's namespaces, wrote in all three places, and got an HTTP 404 from `api.anthropic.com`, which means the host answered. Nothing on stderr |
| The seven with `enabled: true` and no `failIfUnavailable` | 0 | `success` | The same. On stderr, the warning below |
| `nouserns-on-fail` | 1 | `error_during_execution`, 0 turns | Did not run: the CLI refused to start. No `system/init` message. On stderr, the error below |

So twelve sessions ran and one did not start. The warning, in all seven cases:

```
⚠ Sandbox disabled: sandbox is enabled but dependencies are missing: socat not installed · install missing tools (e.g. apt install bubblewrap socat) or see https://code.claude.com/docs/en/sandboxing
  Commands will run WITHOUT sandboxing. Network and filesystem restrictions will NOT be enforced.
```

The error:

```
Error: sandbox required but unavailable: sandbox is enabled but dependencies are missing: socat not installed · install missing tools (e.g. apt install bubblewrap socat) or see https://code.claude.com/docs/en/sandboxing
  sandbox.failIfUnavailable is set — refusing to start without a working sandbox.
```

In the seven cases the `system/init` message had no field that mentions a sandbox, and neither the tool result nor the `result` said anything of it. The script of that run looked at those three messages only, and the streams were not kept.

Under the outer sandboxes the command saw 9 processes, against 446 and 447 without one, and ran with `NoNewPrivs: 1` and no seccomp filter. That is the stand-in at work, not the CLI.

### On the server, with `socat`

All fifteen sessions ran, and nothing was written to stderr in any of them. The cases with no `sandbox` block and with `enabled: false` gave what they gave before. The others:

| Case | The command |
|---|---|
| `none-on-strict`, `bwrap-on`, `bwrap-on-strict`, `bwrap-on-weaker` | Ran in a new user, mount, PID and network namespace, with `Seccomp: 2` and two filters. It saw 8 processes and no network interface but `lo`, against 13 others outside. It wrote in the working directory. Writing in the second directory and in `/tmp` was denied. `curl` got a 403 from the CLI's proxy, and the tool result carried `deny network-outbound api.anthropic.com:443` |
| `nouserns-on` | Failed with `bwrap: Creating new namespace failed`. The model then called Bash again with `dangerouslyDisableSandbox: true`, and that call ran in the CLI's namespaces and wrote in all three places |
| `nouserns-on-strict`, `nouserns-on-weaker`, `nouserns-on-fail` | Failed with the same `bwrap` error, as a tool result marked `is_error`, and was not run again |
| `bwrap-user-on`: no `sandbox` block in `--settings`, and `enabled: true` with `allowUnsandboxedCommands: false` in the `settings.json` of the config directory | Sandboxed by the CLI, as `bwrap-on-strict` was |
| `bwrap-user-on-host-off`: the same config directory, and `enabled: false` in `--settings` | Ran in the CLI's namespaces and wrote in all three places |

Every one of these sessions ended with exit code 0 and a `result` of subtype `success`, the three in which no command ran included. In those three the model's answer said that the command had not run.

In the stream, the `system/init` message again had no field that mentions a sandbox. Where the CLI's sandbox ran, the one line that mentions it is the tool result, which ends in a `<sandbox_violations>` block naming the refused host. Where it failed, the `bwrap` error is in the tool result and the model's answer repeats it.

### In the container

The run of 18:31 and the run of 18:45 gave, case for case, what the server gave with `socat`. The container is where the script was rehearsed, and it adds two things.

`nouserns-on` ended with the second, unsandboxed call in five of six runs: on the server, in the two container runs that are kept, and in two of three earlier container runs whose output is not on the branch. In the other the model said it would not run, outside the sandbox, a script it had not read. Every other case gave the same result each time it ran.

The run of 18:46, without `socat`: the three `on-strict` cases gave what they gave on the server without `socat`, with the same warning on stderr. Each stream had nine lines, of types `system/init`, `system/thinking_tokens`, `assistant`, `user`, `rate_limit_event` and `result`, and none of them holds the word `sandbox`, `socat` or `bwrap`.

### Findings

**Off is the default, and off the CLI adds nothing to the outer sandbox.** With no `sandbox` block and with `enabled: false`, the command's namespaces were the CLI's in every case, in every run. This is the state the factory runs in.

**`enabled: false` in the host's file is a setting that does something.** On its own it cannot be told from the default. Against a config directory that turns the sandbox on, it turned it off, on the server. Under AG-4 the host writes both files, so this matters if the CLI's default changes or if the config directory is ever not what the host wrote.

**Without `socat`, turning it on turns nothing on, and the stream does not say so.** The CLI goes on without a sandbox whatever else the block says: `allowUnsandboxedCommands: false` did not stop the command. The warning is on stderr. In the container no line of the stream mentioned it; on the server the three messages that were looked at did not.

**Where it starts, it takes away part of what the outer sandbox leaves.** Nested under `bwrap` on the server, the command wrote in the working directory, was denied the two other places, and was refused the one host it tried. The documentation gives the defaults: a command "can write to the current working directory, the per-user temp directory" and directories added in settings, and hosts are checked against "allowed domains, which start empty". A stage's target directory and temporary directory (SEC-6) and the crate registry (SEC-11) are outside those limits **(inference: no build was run, and the stand-in is not the factory's sandbox)**. They could be opened in the same settings block, which would state the factory's policy a second time, in a layer SEC-8 does not rely on.

**Where it cannot start, the session's result does not show it.** Under `nouserns` the dependency check passes, since `bwrap` and `socat` are both there, so `failIfUnavailable` does not stop the CLI at startup. Each Bash call fails when it is made, and the failure is in the tool result. The session still ends with `success` and exit code 0. Unless `allowUnsandboxedCommands` is `false`, the model can ask for the command to run unsandboxed, and it did so in five runs of six. `enableWeakerNestedSandbox` does not help: the settings reference describes it as the way to "run the Linux sandbox inside an unprivileged container", and the failure here is the user namespace, which that setting does not do without.

**`failIfUnavailable` catches the missing dependency and not the sandbox that cannot start.** It ended the session on the server while `socat` was missing, and changed nothing once it was there.

**None of this is needed to decide AG-8.** SEC-8 already says that the CLI's sandbox is not relied on. What the probe adds is that turning it on gains nothing in any state that was tried, and which setting keeps it off.

### What changed in the spec

1. AG-8 has the host's `--settings` file set `sandbox.enabled` to `false`, in every session. It no longer makes this depend on a conflict with the outer sandbox.
2. SEC-8 cites AG-8 for the sandbox being off.
3. The table of 4.4 records that sessions of the CLI ran on the server as the factory's user, and that the operator installed `socat` for the second of the two runs.
4. The P-6 row of 23.2 carries the answer and says which parts are open.

### Still open

- **The microVM.** The other half of P-6. It waits for a guest (D-5, M2). AG-8 does not wait for it: the setting is the same in every session.
- **The factory's sandbox** in place of the stand-in: its seccomp filter, its own network namespace and the proxy of SEC-10, and whether it lets a worker create user namespaces. 13.3 does not say. Nor is it known whether the in-sandbox forwarder of SEC-10 will need `socat` itself.
- **A test that the setting holds.** A contract test (19.4) that only checks that a command runs in the CLI's namespaces passes on a machine without `socat` even if the setting is ignored. It has to start from a state in which the sandbox would otherwise be on, as `bwrap-user-on-host-off` does, and so needs `socat` where it runs. Not added to 19.4 here.
- **Other CLI versions,** a changed default and a renamed key among them. One version ran.
- **A CLI of the factory's own on the server.** The sessions ran from a copy of the operator's binary in `/tmp`.
- **The whole stream on the server without `socat`.** Not kept. The container without `socat` stands in for it.

## P-4: how an issue form renders into the issue body

Answered 2026-11-20 for #14, by three issues that the operator created in the web interface of `sapientpants/sf-scratch` through the issue form of 6.2, read back with `gh` 2.104.0 on the operator's OAuth token. Report 07 (`07-gaps`, R7) found the layout undocumented and asked for a tolerant parser for that reason.

**Answer.** The body is one block per field, in the form's order, with a blank line between blocks. A block is the line `### <label>`, a blank line, and the field's text exactly as it was submitted. A field left empty gets the text `_No response_`. That is all GitHub adds: no text before the first heading, no newline at the end, no escaping, and nothing on the issue that says a form made it.

The section parser of WI-1 holds for such a body: a sketch of it read back every field of the two ordinary issues as submitted. It cannot hold for every body. A field's text goes in unescaped, so a line of it that reads `### Outcome` is the form's own heading byte for byte, and the body no longer says where the field ends. WI-1 now says what the parser accepts, and WI-1a makes a repeated section name a failure of the readiness lint.

### Method

The form, committed to the scratch repository's `main` as `.github/ISSUE_TEMPLATE/factory-task.yml` (commit `8829957`):

```yaml
name: Factory task
description: A task or bug with one verifiable outcome, for the factory to implement.
body:
  - type: textarea
    id: outcome
    attributes:
      label: Outcome
      description: "One or two sentences: what is true when this is done."
    validations:
      required: true
  - type: input
    id: spec-refs
    attributes:
      label: Spec refs
      description: Requirement ids and the spec file path, or "none".
  - type: textarea
    id: context
    attributes:
      label: Context
      description: Crates, modules or files likely involved; links.
  - type: textarea
    id: acceptance-criteria
    attributes:
      label: Acceptance criteria
      description: "A list AC-1 … AC-n. Each has a statement in the form WHEN ⟨condition⟩ THE SYSTEM SHALL ⟨behaviour⟩ and a verification method: test, property, review or manual."
    validations:
      required: true
  - type: textarea
    id: out-of-scope
    attributes:
      label: Out of scope
      description: What must not change.
  - type: textarea
    id: stop-conditions
    attributes:
      label: Stop conditions
      description: Situations in which the agent must stop and ask.
```

6.2 names the six sections and their order. The rest is this probe's choice: one `input` among five `textarea` elements, to see both, and only the two fields that tier 0 needs (6.1) marked required, so that the others could be left empty. The form has no `labels` key (GH-3).

The operator created three issues, each from a link that prefilled the form through query parameters named after the field ids, and pressed Create without changing a field. No text was typed by hand.

| Issue on `sf-scratch` | Submission | Body |
|---|---|---|
| #3 | Every field filled: prose, nested and numbered lists, a link, a fenced block | 794 bytes |
| #2 | Only the two required fields | 283 bytes |
| #1 | Text chosen to look like the form's output: headings with the form's own names, a field of spaces and line breaks, a literal `_No response_`, a carriage return | 1347 bytes |

Each issue was read four ways: `GET /repos/{owner}/{repo}/issues/{n}` under API versions `2026-03-10` and `2022-11-28`, the same with `Accept: application/vnd.github.full+json` for `body_html`, and `repository.issue.body` through GraphQL. The four bodies of an issue were the same bytes. The form, the links, the field values, the responses and the parser sketch are on the branch `prototype/issue-form-probe`, at commit [`f6b1898`](https://github.com/sapientpants/software-factory/tree/f6b18985242f75f51f0d2279502b12f0c5f80696/crates/factory-core/prototype/issue-form). They are throwaway code. The form and the three issues stay on the scratch repository.

### The layout

The whole body of #2, the issue with four empty fields. It does not end in a newline:

````markdown
### Outcome

The typo in the README is fixed.

### Spec refs

_No response_

### Context

_No response_

### Acceptance criteria

- AC-1 (review): WHEN the README is read THE SYSTEM SHALL spell `factory` correctly.

### Out of scope

_No response_

### Stop conditions

_No response_
````

| Part | As observed |
|---|---|
| Heading | `### ` and the field's `label`, for the `input` as for a `textarea`. The field's `id` and `description` appear nowhere |
| Order | The form's, with every field present, filled or not |
| Between a heading and its text, and between blocks | One blank line, which is two line feeds |
| An empty field | `_No response_`, the same for the `input` and a `textarea`. GitHub renders it as *No response* in italics |
| The end of the body | The last field's text. No newline follows |
| A list | As submitted, with its markers: `-`, `*`, `1.` and `- [ ]` all arrived unchanged, and so did the two-space indent of a nested item |
| A fenced block, inline code, a link | As submitted |

### What a field's text can carry

From #1. In each row the body holds the field's text byte for byte.

| Submitted | In the body | GitHub's HTML |
|---|---|---|
| A field of three spaces, two line feeds and two spaces | Those seven characters, between the usual blank lines. Not `_No response_` | Nothing between the two headings |
| `_No response_`, as the text of the `input` | `_No response_` | *No response*, as for an empty field |
| Two line feeds before the first line and three after the last | Kept, so four line feeds follow the heading and five precede the next one | One paragraph |
| Spaces at the start and at the end of a line | Kept | Not shown |
| A carriage return and a line feed, sent as `%0D%0A` | Kept: the body holds one `\r\n` among line feeds | A line break, as for a line feed alone |
| A tab, accented letters, curly quotes, kanji, an emoji | Kept | As text |
| `<!-- an HTML comment -->` and a `<details>` element | Kept | The comment is not rendered |
| `### Acceptance criteria` and `### Outcome`, each on a line of the Context field | Kept, unescaped | Level-3 headings, as the form's own |
| `### Stop conditions` inside a fenced block | Kept | Code |
| `### outcome`, `### Outcome ###` and ` ### Outcome` | Kept | Level-3 headings reading "outcome", "Outcome" and "Outcome" |
| `    ### Outcome`, four spaces in | Kept | Code |
| `###Outcome` | Kept | Text. With the lines `Outcome` and `-------` after it, it became one level-2 heading |

The body of #1 therefore holds five level-3 headings that read "Outcome" in one case or another and two that read "Acceptance criteria", where the form wrote one of each.

Nothing else on the issue tells a form from a hand-written body. `labels` is empty, `type` is null, the timeline is empty, and GraphQL reports `lastEditedAt` null, `includesCreatedEdit` false and no content edits.

### The parser sketch

A line rule in Python, not a Markdown parser. A section starts at a level-3 ATX heading whose text is one of the six names, compared without case, outside a fenced code block: up to three spaces, `###`, a space or tab, the text, and an optional closing run of `#`. It ends at the next such heading. Blank lines at either end are dropped, and a text that is then empty or `_No response_` counts as empty.

| Issue | Level-3 headings: the sketch, and `<h3>` in GitHub's HTML | Sections against the fields as submitted |
|---|---|---|
| #3 | 6 and 6, the same list | All six equal |
| #2 | 6 and 6, the same list | The two filled fields equal; the four others empty |
| #1 | 12 and 12, the same list | "Outcome" starts five sections and "Acceptance criteria" two. Context is cut after 51 of its 445 characters and Stop conditions after 62 of 146, each at a heading inside it |

Two simpler parsers, which split on lines starting `### ` and keep one section per name, read #3 and #2 correctly and #1 wrongly either way. Keeping the first, the acceptance criteria are the one line `AC-9` that was typed into Context. Keeping the last, the outcome is "A second outcome.", also from Context. Neither knows fences, so the first also takes the Stop conditions from inside the fenced block.

### Findings

**The layout is regular, and the heading is the label.** Keying on heading text is enough to find the sections of a body the form made from ordinary text. Every field of #3 and #2 came back as submitted.

**Empty is the empty string and nothing else.** A field of spaces and line breaks is kept as it is, and GitHub renders nothing for it. A field in which someone typed `_No response_` is the same bytes as an empty one. The lint asks for sections that are "present and non-empty" (6.4), so the parser has to call both empty.

**The form does not escape, so a body can be ambiguous.** A heading typed into a field is the form's own heading in the body and in the page the operator reads. No rule of position recovers the field: in #1 the typed "Acceptance criteria" comes before the real one and the typed "Outcome" after the real one. A parser that picks one silently hands an agent criteria the operator did not write as criteria. So a repeated name fails the lint, and the operator rewords the issue (WI-1a). The cost is an issue that quotes one of the six headings as a heading of its own, outside a fence, which cannot be dispatched until it is reworded.

**Where a heading counts has to follow what GitHub renders.** The operator approves the page, and the parser reads the source. On these three bodies the line rule and GitHub's HTML agree on every level-3 heading: the one in a fence and the one indented four spaces are code, and the ones with a leading space, a closing `###` or another case are headings. An issue about the parser itself will quote these headings, which is why the fence matters. Matching without regard to case goes with this: `### outcome` is a heading on the page, so it counts as the name and trips WI-1a.

**The body keeps a carriage return it is given.** Whether a browser sends one for a line typed in a field was not seen, since every field was prefilled. The parser takes either line ending.

**Nothing marks a form-made issue.** The factory cannot tell one from a body written by hand, by `gh issue create` or by the factory itself (PLAN-5), so one parser reads them all, and a body is valid by its sections and not by how it was made.

**A new submission cannot be automated.** Only the web interface renders a form. The round-trip test that WI-1 put in the probe suite of `sf doctor --probe` therefore runs against these recorded bodies as fixtures, and seeing whether GitHub changed the layout takes the operator and a browser (WI-1b).

### Against reports 03 and 07

- Report 07 (R7): the documentation was not read again, so whether it now describes the layout is not known. What the report asked for, a tolerant parser with a lint that says what failed, stands. The tolerance that the observed layout needs is small: line endings, blank lines, the empty marker and case.
- Report 03 proposed a body "parsed strictly". Strict by position does not work on #1, and strict on heading text alone is what WI-1 already had.

### What changed in the spec

1. 6.2 states the layout as observed.
2. WI-1 says what the parser accepts: the sanitised body (SEC-4), either line ending, a level-3 ATX heading outside a fenced code block with one of the six names in any case, other headings as content, blank lines trimmed, and `_No response_` or nothing as empty.
3. WI-1a is new: a name that starts more than one section fails the readiness lint, and no section of that body is used. 6.4 lists the rule.
4. WI-1b replaces the round-trip test in the probe suite: the recorded bodies are fixtures of the parser's tests, and a new submission is the operator's to make.
5. The P-4 row of 23.2 carries the answer.

### Still open

- **Text typed by hand.** All three issues were prefilled from a link. What a browser sends for a line break typed in a field was not observed.
- **A required field of spaces only.** Not submitted, so it is not known whether the form accepts it. The lint decides either way.
- **Space at the end of the last field.** No submission ended in any, so it is not known whether GitHub trims the end of the body.
- **Which fields the form requires, and their element types.** 6.2 does not say, and the scratch form's choices were made to see empty fields. Tier 1 needs all six sections (6.1); whether the form or only the lint enforces that is undecided. It matters when the form is enforced (M3).
- **The line format of an acceptance criterion.** The list arrives as typed, under any marker. 6.4 needs each criterion's verification method and 10.5 its id, and 6.2 gives no line format to read them from. Also M3.
- **The other elements.** `dropdown`, `checkboxes`, `markdown`, `upload` and a `textarea` with `render`. The form of 6.2 uses none.
- **A Markdown parser in place of the line rule.** The rule does not know block quotes, list items or HTML blocks, so `> ### Outcome` and `- ### Outcome` are headings on the page and content to the rule, and a heading line inside an HTML block is the reverse. 19.2 lists no Markdown crate. The agreement with GitHub's HTML was shown for three bodies.
- **The order of sanitisation and parsing on real text.** WI-1 puts SEC-4 first, so a heading inside an HTML comment is gone before the parser runs. Neither exists yet.
- **A body edited after creation,** and a body read with an installation token. Not tried.
