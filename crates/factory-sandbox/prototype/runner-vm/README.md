# PROTOTYPE: the sandbox's means on a hosted runner and in the development VM

Throwaway code for issue #85 and probe P-36 (SPEC 13.3 SEC-6 and SEC-6b, 19.5 DEV-3 and DEV-6). Not for merging. The answer is in `docs/research/probes.md` on `main`.

Three questions, each about Linux:

1. Does `bwrap --disable-userns` do on the hosted runners of DEV-5, with the bubblewrap 0.9.0 that apt installs there and inside the network namespace of DEV-6, what it did on the server with 0.12.0 (P-6)?
2. In a Lima guest as DEV-3 describes the development VM: do the five tests of P-16 and the case of 1 pass, does a session of the agent CLI complete under the stand-in of P-6, and does `/dev/kvm` answer `KVM_CREATE_VM`?
3. Where does the agent CLI read and write on Linux outside what SEC-6 gives it: its temporary files, its socket, and the managed settings?

| File | |
|---|---|
| `userns-probe.sh` | The five tests of P-16, the seccomp and `/dev/kvm` checks of P-24 and the `--disable-userns` cases, on the machine and again in a network namespace that holds only `lo`. The same script runs on a runner and in the guest |
| `probe-p36.yml` | The workflow that runs it on `ubuntu-24.04` and `ubuntu-24.04-arm`. It lies on the branch `probe/p-36` of `sapientpants/sf-scratch`, at `.github/workflows/`, beside a copy of the script |
| `lima.yaml` | The guest: Debian 13, `vmType: vz`, nested virtualisation, nothing of the Mac mounted |
| `cli_paths_probe.py` | Seven sessions of the agent CLI in the guest, one under `strace` and the others under the `nouserns` stand-in of P-6, with `/tmp` writable or read-only and with a managed-settings file planted or hidden |
| `run-guest-sessions.sh` | The operator's step: gives the token to the script of P-6 and to `cli_paths_probe.py` in the guest |

```sh
limactl start --name=sf-p36 --tty=false lima.yaml
limactl copy userns-probe.sh sf-p36:/tmp/
limactl shell sf-p36 bash /tmp/userns-probe.sh
limactl shell sf-p36 bash -c 'sudo setpriv --reuid=$(id -u) --regid=$(id -g) --groups=kvm bash /tmp/userns-probe.sh kvm'
limactl shell sf-p36 bash -c 'curl -fsSL https://claude.ai/install.sh -o /tmp/claude-install.sh && bash /tmp/claude-install.sh 2.1.326'
limactl copy cli_paths_probe.py sf-p36:/tmp/
limactl shell sf-p36 python3 /tmp/cli_paths_probe.py --no-login
bash run-guest-sessions.sh                      # needs ~/.config/sf-probe/oauth-token
limactl delete --force sf-p36
```

## Results

| File | Run |
|---|---|
| `results/runner-<label>-run-<id>-attempt-<n>.txt` | The `RESULT` lines of one job of the workflow, with the image lines of its setup. Run 45709070248 is the script at the branch's first commit, `3824064`; run 45709406404 is the script as it is here, at `86c268f` |
| `results/guest-2026-11-25.txt` | `userns-probe.sh` in the guest, and its `/dev/kvm` checks again with `kvm` as a supplementary group |
| `results/guest-cli-no-login-2026-11-25.txt` | `cli_paths_probe.py --no-login`: every session ends with "Not logged in", after the CLI has read its settings and opened its socket |

The guest's account is named after the Mac's. The script prints `<user>` for it and `~` for its home directory.

Lima 2.3.0 from Homebrew on macOS 27.0.1, on an Apple M4 Max. Claude Code 2.1.326 in the guest, the version on the Mac that day.
