# 13 - What GitHub-hosted runners can sandbox

Research date: 2026-11-18. Resolves issue #10 (probe P-16, SPEC 23.2): can GitHub-hosted Linux runners, for a public repository on a personal GitHub Free account, run the sandbox tests of SPEC 19.4, that is bubblewrap with user namespaces (level `process`, 13.3) and a KVM microVM (level `hardened`, 13.3)?

Method: primary sources only, read on the research date. Each claim carries a source number from the Sources section and the sentence it rests on. `actions/runner-images` was read at commit `fbb26cb` (2026-11-18). No workflow was run, so nothing here is an observation of a live runner. Statements marked **(inference)** are my reading of the sources, not something a source says. Statements marked **(user report)** come from an issue comment by someone who is not a maintainer.

## 1. Answer

| | x64 standard runner (`ubuntu-24.04`, `ubuntu-26.04`) | arm64 standard runner (`ubuntu-24.04-arm`, `ubuntu-26.04-arm`) | `ubuntu-slim` |
|---|---|---|---|
| bubblewrap with user namespaces | Yes, with setup. The setup differs between 24.04 and 26.04. | Same as x64 (same image scripts and Ubuntu policy; no arm64-specific source) | No. Unprivileged container. |
| `/dev/kvm` for a microVM | Documented as available, with a udev rule for permissions. Not guaranteed: nested VMs are "not officially supported", and since 2026-10-28 a job may land on AWS, where no source says whether `/dev/kvm` exists. | No, per a maintainer. One user report disagrees for `ubuntu-24.04-arm`. | No. |

Confidence: bubblewrap on 24.04 high, on 26.04 medium (the mechanism is documented, the exact step is untested). `/dev/kvm` present on an Azure-hosted x64 runner: medium-high. A Firecracker or Cloud Hypervisor guest booting there, on every job: low-medium, because no primary source covers it.

## 2. Findings

### 2.1 Which images the labels resolve to

1. For public repositories the standard Linux runner is a 4-CPU, 16 GB x64 VM with the labels `ubuntu-latest`, `ubuntu-24.04`, `ubuntu-22.04`, `ubuntu-26.04`, or a 4-CPU arm64 VM with the labels `ubuntu-24.04-arm`, `ubuntu-22.04-arm`, `ubuntu-26.04-arm` [1]. There is no `-latest` arm64 label in either GitHub's table or the runner-images table [1][3]. "Use of the standard GitHub-hosted runners is free and unlimited on public repositories." [1]
2. `ubuntu-latest` is in the last day of a migration. GitHub's table and the runner-images README both still map it to Ubuntu 24.04 ("`ubuntu-latest` or `ubuntu-24.04`") [1][3]. The announcement says: "The `ubuntu-latest` label is ready to become the default version for Ubuntu 26.04." and "This change will be rolled out over a period of several weeks beginning October 19, 2026. We plan to complete the migration by November 19, 2026." [4] The issue is still open and no maintainer has posted that the rollout is complete. So today a job on `ubuntu-latest` can get either image; from 2026-11-19 it should be 26.04. The sources disagree only in the sense that the tables lag the announcement.
3. Current image contents at `fbb26cb` [6]:

   | Label | OS | Kernel | Image version |
   |---|---|---|---|
   | `ubuntu-24.04` | 24.04.5 LTS | 7.0.0-1017-azure | 20261111.370.1 |
   | `ubuntu-26.04` | 26.04.1 LTS | 7.0.0-1019-azure | 20261111.196.1 |
   | `ubuntu-22.04` | 22.04.5 LTS | 6.8.0-1070-azure | 20261111.360.2 |
   | `ubuntu-24.04-arm` | 24.04.5 LTS | 7.0.0-1017-azure | 20261111.182.1 |
   | `ubuntu-26.04-arm` | 26.04.1 LTS | 7.0.0-1019-azure | 20261111.184.1 |
   | `ubuntu-22.04-arm` | 22.04.5 LTS | 6.8.0-1070-azure | 20261111.190.1 |

4. Ubuntu 22.04 is on its way out: "Deprecation will begin on September 17th, 2026 and the images will be fully unsupported by April 17th,, 2027", with brownouts in March and April 2027 [5]. Do not build on it.
5. `ubuntu-slim` is not a VM: "`ubuntu-slim` runners execute Actions workflows in Ubuntu Linux, inside a container rather than a full VM instance." and "The container for `ubuntu-slim` runners runs in unprivileged mode. This means that some operations requiring elevated privileges—such as mounting file systems, using Docker-in-Docker, or accessing low-level kernel features—are not supported." [1]
6. Larger runners are not an option for this repository: "Larger runners are available for organizations and enterprises on GitHub Team and GitHub Enterprise Cloud plans." [1]

### 2.2 What a workflow step may change

7. Steps have root: "The Linux and macOS virtual machines both run using passwordless `sudo`." [1] Every command in section 3 is a `sudo` command in a `run:` step.
8. The image maintainers expect workflows to change AppArmor themselves. On the request to disable AppArmor in the images: "After careful security review and due to security reasons (risks) we do not plan to disable it. Also these runners are ephemeral VMs and workflows already have root, so can disable AppArmor themselves." [17] In a 26.04 AppArmor bug a maintainer gave a workflow step that edits a profile and runs `sudo apparmor_parser -r` as the interim fix [38].

### 2.3 Unprivileged user namespaces for bubblewrap

9. bubblewrap needs them. Its README: "Bubblewrap uses these to build the sandbox, allowing any user to use the tool." and "Historically, bubblewrap also supported a setuid mode for systems where unprivileged user namespaces were not supported. However, this has been removed." [18] Ubuntu packages bubblewrap 0.6.1 (22.04), 0.9.0 (24.04) and 0.11.1 (26.04) [36].
10. Ubuntu 24.04 and later restrict them through AppArmor. 24.04 release notes: "In combination with the apparmor package, the Ubuntu kernel now restricts the use of unprivileged user namespaces. This affects all programs on the system that are unprivileged and unconfined. A default AppArmor profile is provided that allows the use of user namespaces for unprivileged and unconfined applications but will deny the subsequent use of any capabilities within the user namespace." [8] Ubuntu's feature matrix lists "AppArmor unprivileged user namespace restrictions" as "–" for 22.04 LTS and "kernel & userspace" for 24.04 LTS, 25.10 and 26.04 LTS [10]. The rule: "Applications requiring unprivileged namespaces must be explicitly allowed by their AppArmor profile." [11] bubblewrap needs capabilities inside its namespace to mount and to set up the network namespace, so the default profile breaks it **(inference from [8] and [9])**.
11. The runner images do not lift the restriction. `configure-environment.sh` sets `vm.max_map_count`, `fs.inotify.*` and `vm.mmap_rnd_bits` and nothing about user namespaces or AppArmor [7]. A pull request that added the sysctl to the image was closed unmerged: "Workaround for that issue already provided. Hence closing this PR" [16].
12. **Ubuntu 24.04 images: one sysctl.** Ubuntu documents it: "Disable this restriction on the entire system for one boot by executing `echo 0 | sudo tee /proc/sys/kernel/apparmor_restrict_unprivileged_userns`. This setting is lost on reboot." [8] A runner-images contributor gives the same for a job: "A workaround for job run is to disable `kernel.apparmor_restrict_unprivileged_userns`, e.g.: `- run: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`" [15]. bubblewrap's own CI runs on `ubuntu-latest` with a step "Enable user namespaces" that runs `sudo ./ci/enable-userns.sh`, whose body is `echo "kernel.apparmor_restrict_unprivileged_userns = 0" > /etc/sysctl.d/99-userns.conf` followed by `sysctl --system` [19]. The 24.04 `apparmor` package ships `/etc/apparmor.d/unprivileged_userns` and no bwrap profile in `/etc/apparmor.d`; the bwrap profile exists there only as `/usr/share/apparmor/extra-profiles/bwrap-userns-restrict` in the separate `apparmor-profiles` package [14].
13. Inside issue [15] two contributors disagree. An earlier answer says to set `kernel.unprivileged_userns_clone=1` and that "This might not be an option if you're using a CI runner where you don't have root access." Both points are contradicted by [8], [1] and the later comment quoted above. Use `kernel.apparmor_restrict_unprivileged_userns`.
14. **Ubuntu 26.04 images: a bwrap profile is loaded, and the sysctl alone may not be enough.** Since 25.04: "AppArmor now comes with a bwrap profile (bwrap-userns-restrict) that allows it to create user namespaces and set up sandboxing, before transitioning to a tighter profile that denies capabilities for the processes running inside the bwrap sandbox." and "this profile still restricts unprivileged userns creation and capability usage even when bwrap (and its sandboxed application) are run as a privileged user" [9]. The 26.04 `apparmor` package ships `/etc/apparmor.d/bwrap-userns-restrict` [14]. The upstream profile is attached to the path `/usr/bin/bwrap` (`profile bwrap /usr/bin/bwrap flags=(attach_disconnected,mediate_deleted)`), stacks every child with `unpriv_bwrap`, and `unpriv_bwrap` contains `audit deny capability,` under the comment "The unpriv_bwrap profile is used to strip capabilities within the userns" [13]. The 26.04 release notes say nothing about bwrap or user namespaces [39].
15. Consequences on 26.04 **(inference from [9], [13], [14])**: a stock `/usr/bin/bwrap` should start without any setup, because the profile grants it `userns`; processes inside it hold no capabilities, whatever the sysctl says, because the profile is attached by path and not by the sysctl; a `bwrap` binary at any other path is unconfined and falls under the general restriction of finding 10, so it needs the sysctl of finding 12.
16. One **(user report)**, posted today by an account with no maintainer association, matches that reading: tests that run commands inside `bwrap` "passed on 24.04 and failed on 26.04. A plain `bwrap --unshare-all --ro-bind / / ...` still works. Only commands inside the sandbox fail.", with `apparmor="DENIED" operation="capable" class="cap" profile="unpriv_bwrap" capability=21 capname="sys_admin"`, and "Setting `kernel.apparmor_restrict_unprivileged_userns=0` didn't help." Their fix was to unload the profile with `apparmor_parser -R` [20]. No maintainer has confirmed it. Ubuntu's security team documents the same operation for other profiles: `sudo apparmor_parser -R /etc/apparmor.d/busybox` [12].
17. Whether `bubblewrap` is preinstalled on the images is not stated. It is not in any Ubuntu toolset file and not in the image READMEs [6][37]. Install it explicitly.

### 2.4 `/dev/kvm`

18. GitHub documents KVM on standard Linux runners only through the Android emulator. Reference page: "GitHub-hosted Linux runners support hardware acceleration for Android SDK tools, which makes running Android tests much faster and consumes fewer minutes." [1] The docs never use the string "kvm" for hosted runners; a search of the docs repository finds it only in GitHub Enterprise Server pages.
19. Runner sizes: first "GitHub-hosted larger Linux runners" (2023-02-23) [22], then "Available now, Actions users of our 2-vCPU GitHub-hosted Linux runners will be able to make use of hardware acceleration for Android testing. Previously this feature was only available on runners with 4 or more vCPUs." (2024-04-02) [21]. In the community discussion that the runner-images maintainers point to, the author of those changelog entries wrote on 2024-04-05: "This has now shipped :) All Linux runners are now on a SKU that supports nested virtualization" [24]. The standard public-repository runner (4 CPU, x64) is covered.
20. Documented setup, identical in both changelogs: "To make use of this on Linux, Actions users will need to add the runner user to the KVM user group", followed by the step in section 3 [21][22]. The images contain no KVM configuration of their own (no match for "kvm" under `images/ubuntu` at `fbb26cb`). The `acl` package is in every Ubuntu toolset [37], so Firecracker's alternative `sudo setfacl -m u:${USER}:rw /dev/kvm` [28] is also available, as is running the VMM under `sudo`.
21. GitHub does not support what the `hardened` tests would do: "While nested virtualization is technically possible while using runners, it is not officially supported. Any use of nested VMs is experimental and done at your own risk, we offer no guarantees regarding stability, performance, or compatibility." [2] A maintainer repeated it in August 2026: "nested virtualization isn't officially supported on GitHub-hosted runners in general" [25].
22. **New since 2026-10-28, and not covered by any KVM statement:** "GitHub Actions uses a multi-cloud compute pool to help improve runner availability for eligible Linux jobs. These jobs can run on either Amazon Web Services (AWS) or Microsoft Azure." "Multi-cloud compute is used for standard 2-core and 4-core Linux runners" and "Eligible runners on GitHub Free, GitHub Pro, and GitHub Team automatically use multi-cloud compute." [2][23] This repository's jobs are eligible and cannot opt out. No GitHub source says whether an AWS-hosted runner exposes `/dev/kvm`. AWS's own documentation says nested virtualization exists on virtual instances but only on listed Intel instance types and only when turned on at launch ("When you launch a new instance, you can turn on nested virtualization") [33]. Which instance types GitHub uses, and with which option, is not published. The job log names the cloud in its "Runner Image Provisioner" block ("Region: us-west-2", "Cloud: AWS") [35].

### 2.5 Do Firecracker or Cloud Hypervisor work under nested virtualization there?

23. Firecracker's requirement is only this: "Firecracker requires read/write access to `/dev/kvm` exposed by the KVM module." [28] A nested host is allowed for development: "setting up a development environment requires either a bare-metal machine (with hardware virtualization), or a virtual machine that supports nested virtualization." [29] The Azure section of that page is "`[TODO]`" [29].
24. Firecracker is not validated on this kind of host. Its tested platforms are all EC2 `.metal` instances [31]. Supported host kernels are v5.10, v6.1 and v6.18, with the note "While other versions and other kernel configs might work, they are not periodically validated in our test suite, and using them might result in unexpected behaviour." [30] The runner kernels are 7.0 (24.04, 26.04) and 6.8 (22.04) [6]. Supported architectures are "**x86_64** and **aarch64** Linux" [28].
25. Firecracker's getting-started page and AWS disagree about EC2: "We exclusively use `.metal` instance types, because EC2 only supports KVM on `.metal` instance types." [28] against AWS: "Nested virtualization allows you to run hypervisors such as Hyper-V and KVM inside virtual Amazon EC2 instances." [33] This matters only for finding 22.
26. Cloud Hypervisor: "Cloud Hypervisor is an open source Virtual Machine Monitor (VMM) that runs on top of the KVM hypervisor and the Microsoft Hypervisor (MSHV)." and "For required KVM functionality and adequate performance the minimum supported host kernel version is 6.8." [32] All three runner kernels meet that. Its README does not mention nested virtualization.
27. No primary source reports either VMM booting a guest on a GitHub-hosted runner. Reports about other hypervisors are mixed: a user found that on standard runners KVM through libvirt failed while LXD VMs and Multipass worked [27].

### 2.6 arm64 runners

28. arm64 standard runners are generally available for public repositories: "The standard runners are 4vCPU and available for both Linux and Windows." [34] bubblewrap: no arm64-specific statement exists. The arm64 images are built from the same scripts [7] and the Ubuntu AppArmor policy is not architecture-specific [8][10], so findings 10 to 17 should carry over **(inference)**. A maintainer reproduced a different 26.04 AppArmor bug on both architectures [38].
29. KVM: no. Maintainer, 2026-08-20: "`/dev/kvm` has to be exposed by the underlying host the runner VM runs on, and the ARM64 hosted fleet doesn't surface a nested KVM device — so it never appears in the VM (or in a container on it)". "Today the only path to a KVM-accelerated arm64 guest is a self-hosted ARM64 runner on bare-metal (or an ARM64 host with nested virtualization) that exposes `/dev/kvm`." [25]
30. A **(user report)** eight days earlier disagrees for one image: "On `ubuntu-24.04-arm` runners, `/dev/kvm` is available and enables KVM hardware acceleration for QEMU out of the box. However, on `ubuntu-26.04-arm`, the `/dev/kvm` device node does not exist" [26]. It was closed about a minute after it was opened, with no reply from anyone. The maintainer statement is the better source; the disagreement is unresolved.
31. GitHub's sentence in finding 18 ("GitHub-hosted Linux runners support hardware acceleration") is not limited to x64 [1], which contradicts finding 29. The page has no arm64 Linux limitations section.

## 3. Setup steps

These are step fragments for a job, each from the source named. None was run for this report.

bubblewrap on `ubuntu-24.04` and `ubuntu-24.04-arm` [8][15][19]:

```yaml
- run: sudo apt-get update && sudo apt-get install -y bubblewrap
- run: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
```

bubblewrap on `ubuntu-26.04` and `ubuntu-26.04-arm`. The first command is the documented way to remove a loaded profile [12] applied to the file the package ships [14]; the second is then needed because `bwrap` becomes an unconfined program (finding 15). The combination is **(inference)** and matches one user report [20]:

```yaml
- run: sudo apt-get update && sudo apt-get install -y bubblewrap
- run: |
    sudo apparmor_parser -R /etc/apparmor.d/bwrap-userns-restrict
    sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
```

Leaving the 26.04 profile loaded is also a valid configuration, but then AppArmor, not the factory's sandbox, denies every capability inside the sandbox. A 19.4 test such as "escape through a setuid binary or a mount" could then pass without the sandbox policy being the cause **(inference)**.

KVM permissions on x64, verbatim from GitHub's changelog [21]:

```yaml
- name: Enable KVM group perms
  run: |
    echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules
    sudo udevadm control --reload-rules
    sudo udevadm trigger --name-match=kvm
```

Firecracker's own check afterwards [28]: `[ -r /dev/kvm ] && [ -w /dev/kvm ] && echo "OK" || echo "FAIL"`.

## 4. Where sources are silent or disagree

| Question | State of the sources |
|---|---|
| What `ubuntu-latest` is today | Tables say 24.04 [1][3]; announcement says 26.04 by 2026-11-19 [4]. Mid-rollout. |
| Is `bubblewrap` preinstalled | Silent [6][37]. |
| Exact bwrap setup on 26.04 | Mechanism documented [9][13][14]; the working step rests on inference and one user report [20]. |
| Is the sysctl default 1 on the runner kernels | Not stated for the runner; Ubuntu's default [8] plus no override in the image [7] plus the failure reported in [15]. |
| `/dev/kvm` on AWS-hosted standard runners | Silent [2][23]. |
| Nested virtualization support | "technically possible", "not officially supported" [2]; yet Android acceleration, which uses KVM, is a documented feature [1][21]. |
| `/dev/kvm` on arm64 | Maintainer: never [25]. User report: present on `ubuntu-24.04-arm` [26]. GitHub's docs sentence is unqualified [1]. |
| Firecracker or Cloud Hypervisor on a hosted runner | Silent. Firecracker's kernel policy does not cover kernels 6.8 or 7.0 [30]. |
| KVM on virtual EC2 instances | Firecracker docs: only `.metal` [28]. AWS docs: also listed virtual Intel types when enabled [33]. |
| bubblewrap in `ubuntu-slim` | Not stated beyond "unprivileged mode" [1]. |

## 5. What this means for SPEC 19.4

The Sandbox row of 19.4 says the tests "run on the server if CI runners cannot provide user namespaces and KVM (probe P-16)". The two backends come out differently.

The `process` backend tests can run in CI on every pull request, on x64 hosted runners, with the two-step setup of section 3. Pin the label (`ubuntu-24.04` or `ubuntu-26.04`, not `ubuntu-latest`), because the setup differs between the two and the label is moving this week. Do not use `ubuntu-22.04` (retired 2027-04-17) or `ubuntu-slim`.

The `hardened` backend tests should keep the server as their home. `/dev/kvm` is documented on x64 standard runners, so a CI job can try them, but three things make CI unfit as the only place they run: GitHub offers "no guarantees regarding stability, performance, or compatibility" for nested VMs [2]; a job may be scheduled on AWS, where `/dev/kvm` is undocumented [2][23]; and neither VMM is validated on these hosts or kernels [30][32]. If a CI job is added, it must detect a missing or unusable `/dev/kvm` and report "not run", never "passed", and it must be x64 only. A skipped CI job must not be allowed to stand in for the scheduled server run.

## 6. What only a live workflow run can settle

1. Whether `/dev/kvm` exists on an AWS-hosted standard runner, and how often jobs land there.
2. Whether Firecracker and Cloud Hypervisor boot a guest under nested virtualization on the 7.0 Azure kernel, and how reliably.
3. The exact 26.04 bwrap step: whether unloading `bwrap-userns-restrict` plus the sysctl is necessary and sufficient, and whether the factory's seccomp and namespace set-up works with the profile left loaded.
4. Whether `bubblewrap` is already on the images, and whether `kernel.apparmor_restrict_unprivileged_userns` reads 1 on a fresh runner.
5. Whether `ubuntu-24.04-arm` has `/dev/kvm` (finding 30).

## Sources

1. https://docs.github.com/en/actions/reference/runners/github-hosted-runners
2. https://docs.github.com/en/actions/concepts/runners/github-hosted-runners
3. https://github.com/actions/runner-images/blob/fbb26cb01c3651ea3a0347e54bfa2ef83d740d49/README.md
4. https://github.com/actions/runner-images/issues/14748
5. https://github.com/actions/runner-images/issues/14254
6. https://github.com/actions/runner-images/blob/fbb26cb01c3651ea3a0347e54bfa2ef83d740d49/images/ubuntu/Ubuntu2404-Readme.md and, in the same directory, `Ubuntu2604-Readme.md`, `Ubuntu2204-Readme.md`, `Ubuntu2404-Arm64-Readme.md`, `Ubuntu2604-Arm64-Readme.md`, `Ubuntu2204-Arm64-Readme.md`
7. https://github.com/actions/runner-images/blob/fbb26cb01c3651ea3a0347e54bfa2ef83d740d49/images/ubuntu/scripts/build/configure-environment.sh
8. https://documentation.ubuntu.com/release-notes/24.04/ (section "Unprivileged user namespace restrictions")
9. https://documentation.ubuntu.com/release-notes/25.04/ (section "AppArmor profile for bwrap")
10. https://documentation.ubuntu.com/security/security-features/security-features-overview/
11. https://documentation.ubuntu.com/security/security-features/privilege-restriction/apparmor/
12. https://discourse.ubuntu.com/t/understanding-apparmor-user-namespace-restriction/58007
13. https://gitlab.com/apparmor/apparmor/-/blob/master/profiles/apparmor/profiles/extras/bwrap-userns-restrict
14. https://packages.ubuntu.com/resolute/amd64/apparmor/filelist, https://packages.ubuntu.com/noble-updates/amd64/apparmor/filelist, https://packages.ubuntu.com/noble-updates/all/apparmor-profiles/filelist, https://packages.ubuntu.com/jammy-updates/amd64/apparmor/filelist
15. https://github.com/actions/runner-images/issues/10443 (the sysctl: https://github.com/actions/runner-images/issues/10443#issuecomment-2616616457; the earlier answer: https://github.com/actions/runner-images/issues/10443#issuecomment-2296608244)
16. https://github.com/actions/runner-images/pull/11489
17. https://github.com/actions/runner-images/issues/10015#issuecomment-4183129225
18. https://github.com/containers/bubblewrap/blob/main/README.md
19. https://github.com/containers/bubblewrap/blob/main/.github/workflows/check.yml and https://github.com/containers/bubblewrap/blob/main/ci/enable-userns.sh
20. https://github.com/actions/runner-images/issues/14748#issuecomment-6979085371
21. https://github.blog/changelog/2024-04-02-github-actions-hardware-accelerated-android-virtualization-now-available/
22. https://github.blog/changelog/2023-02-23-hardware-accelerated-android-virtualization-on-actions-windows-and-linux-larger-hosted-runners/
23. https://github.blog/changelog/2026-10-28-github-actions-adds-multi-cloud-compute-to-improve-reliability/
24. https://github.com/orgs/community/discussions/160591#discussioncomment-9018826
25. https://github.com/actions/runner-images/issues/14062#issuecomment-5352403358
26. https://github.com/actions/runner-images/issues/14549
27. https://github.com/actions/runner-images/issues/12933
28. https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md
29. https://github.com/firecracker-microvm/firecracker/blob/main/docs/dev-machine-setup.md
30. https://github.com/firecracker-microvm/firecracker/blob/main/docs/kernel-policy.md
31. https://github.com/firecracker-microvm/firecracker/blob/main/README.md (section "Tested platforms")
32. https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/README.md
33. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/amazon-ec2-nested-virtualization.html
34. https://github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/
35. https://github.com/actions/runner-images/issues/15347
36. https://packages.ubuntu.com/jammy-updates/bubblewrap, https://packages.ubuntu.com/noble-updates/bubblewrap, https://packages.ubuntu.com/resolute/bubblewrap
37. https://github.com/actions/runner-images/tree/fbb26cb01c3651ea3a0347e54bfa2ef83d740d49/images/ubuntu/toolsets
38. https://github.com/actions/runner-images/issues/15182#issuecomment-6493085515
39. https://documentation.ubuntu.com/release-notes/26.04/ and https://documentation.ubuntu.com/release-notes/26.04/changes-since-previous-interim/
