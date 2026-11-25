#!/usr/bin/env bash
# PROTOTYPE for issue #85 (probe P-36, part 3). Never merged.
#
# Which directory does the agent CLI take for its socket? One `claude -p` per case, with
# no token: the CLI sets its socket up before it finds that it is not logged in. A case
# is the mode and owner of the directory that TMPDIR names, or of the one above it.
#
#     bash socket-dir-probe.sh
#
# Run it in the guest, as an ordinary user with `sudo` without a password, which three
# cases need to change an owner. It works under ~/sf-p36-sock-* and removes it, and it
# removes /tmp/cc-socks-<uid> first, so that what the CLI makes there can be told apart.
# It prints uids and no account name. Guest facts that the other scripts do not print
# come first.
set -u
export LC_ALL=C

uid=$(id -u)
claude=$(readlink -f "$HOME/.local/bin/claude")
base=$(mktemp -d "$HOME/sf-p36-sock-XXXXXX")
trap 'sudo -n rm -rf "$base"' EXIT
clean() { sed "s|$base|<base>|g; s|$HOME|~|g"; }
owner() { case $(stat -c %u "$1") in "$uid") echo self ;; 0) echo root ;; *) echo another ;; esac; }

echo "## Guest facts"
echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "/etc/debian_version: $(cat /etc/debian_version)"
echo "umask: $(umask)"
echo "modes: ~ $(stat -c %a "$HOME"), ~/.cache $(stat -c %a "$HOME/.cache"), /tmp $(stat -c %a /tmp), /home $(stat -c %a /home)"
echo "claude: $("$claude" --version 2>&1)"
echo "kernel log, lines of kvm:"
sudo -n dmesg | grep 'kvm \[' | sed 's/^/    /'

echo
echo "## Cases"
rm -rf "/tmp/cc-socks-$uid"
echo "/tmp/cc-socks-$uid before the cases: $([ -e "/tmp/cc-socks-$uid" ] && echo present || echo absent)"

# run <name>: TMPDIR is $base/<name>/parent/tmpdir, prepared by the caller.
run() {
  local name=$1 dir=$base/$1/parent/tmpdir
  mkdir -p "$base/$name/home/.claude" "$base/$name/work"
  (cd "$base/$name/work" && env -i PATH=/usr/bin:/bin HOME="$base/$name/home" \
    CLAUDE_CONFIG_DIR="$base/$name/home/.claude" TMPDIR="$dir" LANG=C.UTF-8 \
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 DISABLE_AUTOUPDATER=1 \
    "$claude" -p --setting-sources user --strict-mcp-config --output-format stream-json --verbose \
    --debug-file "$base/$name/debug.log" hi >"$base/$name/events.jsonl" 2>/dev/null </dev/null)
  echo
  echo "RESULT $name: the directory above TMPDIR $(stat -c %a "$base/$name/parent") $(owner "$base/$name/parent"), TMPDIR $(stat -c %a "$dir") $(owner "$dir")"
  grep -o '\[uds-messaging\] \(sockets directory.*\|Listening.*\|Failed.*\)' "$base/$name/debug.log" | clean | sed 's/^/    /'
  echo "    messaging_socket_path of system/init: $(grep -o '"messaging_socket_path":"[^"]*"' "$base/$name/events.jsonl" | head -n 1 | cut -d'"' -f4 | clean)"
}

# prepare <name> <mode of the directory above> <mode of TMPDIR>
prepare() {
  mkdir -p "$base/$1/parent/tmpdir"
  chmod 0700 "$base/$1"
  chmod "$2" "$base/$1/parent"
  chmod "$3" "$base/$1/parent/tmpdir"
}

prepare private 0700 0700; run private
prepare group-writable 0700 0770; run group-writable
prepare other-writable 0700 0707; run other-writable
prepare sticky 0700 1777; run sticky
prepare above-group-writable 0775 0700; run above-group-writable
prepare above-sticky 1777 0700; run above-sticky
prepare root-sticky 0755 1777; sudo -n chown root "$base/root-sticky/parent/tmpdir"; run root-sticky
prepare another-sticky 0755 1777; sudo -n chown nobody "$base/another-sticky/parent/tmpdir"; run another-sticky
prepare above-another 0755 0700; sudo -n chown nobody "$base/above-another/parent"; run above-another

echo
echo "/tmp/cc-socks-$uid after the cases: $(stat -c '%a' "/tmp/cc-socks-$uid" 2>/dev/null || echo absent) $([ -d "/tmp/cc-socks-$uid" ] && owner "/tmp/cc-socks-$uid"), holding $(find "/tmp/cc-socks-$uid" -mindepth 1 2>/dev/null | wc -l) entries"
