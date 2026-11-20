#!/usr/bin/env bash
# Collects the server facts that issue #7 asks for (SPEC 4.4).
#
# Run it on the server as an ordinary user, without sudo:
#
#     bash server-facts.sh | tee server-facts.txt
#
# It changes nothing on the machine. It reads system state, starts a few short-lived
# bubblewrap sandboxes, opens /dev/kvm, and makes HTTPS requests to the hosts the factory
# needs. Its only write is a temporary directory, which it removes. It prints the host name
# but no IP address.
set -u
export LC_ALL=C

have() { command -v "$1" >/dev/null 2>&1; }

section() { printf '\n## %s\n\n' "$1"; }

# show <command...>: the command, its output, and its exit status when not 0.
show() {
  printf '$ %s\n' "$*"
  if ! have "$1"; then
    printf '  (%s: not installed)\n' "$1"
    return
  fi
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  [ "$rc" -eq 0 ] || printf '  (exit %s)\n' "$rc"
}

# value <file>: the content of a one-line file such as a sysctl.
value() {
  if [ -r "$1" ]; then
    printf '%s = %s\n' "$1" "$(tr '\n' ' ' <"$1")"
  else
    printf '%s: absent\n' "$1"
  fi
}

# check <name> <command...>: one result line, with the last output lines on failure.
check() {
  local name=$1 out rc
  shift
  out=$("$@" 2>&1)
  rc=$?
  if [ "$rc" -eq 0 ]; then
    printf '%-8s ok\n' "$name"
  else
    printf '%-8s fail (exit %s)\n' "$name" "$rc"
    printf '%s\n' "$out" | tail -n 5 | sed 's/^/    /'
  fi
}

printf '# Server facts for issue #7\n\n'
printf 'Collected %s as uid %s (%s).\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(id -u)" \
  "$([ "$(id -u)" -eq 0 ] && echo 'root: run it again as an ordinary user' || echo unprivileged)"
show id

section 'Distribution, kernel, hardware'
if [ -r /etc/os-release ]; then
  grep -E '^(PRETTY_NAME|ID|VERSION_ID|VERSION_CODENAME)=' /etc/os-release
else
  echo '/etc/os-release: absent'
fi
show uname -srm
show uname -v
show nproc
printf '$ lscpu (selected)\n'
if have lscpu; then
  lscpu | grep -E '^(Architecture|CPU\(s\)|Model name|Thread\(s\) per core|Core\(s\) per socket|Socket\(s\)|Virtualization|Hypervisor vendor)' | sed 's/^/  /'
else
  echo '  (lscpu: not installed)'
fi
grep -E '^(MemTotal|MemAvailable|SwapTotal):' /proc/meminfo
show df -hT -x tmpfs -x devtmpfs -x squashfs -x overlay -x efivarfs
show lsblk -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINT
printf '$ systemctl --version (first line)\n'
if have systemctl; then systemctl --version | head -n 1 | sed 's/^/  /'; else echo '  (systemctl: not installed)'; fi
show systemd-detect-virt
show stat -fc %T /sys/fs/cgroup

section '/dev/kvm'
show ls -l /dev/kvm
show getent group kvm
printf 'CPU flags vmx or svm on %s of %s logical CPUs (x86 only)\n' \
  "$(grep -c -E '^flags.*\b(vmx|svm)\b' /proc/cpuinfo)" "$(grep -c '^processor' /proc/cpuinfo)"
printf '$ lsmod | grep kvm\n'
if have lsmod; then lsmod | grep -E '^kvm' | sed 's/^/  /'; else echo '  (lsmod: not installed)'; fi
if [ ! -e /dev/kvm ]; then
  echo '/dev/kvm is absent: opening it was not tried'
else
  check open sh -c 'exec 3<>/dev/kvm'
  if have python3; then
    # KVM_GET_API_VERSION is _IO(0xAE, 0x00) and KVM_CREATE_VM is _IO(0xAE, 0x01).
    # The VM is an empty file descriptor that goes away when python exits.
    check ioctls python3 -c '
import fcntl, os
fd = os.open("/dev/kvm", os.O_RDWR)
assert fcntl.ioctl(fd, 0xAE00, 0) == 12, "KVM_GET_API_VERSION is not 12"
fcntl.ioctl(fd, 0xAE01, 0)'
    echo '  (ioctls: KVM_GET_API_VERSION returning 12, then KVM_CREATE_VM)'
  else
    echo 'python3 is not installed: the KVM ioctls were not tried'
  fi
fi

section 'User namespaces and their restrictions'
value /proc/sys/kernel/unprivileged_userns_clone
value /proc/sys/user/max_user_namespaces
value /proc/sys/kernel/apparmor_restrict_unprivileged_userns
value /proc/sys/kernel/apparmor_restrict_unprivileged_unconfined
value /sys/kernel/security/lsm
value /sys/module/apparmor/parameters/enabled
printf '$ ls /etc/apparmor.d | grep -i -E "bwrap|userns"\n'
if [ -d /etc/apparmor.d ]; then
  find /etc/apparmor.d -maxdepth 1 \( -iname '*bwrap*' -o -iname '*userns*' \) | sed 's/^/  /'
else
  echo '  (/etc/apparmor.d: absent)'
fi
have getenforce && show getenforce
grep -E '^Seccomp' /proc/self/status

section 'Installed tools'
for tool in git bwrap curl claude rustup rustc cargo; do
  if have "$tool"; then
    printf '%-7s %s (%s)\n' "$tool" "$("$tool" --version 2>&1 | head -n 1)" "$(command -v "$tool")"
  else
    printf '%-7s not installed\n' "$tool"
  fi
done
have bwrap && show ls -l "$(command -v bwrap)"
if have dpkg-query; then
  # shellcheck disable=SC2016
  show dpkg-query -W -f '${Package} ${Version}\n' git bubblewrap
elif have rpm; then
  show rpm -q git bubblewrap
fi

section 'bubblewrap as this user (the tests of probe P-16)'
if have bwrap; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  basic=(--unshare-all --die-with-parent --ro-bind / / --proc /proc --dev /dev --tmpfs /tmp)
  check basic bwrap "${basic[@]}" -- true
  check caps bwrap "${basic[@]}" --uid 0 --gid 0 --cap-add ALL -- mount -t tmpfs tmpfs /mnt
  check nested bwrap "${basic[@]}" -- bwrap "${basic[@]}" -- true
  cp "$(command -v bwrap)" "$tmp/bwrap"
  check copy "$tmp/bwrap" "${basic[@]}" -- true
  # shellcheck disable=SC2016
  printf 'Network interfaces inside the basic sandbox (expected: lo alone): %s\n' \
    "$(bwrap "${basic[@]}" -- sed -n '3,$s/:.*//p' /proc/net/dev 2>&1 | tr -s ' \n' ' ')"
else
  echo 'bwrap is not installed: basic, caps, nested and copy were not run'
fi
check unshare unshare --user --map-root-user --mount -- mount -t tmpfs tmpfs /mnt
printf '$ dmesg | grep apparmor="DENIED" | tail -n 5\n'
if dmesg >/dev/null 2>&1; then
  dmesg | grep 'apparmor="DENIED"' | tail -n 5 | sed 's/^/  /'
else
  echo '  (the kernel log is not readable without root)'
fi

section 'How the server is reached'
show hostname
show hostname -f
if [ -n "${SSH_CONNECTION:-}" ]; then echo 'This run is over SSH: yes'; else echo 'This run is over SSH: no'; fi
show systemctl is-active ssh.service sshd.service ssh.socket
printf '$ ss -tln | grep :22\n'
if have ss; then ss -tln | grep -E ':22[[:space:]]' | sed 's/^/  /'; else echo '  (ss: not installed)'; fi

section 'What the server reaches'
for var in http_proxy https_proxy HTTP_PROXY HTTPS_PROXY no_proxy NO_PROXY; do
  [ -n "${!var:-}" ] && printf 'A proxy variable is set: %s\n' "$var"
done
echo 'An HTTP status of any kind means the host answered; 000 means it did not.'
if have curl; then
  for url in https://github.com https://api.github.com https://api.anthropic.com \
    https://index.crates.io/config.json https://static.crates.io; do
    printf '%-36s %s\n' "$url" \
      "$(curl -sS -o /dev/null -w '%{http_code} in %{time_total}s' --max-time 15 "$url" 2>&1 | tr '\n' ' ')"
  done
else
  echo 'curl is not installed: the HTTPS requests were not made'
fi
if have git; then
  check ls-remote git ls-remote https://github.com/sapientpants/sf-scratch.git HEAD
else
  echo 'git is not installed: ls-remote was not run'
fi
