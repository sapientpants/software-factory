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
