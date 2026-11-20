# 14 - The server's facts

Observed 2026-11-20. Resolves issue #7: what the operator's server is, measured against the host requirements of SPEC 4.4. The summary is the table in SPEC 4.4; this report is the evidence behind it.

Method: one shell script, run once by the operator over SSH from the Mac, as the operator's own account and without root. The script reads system state, tries the operations the factory will need, and makes HTTPS requests; it changes nothing on the server. The script as it ran and its output are on the branch `prototype/server-facts`, at commit [`82693bd`](https://github.com/sapientpants/software-factory/tree/82693bd4270e05933616bf685181ce6066c40e25/crates/factory-sandbox/prototype/server-facts). They are throwaway code. Nothing here was observed as root, as a member of group `kvm`, or as the factory's own user, which does not exist yet.

The repository is public, and the operator chose to name nothing that identifies the server. The saved output has the account's name, the host name and a network mount redacted, and this report gives no address. How the server is reached from outside (section 2.6) is the operator's statement, not an observation. Statements marked **(inference)** are my reading of the output.

## 1. Answer

| The ticket asks | Observed |
|---|---|
| Distribution and version | Debian GNU/Linux 13 (trixie), on bare metal |
| Kernel | `6.12.116+deb13-amd64`, Debian's 6.12.116-1 of 2026-11-07 |
| CPU architecture, cores | x86-64. One Intel Core Ultra 5 125H: 14 cores, 18 logical CPUs, VT-x on all of them |
| Memory | 14.9 GiB (`MemTotal` 15671772 kB), 12.1 GiB of it available at the time; 977 MiB of swap |
| Free disk | One 931.5 GiB NVMe disk. `/`: 49 GiB free of 55. `/var`: 64 MiB free of 22 GiB. `/srv`: 792 GiB free of 838 |
| Is `/dev/kvm` usable by an unprivileged user | No. It is `crw-rw---- root kvm`, group `kvm` has no member, and the account got `Permission denied` on opening it |
| Are unprivileged user namespaces enabled | Yes. `kernel.unprivileged_userns_clone` is 1, and the account created a user and a mount namespace and mounted a tmpfs in them |
| AppArmor or sysctl restriction on them | None found. AppArmor is enabled, but the kernel has no `kernel.apparmor_restrict_unprivileged_userns`, the sysctl by which Ubuntu restricts them. Which profiles are loaded was not read |
| `git` | 2.47.3 (`1:2.47.3-0+deb13u1`) |
| `bubblewrap` | Not installed |
| How it is reached | Over SSH. `sshd` listens on port 22 on every IPv4 and IPv6 address. From the operator's home network, and since recently over the operator's VPN from anywhere |
| Can it reach GitHub and the Anthropic API | Yes, over HTTPS and with no proxy variable set: `github.com`, `api.github.com` and `api.anthropic.com` answered, and so did the two crates.io hosts |

## 2. What was observed

### 2.1 The machine

| | |
|---|---|
| `/etc/os-release` | `PRETTY_NAME="Debian GNU/Linux 13 (trixie)"`, `VERSION_ID="13"` |
| `uname -srm`, `uname -v` | `Linux 6.12.116+deb13-amd64 x86_64`, `#1 SMP PREEMPT_DYNAMIC Debian 6.12.116-1 (2026-11-07)` |
| `lscpu` | Intel(R) Core(TM) Ultra 5 125H; 1 socket, 14 cores per socket, 18 CPUs, `Virtualization: VT-x`. `Thread(s) per core: 2` is not true of every core, since 14 cores give 18 CPUs |
| `/proc/meminfo` | `MemTotal` 15671772 kB, `MemAvailable` 12637864 kB, `SwapTotal` 1000444 kB |
| `systemd-detect-virt` | `none` |
| `systemctl --version` | systemd 257 (`257.13-1~deb13u1`) |
| `/sys/fs/cgroup` | `cgroup2fs` |

The disk is one NVMe device of 931.5 GiB, all ext4 apart from the EFI partition:

| Mounted on | Size | Used | Available |
|---|---|---|---|
| `/` | 55 GiB | 2.7 GiB | 49 GiB |
| `/var` | 22 GiB | 21 GiB | 64 MiB |
| `/srv` | 838 GiB | 3.2 GiB | 792 GiB |
| `/boot/efi` | 975 MiB | 9.1 MiB | 966 MiB |

There is a 977 MiB swap partition and no separate `/home`. A network filesystem is mounted as well; the factory does not use it.

The account that ran the script is a member of groups `sudo` and `docker` among others, and not of `kvm`. That a group `docker` exists is all the script saw of Docker.

### 2.2 `/dev/kvm`

- `ls -l /dev/kvm`: `crw-rw---- 1 root kvm 10, 232`.
- `getent group kvm`: `kvm:x:993:`, so no member.
- `vmx` is among the CPU flags of all 18 logical CPUs, and `kvm_intel` and `kvm` are loaded.
- Opening the device for reading and writing failed with `Permission denied`. The two ioctls the script goes on to make, `KVM_GET_API_VERSION` and `KVM_CREATE_VM`, were therefore not tried.

So the device is there and the hardware supports it, and an unprivileged account cannot use it. This is the same state as the x86-64 hosted runners before their udev rule (P-16). On the server the way in is membership of group `kvm` **(inference: nobody has opened the device as a member yet)**.

### 2.3 User namespaces

| | |
|---|---|
| `kernel.unprivileged_userns_clone` | 1 |
| `user.max_user_namespaces` | 60236 |
| `kernel.apparmor_restrict_unprivileged_userns` | The sysctl does not exist |
| `kernel.apparmor_restrict_unprivileged_unconfined` | The sysctl does not exist |
| `/sys/kernel/security/lsm` | `lockdown,capability,landlock,yama,apparmor,tomoyo,bpf,ipe,ima,evm` |
| `/sys/module/apparmor/parameters/enabled` | `Y` |
| Files in `/etc/apparmor.d` named for `bwrap` or `userns` | `unprivileged_userns`, `lxc-usernsexec`. No `bwrap-userns-restrict` |

The one test that ran, as the unprivileged account:

```sh
unshare --user --map-root-user --mount -- mount -t tmpfs tmpfs /mnt
```

It succeeded. On the Ubuntu 24.04 and 26.04 hosted runners the same command failed as provisioned and passed only once Ubuntu's AppArmor restriction was lifted (P-16, column "unshare"). The sysctl that carries that restriction does not exist on the server's kernel, and the file `unprivileged_userns` in `/etc/apparmor.d` did not stop the test. Listing the loaded profiles needs root and was not done, and neither was reading the kernel log for denials.

### 2.4 `git` and `bubblewrap`

- `git version 2.47.3`, package `1:2.47.3-0+deb13u1`, at `/usr/bin/git`. SPEC 4.4 asks for 2.42 or later. 2.47.3 is one of the two versions the capture prototype ran on (P-7).
- `bubblewrap`: `dpkg-query` finds no such package and no `bwrap` is on the `PATH`. The four tests of P-16 that use it (basic, caps, nested, copy) did not run.
- On the same day, `apt-get install bubblewrap` in a Debian 13 container on the Mac (arm64) gave `0.12.0-1~deb13u1`. That is what the distribution offers, not something seen on the server.

### 2.5 Other tools

`curl` 8.14.1 and `python3` are installed. `claude`, `rustup`, `rustc` and `cargo` were not found, but the script only looked on the `PATH` of a shell that reads no profile, so a copy under `~/.cargo/bin` or `~/.local/bin` would have been missed. Whether they are installed is not settled.

### 2.6 How the server is reached

- The run itself came in over SSH.
- `systemctl is-active` reports `ssh.service` active and `ssh.socket` inactive. `ss -tln` shows a listener on port 22 on `0.0.0.0` and on `[::]`.
- The operator's statement, 2026-11-20: the server is reached from the home network, and a VPN recently installed on it and on the Mac reaches it from elsewhere, to make development possible while travelling.

How `sshd` authenticates (keys, passwords, root login) was not read.

### 2.7 What the server reaches

Each line is one `curl` over HTTPS from the account's shell, with no proxy variable set in its environment. Any HTTP status means the host answered.

| URL | Status |
|---|---|
| `https://github.com` | 200 |
| `https://api.github.com` | 200 |
| `https://api.anthropic.com` | 404 |
| `https://index.crates.io/config.json` | 200 |
| `https://static.crates.io` | 404 |

`git ls-remote https://github.com/sapientpants/sf-scratch.git HEAD` succeeded. These are the hosts of SEC-11 and the two GitHub hosts the control plane uses. No request carried a credential, so nothing here shows an authenticated call working.

## 3. Against the requirements of SPEC 4.4

| Requirement | State on 2026-11-20 |
|---|---|
| A Linux server on the operator's own hardware, x86-64 or arm64 | Met |
| KVM available to the factory's user | Not yet. The device and the module are there; the factory's user has to be created as a member of group `kvm` |
| Unprivileged user namespaces enabled | Met |
| `bubblewrap` installed | Not met |
| `git` ≥ 2.42 | Met: 2.47.3 |
| The data directory on its own volume if possible | Possible: `/srv` is a volume of its own with 792 GiB free |
| An agent CLI inside a tested version range | Not found (2.5) |
| A dedicated system user under systemd | systemd 257 is there; the user is not created |

What follows from the facts:

- **`/var` is full.** With 64 MiB free, installing a package may fail, and the journal and anything else that defaults to `/var/lib` or `/var/log` has no room. Whatever filled it is outside the factory. The factory's data directory and its logs cannot live there.
- **The data directory goes on `/srv`.** It is the only volume with room for mirrors, workspaces and build output.
- **Nothing in SPEC 13.3 has to change on this evidence, and nothing confirms it either.** The user-namespace test passed without any setup, which is the precondition for bubblewrap **(inference: bubblewrap itself has not run on the server)**. The setup step of DEV-6, which lifts Ubuntu's AppArmor restriction on the hosted runners, has no counterpart here because the setting does not exist.
- **The server and the development VM are the same distribution.** DEV-3 fixes the VM at the latest Debian stable, which is Debian 13 today. Their kernels and architectures differ: the VM runs on the Mac's architecture.
- **Memory is the tight resource.** 14.9 GiB and under 1 GiB of swap have to hold up to four concurrent runs (8.3), each a cargo build and an agent CLI, and from M2 a microVM per stage execution. The limit values of M1 (#19) should be weighed against it. The 18 logical CPUs and the disk on `/srv` are not the constraint.
- **The machine is not the factory's alone.** It mounts a network filesystem and has a `docker` group. Membership of `docker` is equivalent to root, and so is `sudo`; the factory's system user needs neither.
- **The reason D-16 gives for the server being the operator's has gone.** The Mac now reaches the server from anywhere, so a session on the Mac could open a connection to it. The rule that every step on the server is the operator's stays, as a rule and not as a consequence of the network.

## 4. Not observed

- **bubblewrap on the server.** Not installed. The five tests of P-16 and the check that the sandbox holds only `lo` wait for it. Probe P-6 (#12) waits for it too, and for Claude Code.
- **`/dev/kvm` opened by anyone.** Needs a member of group `kvm`. Whether a Firecracker or Cloud Hypervisor guest boots is M2 work (D-5).
- **Which AppArmor profiles are loaded, and the kernel log.** Both need root.
- **Whether the kernel accepts a seccomp filter.** The script printed the seccomp state of its own shell, which says nothing about it.
- **Claude Code and the Rust toolchain** (2.5).
- **How `sshd` authenticates**, and what else listens on the machine.
- **An authenticated call** to GitHub or to the Anthropic API from the server.
- **Anything as the factory's user.** It does not exist. Every result above is for the operator's own account.

The script on `prototype/server-facts` was changed after the run so that the next one needs no redaction and finds tools outside the `PATH`: commit [`734fec1`](https://github.com/sapientpants/software-factory/tree/734fec1068b59999e5ce27a7c7aceb79a7a4bb28/crates/factory-sandbox/prototype/server-facts). That version ran in a container on the Mac only.
