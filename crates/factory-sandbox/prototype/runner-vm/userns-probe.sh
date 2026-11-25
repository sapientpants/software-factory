#!/usr/bin/env bash
# PROTOTYPE for issue #85 (probe P-36, SPEC 13.3 SEC-6b, 19.5 DEV-3 and DEV-6). Never merged.
#
# Does `bwrap --disable-userns` do on this machine what it did on the server (P-6)? The
# same script runs on a GitHub-hosted runner and in the Lima guest, so both are asked
# with the same arguments. It also repeats the five tests of P-16 and the seccomp and
# /dev/kvm checks of P-24, whose arguments it copies from server-facts.sh.
#
#     bash userns-probe.sh            # the facts, the tests, and the tests again in a
#                                     # network namespace that holds only `lo` (DEV-6)
#     bash userns-probe.sh tests X    # the tests alone, with X as the state in each line
#     bash userns-probe.sh kvm        # the /dev/kvm checks alone, for `sudo -g kvm`
#
# Run it as an ordinary user. The network namespace needs `sudo` without a password; where
# there is none that part is skipped and the script says so. It changes nothing that
# outlives it: the namespace and a temporary directory are removed at the end. Every
# result is a line that starts with RESULT, and the script exits 0 whatever they say.
set -u
export LC_ALL=C

NETNS=sf-p36
SELF=$(readlink -f "$0")

have() { command -v "$1" >/dev/null 2>&1; }

STATE=facts
r() { echo "RESULT $STATE $*"; }

# One line of at most two lines of output, for the end of a RESULT line.
tail2() { tail -n 2 | tr '\n' ' ' | sed 's/ *$//'; }

# t <name> <command...>: the command is expected to succeed. Returns its exit status.
t() {
  local name=$1 out rc
  shift
  out=$(timeout 60 "$@" 2>&1)
  rc=$?
  if [ "$rc" -eq 0 ]; then r "$name=ok"; else r "$name=fail($rc) :: $(echo "$out" | tail2)"; fi
  return "$rc"
}

# refused <name> <command...>: the command is expected to fail, because the sandbox
# around it lets nothing create a user namespace.
refused() {
  local name=$1 out rc
  shift
  out=$(timeout 60 "$@" 2>&1)
  rc=$?
  if [ "$rc" -eq 0 ]; then r "$name=NOT-REFUSED"; else r "$name=refused($rc) :: $(echo "$out" | tail2)"; fi
}

value() {
  if [ -r "$1" ]; then r "$1=$(tr '\n' ' ' <"$1" | sed 's/ *$//')"; else r "$1=absent"; fi
}

facts() {
  STATE=facts
  r "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  r "uid=$(id -u) groups-of-interest=$(id -Gn | tr ' ' '\n' | grep -x -E 'kvm|sudo|wheel|docker' | tr '\n' ' ')"
  r "os=$(. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME")"
  r "kernel=$(uname -r) $(uname -m)"
  r "virt=$(systemd-detect-virt 2>&1)"
  r "vendor=$(cat /sys/class/dmi/id/sys_vendor 2>/dev/null || echo unreadable)"
  r "cpus=$(nproc) mem-kib=$(sed -n 's/^MemTotal: *\([0-9]*\).*/\1/p' /proc/meminfo)"
  if have bwrap; then
    r "bwrap=$(bwrap --version 2>&1) at $(command -v bwrap) mode $(stat -c %A "$(command -v bwrap)")"
    # shellcheck disable=SC2016
    have dpkg-query && r "package=$(dpkg-query -W -f '${Package} ${Version}' bubblewrap 2>&1)"
  else
    r "bwrap=not installed"
  fi
  value /proc/sys/kernel/unprivileged_userns_clone
  value /proc/sys/user/max_user_namespaces
  value /proc/sys/kernel/apparmor_restrict_unprivileged_userns
  value /sys/kernel/security/lsm
  r "apparmor-files=$(find /etc/apparmor.d -maxdepth 1 \( -iname '*bwrap*' -o -iname '*userns*' \) 2>/dev/null | tr '\n' ' ')"
  r "sudo-n=$(sudo -n true 2>/dev/null && echo yes || echo no)"
}

kvm() {
  STATE=kvm
  if [ ! -e /dev/kvm ]; then
    r "device=absent"
    return
  fi
  r "device=$(ls -l /dev/kvm 2>&1)"
  r "group=$(getent group kvm | sed 's/:[^:]*$/:<members>/') in-group=$(id -Gn | tr ' ' '\n' | grep -c -x kvm)"
  t open sh -c 'exec 3<>/dev/kvm'
  # KVM_GET_API_VERSION is _IO(0xAE, 0x00), KVM_CREATE_VM _IO(0xAE, 0x01),
  # KVM_CHECK_EXTENSION _IO(0xAE, 0x03) and KVM_CREATE_VCPU _IO(0xAE, 0x41). On arm64 the
  # argument of KVM_CREATE_VM is the size of the guest's physical address space, and 0
  # stands for 40 bits, which a host with a smaller one refuses. KVM_CAP_ARM_VM_IPA_SIZE
  # (165) is the largest size the host takes. No guest is booted: the VM and the vCPU are
  # file descriptors that go away when python exits.
  timeout 60 python3 - <<'EOF' 2>&1 | while read -r line; do r "$line"; done
import fcntl, os, platform
try:
    fd = os.open("/dev/kvm", os.O_RDWR)
except OSError as error:
    print(f"ioctls=not tried: {error}")
    raise SystemExit
def call(name, target, request, arg):
    try:
        value = fcntl.ioctl(target, request, arg)
        print(f"{name}=ok({value})")
        return value
    except OSError as error:
        print(f"{name}=fail :: {error}")
        return None
call("KVM_GET_API_VERSION", fd, 0xAE00, 0)
vm = call("KVM_CREATE_VM(0)", fd, 0xAE01, 0)
if platform.machine() == "aarch64":
    ipa = call("KVM_CHECK_EXTENSION(KVM_CAP_ARM_VM_IPA_SIZE)", fd, 0xAE03, 165)
    if vm is None and ipa:
        vm = call(f"KVM_CREATE_VM({ipa})", fd, 0xAE01, ipa)
if vm is not None:
    call("KVM_CREATE_VCPU(0)", vm, 0xAE41, 0)
EOF
}

tests() {
  STATE=$1
  if ! have bwrap; then
    r "bwrap=not installed: no test was run"
    return
  fi
  local tmp
  tmp=$(mktemp -d)
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" EXIT

  r "links=$(sed -n '3,$s/:.*//p' /proc/net/dev | tr -d ' ' | tr '\n' ' ')"

  # The five tests of P-16, with the arguments of server-facts.sh (P-24).
  local basic=(--unshare-all --die-with-parent --ro-bind / / --proc /proc --dev /dev --tmpfs /tmp)
  t basic bwrap "${basic[@]}" -- true
  t caps bwrap "${basic[@]}" --uid 0 --gid 0 --cap-add ALL -- mount -t tmpfs tmpfs /mnt
  t nested bwrap "${basic[@]}" -- bwrap "${basic[@]}" -- true
  cp "$(command -v bwrap)" "$tmp/bwrap"
  t copy "$tmp/bwrap" "${basic[@]}" -- true
  t unshare unshare --user --map-root-user --mount -- mount -t tmpfs tmpfs /mnt
  # shellcheck disable=SC2016
  r "links-in-basic=$(bwrap "${basic[@]}" -- sed -n '3,$s/:.*//p' /proc/net/dev 2>&1 | tr -s ' \n' ' ')"

  # The seccomp check of P-24: one BPF_RET instruction that returns SECCOMP_RET_ALLOW.
  python3 -c 'import sys, struct; sys.stdout.buffer.write(struct.pack("HBBI", 0x06, 0, 0, 0x7fff0000))' >"$tmp/allow.bpf"
  r "seccomp=$(bwrap "${basic[@]}" --seccomp 9 -- grep Seccomp /proc/self/status 9<"$tmp/allow.bpf" 2>&1 | tr -s '\t\n' ' ')"

  # The case of SEC-6b. The outer sandbox takes the namespaces of level `process` (13.3)
  # one by one, so that --unshare-user is explicit, as in the stand-in of P-6.
  local nu=(--unshare-user --unshare-pid --unshare-ipc --unshare-net --unshare-uts --unshare-cgroup-try
    --disable-userns --die-with-parent --ro-bind / / --proc /proc --dev /dev --tmpfs /tmp)
  # Whether --unshare-all, which asks for the user namespace with --unshare-user-try,
  # is enough for the option.
  t nouserns-unshare-all bwrap "${basic[@]}" --disable-userns -- true
  if ! t nouserns-starts bwrap "${nu[@]}" -- true; then
    r "nouserns-others=not run: the outer sandbox does not start"
    return
  fi
  r "nouserns-limit=$(bwrap "${nu[@]}" -- cat /proc/sys/user/max_user_namespaces 2>&1 | tail2)"
  r "nouserns-links=$(bwrap "${nu[@]}" -- sed -n '3,$s/:.*//p' /proc/net/dev 2>&1 | tr -s ' \n' ' ')"
  refused nouserns-nested bwrap "${nu[@]}" -- bwrap "${basic[@]}" -- true
  refused nouserns-unshare bwrap "${nu[@]}" -- unshare --user --map-root-user true
  # Not run by P-6: the same refusal for a process that holds every capability inside
  # the sandbox and first tries to raise the limit. A worker is given no capability.
  refused nouserns-caps bwrap "${nu[@]}" --uid 0 --gid 0 --cap-add ALL -- \
    sh -c 'echo 64 >/proc/sys/user/max_user_namespaces; unshare --user --map-root-user true'
  # A filter and the option together, as the factory's sandbox will have them.
  r "nouserns-seccomp=$(bwrap "${nu[@]}" --seccomp 9 -- grep Seccomp /proc/self/status 9<"$tmp/allow.bpf" 2>&1 | tr -s '\t\n' ' ')"
  # The control: the outer sandbox without the option lets the same inner one start.
  local ctl=("${nu[@]/--disable-userns/--new-session}")
  t control-nested bwrap "${ctl[@]}" -- bwrap "${basic[@]}" -- true
}

if [ "${1:-}" = tests ]; then
  tests "${2:?state}"
  exit 0
fi
if [ "${1:-}" = kvm ]; then
  kvm
  exit 0
fi

facts
kvm
tests host

STATE=netns
if sudo -n true 2>/dev/null && have setpriv && have ip; then
  sudo ip netns del "$NETNS" 2>/dev/null
  sudo ip netns add "$NETNS"
  sudo ip netns exec "$NETNS" ip link set lo up
  # setpriv and not `sudo -u`: sudo looks the host name up, which fails without DNS (P-16).
  sudo ip netns exec "$NETNS" setpriv --reuid="$(id -u)" --regid="$(id -g)" --init-groups -- \
    bash "$SELF" tests netns
  sudo ip netns del "$NETNS"
else
  r "skipped=no sudo without a password, or no setpriv or ip"
fi

STATE=dmesg
if sudo -n true 2>/dev/null; then
  sudo dmesg 2>/dev/null | grep -i 'apparmor="DENIED"' | tail -n 4 | while read -r line; do r "$line"; done
fi
exit 0
