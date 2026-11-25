#!/usr/bin/env bash
# PROTOTYPE for issue #85 (probe P-36). Never merged.
#
# The step that needs the operator's Claude Code login: live sessions of the agent CLI
# in the Lima guest `sf-p36`. Run it on the Mac, from anywhere:
#
#     bash crates/factory-sandbox/prototype/runner-vm/run-guest-sessions.sh
#
# It reads the token that `claude setup-token` made from ~/.config/sf-probe/oauth-token
# and gives it to each script on standard input. The token is written to no file in the
# guest: the scripts pass it to the sessions in the environment and print it nowhere.
#
# 1. The script of P-6, unchanged, at the commit its record names: fifteen sessions under
#    the stand-ins of that probe. About 0.01 API-equivalent dollars on the server.
# 2. cli_paths_probe.py: seven sessions, five of them with one subagent.
#
# Both run on the `haiku` alias. The output lands in results/ beside this script.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
token=${SF_PROBE_TOKEN_FILE:-$HOME/.config/sf-probe/oauth-token}
guest=sf-p36
day=$(date -u +%Y-%m-%d)

[ -r "$token" ] || { echo "No token at $token: run \`claude setup-token\` and write its output there, mode 0600." >&2; exit 2; }
limactl list --format '{{.Name}} {{.Status}}' | grep -q -x "$guest Running" ||
  { echo "The guest $guest is not running: limactl start --name=$guest --tty=false $here/lima.yaml" >&2; exit 2; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git -C "$here" show d23e6b72e805cc92ed668dde11b7f858def2a352:crates/factory-agent/prototype/inner-sandbox/inner_sandbox_probe.py \
  >"$tmp/inner_sandbox_probe.py"
limactl copy "$tmp/inner_sandbox_probe.py" "$here/cli_paths_probe.py" "$guest:/tmp/" 2>/dev/null

mkdir -p "$here/results"
echo "1 of 2: the script of P-6 (fifteen sessions) ..."
limactl shell "$guest" python3 /tmp/inner_sandbox_probe.py --token-stdin <"$token" \
  >"$here/results/guest-p6-$day.txt" 2>&1 || echo "  it exited with $?"
tail -n 20 "$here/results/guest-p6-$day.txt"

echo "2 of 2: cli_paths_probe.py (seven sessions) ..."
limactl shell "$guest" python3 /tmp/cli_paths_probe.py --token-stdin <"$token" \
  >"$here/results/guest-cli-$day.txt" 2>&1 || echo "  it exited with $?"
tail -n 14 "$here/results/guest-cli-$day.txt"

echo "Done: $here/results/guest-p6-$day.txt and guest-cli-$day.txt"
