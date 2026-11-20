# The server's facts

A throwaway script and its output for #7, "Record the server's facts". Not for merging. The record is `docs/research/14-server-facts.md`; this directory is the evidence behind it.

`server-facts.sh` runs on the server as an ordinary user, without sudo, and changes nothing there. It reads system state, starts a few short-lived bubblewrap sandboxes, opens `/dev/kvm`, and makes HTTPS requests to the hosts the factory needs. Its only write is a temporary directory, which it removes.

```sh
ssh <server> 'bash -s' < server-facts.sh | tee results/server-<date>.txt
```

## Results

| File | Run |
|---|---|
| `results/server-2026-11-20.txt` | The operator's run of 2026-11-20, over SSH from the Mac, as the operator's own account |

What is not as the script printed it:

- **Four values are replaced by `<redacted>`:** the account's name, the host name, and the source and the mount point of a network filesystem. The repository is public and the operator chose to name nothing that identifies the server.
- **What `ssh` printed before the script started** (the address, the host key) is left out.
- **The text was pasted from a terminal**, so tabs arrived as spaces.

## What the run of 2026-11-20 does not show

- **bubblewrap was not installed,** so the four tests that use it did not run. `unshare` ran.
- **The account is not in group `kvm`,** so `/dev/kvm` could not be opened and the two ioctls were not tried.
- **"not installed" means "not on the `PATH`".** `ssh <server> 'bash -s'` starts a shell that is neither a login shell nor interactive, so a tool under `~/.cargo/bin` or `~/.local/bin` is reported as not installed. The package query settles it for `bubblewrap`; nothing settles it for `claude`, `rustup`, `rustc` and `cargo`.
- **The `Seccomp` lines** say that the script's own shell ran without a filter. They do not say whether the kernel supports one.
