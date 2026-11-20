# The server's facts

A throwaway script and its output for #7, "Record the server's facts". Not for merging. The record is `docs/research/14-server-facts.md`; this directory is the evidence behind it.

`server-facts.sh` runs on the server as an ordinary user, without sudo, and changes nothing there. It reads system state, starts a few short-lived bubblewrap sandboxes, opens `/dev/kvm`, and makes HTTPS requests to the hosts the factory needs. Its only write is a temporary directory, which it removes.

```sh
ssh <server> 'bash -s' < server-facts.sh | tee results/server-<date>.txt
```

## Results

| File | Run |
|---|---|
| `results/server-2026-11-20.txt` | The operator's run of 2026-11-20 at 08:30 UTC, over SSH from the Mac, as the operator's own account. The script at commit `82693bd` |
| `results/server-2026-11-20-second.txt` | The same, at 08:53 UTC, after the operator installed `bubblewrap` and Claude Code. The script at commit `734fec1`, which is the one in this directory |

What is not as the script printed it, in the first file:

- **Four values are replaced by `<redacted>`:** the account's name, the host name, and the source and the mount point of a network filesystem. The repository is public and the operator chose to name nothing that identifies the server.
- **What `ssh` printed before the script started** (the address, the host key) is left out.
- **The text was pasted from a terminal**, so tabs arrived as spaces.

The second file needed no redaction. It is the script's output as the session captured it, with trailing spaces dropped.

The script here is not the one that made the first run. The one that did is at commit `82693bd` of this branch. Since then:

- It prints no host name, no account name and no network mount, so nothing has to be redacted by hand.
- It adds `~/.cargo/bin` and `~/.local/bin` to its `PATH` before looking for tools.
- It says how to run the `/dev/kvm` tests as a member of group `kvm`.
- After the second run: it gives bubblewrap an allow-everything seccomp filter and prints the seccomp state inside the sandbox. Run in a container on the Mac only.

The changed script first ran in a Debian 13 container on the Mac, where `/dev/kvm` is absent and user namespaces are denied, and then made the second run on the server.

## What the first run does not show

- **bubblewrap was not installed,** so the four tests that use it did not run. `unshare` ran.
- **The account is not in group `kvm`,** so `/dev/kvm` could not be opened and the two ioctls were not tried.
- **"not installed" means "not on the `PATH`".** `ssh <server> 'bash -s'` starts a shell that is neither a login shell nor interactive, so a tool under `~/.cargo/bin` or `~/.local/bin` is reported as not installed. The package query settles it for `bubblewrap`; nothing settles it for `claude`, `rustup`, `rustc` and `cargo`.
- **The `Seccomp` lines** say that the script's own shell ran without a filter. They do not say whether the kernel supports one.

## What the second run does not show

- **Anything as the factory's user.** The account is the operator's own again.
- **`/dev/kvm` opened.** The account is still not in group `kvm`.
- **A seccomp filter.** The bubblewrap tests pass no `--seccomp` argument.
- **The factory's sandbox.** The tests are the stand-in commands of P-16.
- **A Claude Code session.** The script only asked the CLI for its version.
