//! The seccomp filter of the bubblewrap backend (SEC-6e), as data.
//!
//! [`TABLE`] is the table of SEC-6e: every call that the filter refuses,
//! with the condition on its arguments and the error code it gets, and its
//! number on each architecture where it has one. Every other call is
//! allowed. The data is the same on every platform, so that its snapshot
//! test runs on the Mac too; compiling it into the programs that bubblewrap
//! takes is for Linux alone, where `seccompiler` is a dependency (19.2).

use std::fmt;

/// An architecture that the filter is compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// x86-64, whose kernel also takes the calls of the x32 ABI.
    X86_64,
    /// arm64.
    Aarch64,
}

impl Arch {
    /// Both, in the order of the snapshot.
    pub const ALL: [Self; 2] = [Self::X86_64, Self::Aarch64];

    /// The architecture of this build, where it is one of the two.
    pub const THIS: Option<Self> = if cfg!(target_arch = "x86_64") {
        Some(Self::X86_64)
    } else if cfg!(target_arch = "aarch64") {
        Some(Self::Aarch64)
    } else {
        None
    };
}

/// The error code that a refused call returns with (SEC-6e).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Code {
    /// `EPERM`.
    Eperm,
    /// `ENOSYS`, where a program is expected to have another way.
    Enosys,
    /// `EAFNOSUPPORT`, for a family of sockets.
    Eafnosupport,
}

impl Code {
    /// Every code, one program each.
    pub const ALL: [Self; 3] = [Self::Eperm, Self::Enosys, Self::Eafnosupport];

    /// The number of the code, which is the same on both architectures.
    pub const fn errno(self) -> u32 {
        match self {
            Self::Eperm => 1,
            Self::Enosys => 38,
            Self::Eafnosupport => 97,
        }
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Eperm => "EPERM",
            Self::Enosys => "ENOSYS",
            Self::Eafnosupport => "EAFNOSUPPORT",
        }
    }
}

/// When a call of the table is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// Whatever its arguments.
    Always,
    /// Where the argument `arg` holds a bit of `mask`.
    AnyBit {
        /// The index of the argument.
        arg: u8,
        /// The bits.
        mask: u64,
    },
    /// Where the low 32 bits of the argument `arg` are one of `values`.
    Low32In {
        /// The index of the argument.
        arg: u8,
        /// The values.
        values: &'static [u32],
    },
    /// Where the low 32 bits of the argument `arg` are none of `values`.
    Low32NotIn {
        /// The index of the argument.
        arg: u8,
        /// The values.
        values: &'static [u32],
    },
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |f: &mut fmt::Formatter<'_>, values: &[u32]| {
            let values: Vec<String> = values.iter().map(|value| format!("{value:#x}")).collect();
            write!(f, "{{{}}}", values.join(", "))
        };
        match self {
            Self::Always => Ok(()),
            Self::AnyBit { arg, mask } => write!(f, " if arg{arg} & {mask:#x} != 0"),
            Self::Low32In { arg, values } => {
                write!(f, " if arg{arg} as u32 in ")?;
                list(f, values)
            }
            Self::Low32NotIn { arg, values } => {
                write!(f, " if arg{arg} as u32 not in ")?;
                list(f, values)
            }
        }
    }
}

/// A call that the filter refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused {
    /// Its name.
    pub name: &'static str,
    /// Its number on x86-64, where it has one.
    pub x86_64: Option<i64>,
    /// Its number on arm64, where it has one.
    pub aarch64: Option<i64>,
    /// When it is refused.
    pub condition: Condition,
    /// What it returns then.
    pub code: Code,
}

impl Refused {
    /// Its number on `arch`, or none where `arch` does not have it.
    pub fn number(&self, arch: Arch) -> Option<i64> {
        match arch {
            Arch::X86_64 => self.x86_64,
            Arch::Aarch64 => self.aarch64,
        }
    }
}

/// Every flag of `clone` that creates a namespace, the mask of Moby's
/// profile: `CLONE_NEWNS`, `CLONE_NEWCGROUP`, `CLONE_NEWUTS`,
/// `CLONE_NEWIPC`, `CLONE_NEWUSER`, `CLONE_NEWPID` and `CLONE_NEWNET`.
pub const CLONE_NAMESPACES: u64 = 0x7E02_0000;

/// The ioctl requests that put input into a terminal: `TIOCSTI` and
/// `TIOCLINUX`.
pub const TERMINAL_INPUT: &[u32] = &[0x5412, 0x541C];

/// `PER_LINUX`, the one execution domain that `personality` may ask for.
pub const PER_LINUX: &[u32] = &[0];

/// The families of sockets that a worker may open: `AF_UNSPEC`, `AF_UNIX`,
/// `AF_INET`, `AF_INET6` and `AF_NETLINK`.
pub const SOCKET_FAMILIES: &[u32] = &[0, 1, 2, 10, 16];

/// The first call number of the x32 ABI, which arrives under the
/// architecture of x86-64.
pub const X32_FIRST: u32 = 0x4000_0000;

const fn row(
    name: &'static str,
    x86_64: Option<i64>,
    aarch64: Option<i64>,
    condition: Condition,
    code: Code,
) -> Refused {
    Refused {
        name,
        x86_64,
        aarch64,
        condition,
        code,
    }
}

const fn always(name: &'static str, x86_64: i64, aarch64: i64, code: Code) -> Refused {
    row(name, Some(x86_64), Some(aarch64), Condition::Always, code)
}

/// The table of SEC-6e, row by row. The numbers are those of the kernel's
/// tables, which a test compares with those of `libc` for the architecture
/// it runs on.
pub const TABLE: &[Refused] = &[
    // No namespace of a worker's own (SEC-6b).
    always("unshare", 272, 97, Code::Eperm),
    always("setns", 308, 268, Code::Eperm),
    row(
        "clone",
        Some(56),
        Some(220),
        Condition::AnyBit {
            arg: 0,
            mask: CLONE_NAMESPACES,
        },
        Code::Eperm,
    ),
    always("clone3", 435, 435, Code::Enosys),
    // The mount tree and the root of the sandbox. Neither architecture has
    // `umount`, which is a call of the 32-bit ABIs.
    always("mount", 165, 40, Code::Eperm),
    row("umount", None, None, Condition::Always, Code::Eperm),
    always("umount2", 166, 39, Code::Eperm),
    always("pivot_root", 155, 41, Code::Eperm),
    always("chroot", 161, 51, Code::Eperm),
    always("open_tree", 428, 428, Code::Enosys),
    always("move_mount", 429, 429, Code::Enosys),
    always("fsopen", 430, 430, Code::Enosys),
    always("fsconfig", 431, 431, Code::Enosys),
    always("fsmount", 432, 432, Code::Enosys),
    always("fspick", 433, 433, Code::Enosys),
    always("mount_setattr", 442, 442, Code::Enosys),
    // The kernel keyring.
    always("add_key", 248, 217, Code::Eperm),
    always("keyctl", 250, 219, Code::Eperm),
    always("request_key", 249, 218, Code::Eperm),
    // The kernel log, accounting, quotas, and two old calls that arm64
    // does not have.
    always("syslog", 103, 116, Code::Eperm),
    always("acct", 163, 89, Code::Eperm),
    always("quotactl", 179, 60, Code::Eperm),
    row("uselib", Some(134), None, Condition::Always, Code::Eperm),
    row(
        "modify_ldt",
        Some(154),
        None,
        Condition::Always,
        Code::Eperm,
    ),
    // The placement of memory.
    always("move_pages", 279, 239, Code::Eperm),
    always("mbind", 237, 235, Code::Eperm),
    always("get_mempolicy", 239, 236, Code::Eperm),
    always("set_mempolicy", 238, 237, Code::Eperm),
    always("migrate_pages", 256, 238, Code::Eperm),
    // Profiling, tracing and another execution domain.
    always("perf_event_open", 298, 241, Code::Eperm),
    always("ptrace", 101, 117, Code::Eperm),
    row(
        "personality",
        Some(135),
        Some(92),
        Condition::Low32NotIn {
            arg: 0,
            values: PER_LINUX,
        },
        Code::Eperm,
    ),
    // The memory of another process.
    always("process_vm_readv", 310, 270, Code::Eperm),
    always("process_vm_writev", 311, 271, Code::Eperm),
    always("bpf", 321, 280, Code::Eperm),
    always("userfaultfd", 323, 282, Code::Eperm),
    always("io_uring_setup", 425, 425, Code::Enosys),
    always("io_uring_enter", 426, 426, Code::Enosys),
    always("io_uring_register", 427, 427, Code::Enosys),
    // Input put into a terminal.
    row(
        "ioctl",
        Some(16),
        Some(29),
        Condition::Low32In {
            arg: 1,
            values: TERMINAL_INPUT,
        },
        Code::Eperm,
    ),
    row(
        "socket",
        Some(41),
        Some(198),
        Condition::Low32NotIn {
            arg: 0,
            values: SOCKET_FAMILIES,
        },
        Code::Eafnosupport,
    ),
];

/// The filter on `arch` as lines of text, for its snapshot test (19.4):
/// every call it refuses with its condition and its code, then the rows
/// that no call name stands for.
pub fn snapshot(arch: Arch) -> Vec<String> {
    let mut lines: Vec<String> = TABLE
        .iter()
        .filter(|refused| refused.number(arch).is_some())
        .map(|refused| {
            format!(
                "{}{}: {}",
                refused.name,
                refused.condition,
                refused.code.name()
            )
        })
        .collect();
    if arch == Arch::X86_64 {
        lines.push(format!("every call from {X32_FIRST:#x} (x32): ENOSYS"));
    }
    lines.push("every call of another architecture: the process is killed".to_owned());
    lines
}

/// The filter compiled for `arch`: the programs that bubblewrap takes, one
/// for each error code and, on x86-64, one more for the x32 ABI (SEC-6e).
/// Each is classic BPF, 8 bytes to an instruction in the byte order of the
/// host, and each starts by killing the process where the call is of
/// another architecture.
///
/// # Errors
///
/// What `seccompiler` reports, where it cannot compile a program.
#[cfg(target_os = "linux")]
pub fn compile(arch: Arch) -> Result<Vec<Vec<u8>>, String> {
    use std::collections::BTreeMap;

    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};

    let target = match arch {
        Arch::X86_64 => TargetArch::x86_64,
        Arch::Aarch64 => TargetArch::aarch64,
    };
    let mut programs = Vec::new();
    for code in Code::ALL {
        let mut rules = BTreeMap::new();
        for refused in TABLE.iter().filter(|refused| refused.code == code) {
            if let Some(number) = refused.number(arch) {
                rules.insert(number, rules_of(refused.condition)?);
            }
        }
        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(code.errno()),
            target,
        )
        .map_err(|error| error.to_string())?;
        let program = BpfProgram::try_from(filter).map_err(|error| error.to_string())?;
        programs.push(
            program
                .iter()
                .map(|op| Op {
                    code: op.code,
                    jt: op.jt,
                    jf: op.jf,
                    k: op.k,
                })
                .collect::<Vec<_>>(),
        );
    }
    if arch == Arch::X86_64 {
        programs.push(x32().to_vec());
    }
    Ok(programs.iter().map(|program| bytes(program)).collect())
}

/// The rules of `seccompiler` for a condition: the call matches where one
/// rule does, and a rule where each of its conditions does.
#[cfg(target_os = "linux")]
fn rules_of(condition: Condition) -> Result<Vec<seccompiler::SeccompRule>, String> {
    use seccompiler::{SeccompCmpArgLen as Len, SeccompCmpOp as Op, SeccompCondition, SeccompRule};

    let rule = |conditions: Vec<Result<SeccompCondition, seccompiler::BackendError>>| {
        let conditions = conditions
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        SeccompRule::new(conditions).map_err(|error| error.to_string())
    };
    match condition {
        // An empty list matches the call whatever its arguments.
        Condition::Always => Ok(Vec::new()),
        // A filter compares and cannot negate, so a bit of the mask is a
        // rule of its own.
        Condition::AnyBit { arg, mask } => (0..64)
            .map(|bit| 1_u64 << bit)
            .filter(|bit| mask & bit != 0)
            .map(|bit| {
                rule(vec![SeccompCondition::new(
                    arg,
                    Len::Qword,
                    Op::MaskedEq(bit),
                    bit,
                )])
            })
            .collect(),
        // A `Dword` compares the low 32 bits alone.
        Condition::Low32In { arg, values } => values
            .iter()
            .map(|value| {
                rule(vec![SeccompCondition::new(
                    arg,
                    Len::Dword,
                    Op::Eq,
                    u64::from(*value),
                )])
            })
            .collect(),
        Condition::Low32NotIn { arg, values } => Ok(vec![rule(
            values
                .iter()
                .map(|value| SeccompCondition::new(arg, Len::Dword, Op::Ne, u64::from(*value)))
                .collect(),
        )?]),
    }
}

/// One instruction of classic BPF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    /// The operation.
    pub code: u16,
    /// Where a jump goes on true, as a count of instructions to skip.
    pub jt: u8,
    /// Where it goes on false.
    pub jf: u8,
    /// The operand.
    pub k: u32,
}

/// `AUDIT_ARCH_X86_64`.
pub const AUDIT_ARCH_X86_64: u32 = 0xC000_003E;
/// `AUDIT_ARCH_AARCH64`.
pub const AUDIT_ARCH_AARCH64: u32 = 0xC000_00B7;
/// The action that kills the process.
pub const RET_KILL_PROCESS: u32 = 0x8000_0000;
/// The action that returns an error code, in its low 16 bits.
pub const RET_ERRNO: u32 = 0x0005_0000;
/// The action that allows the call.
pub const RET_ALLOW: u32 = 0x7FFF_0000;

/// What the program for the x32 ABI returns for a call of that ABI: the
/// code lies in the low 16 bits, which the action leaves clear.
const X32_REFUSAL: u32 = RET_ERRNO + Code::Enosys.errno();

/// The program for the x32 ABI, which `seccompiler` cannot write: it keys
/// a rule by one exact call number. It kills a call of another
/// architecture, as every program of `seccompiler` does, and refuses with
/// `ENOSYS` every call from [`X32_FIRST`].
pub fn x32() -> [Op; 7] {
    const LD_W_ABS: u16 = 0x20;
    const JEQ_K: u16 = 0x15;
    const JGE_K: u16 = 0x35;
    const RET_K: u16 = 0x06;
    let op = |code, jt, jf, k| Op { code, jt, jf, k };
    [
        // The architecture, at offset 4 of `seccomp_data`.
        op(LD_W_ABS, 0, 0, 4),
        op(JEQ_K, 1, 0, AUDIT_ARCH_X86_64),
        op(RET_K, 0, 0, RET_KILL_PROCESS),
        // The call number, at offset 0.
        op(LD_W_ABS, 0, 0, 0),
        op(JGE_K, 0, 1, X32_FIRST),
        op(RET_K, 0, 0, X32_REFUSAL),
        op(RET_K, 0, 0, RET_ALLOW),
    ]
}

/// A program as bubblewrap reads it: `struct sock_filter` after
/// `struct sock_filter`, in the byte order of the host.
pub fn bytes(program: &[Op]) -> Vec<u8> {
    program
        .iter()
        .flat_map(|op| {
            let mut bytes = Vec::with_capacity(8);
            bytes.extend(op.code.to_ne_bytes());
            bytes.extend([op.jt, op.jf]);
            bytes.extend(op.k.to_ne_bytes());
            bytes
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows that both architectures share, in the order of the table.
    fn shared_head() -> Vec<&'static str> {
        vec![
            "unshare: EPERM",
            "setns: EPERM",
            "clone if arg0 & 0x7e020000 != 0: EPERM",
            "clone3: ENOSYS",
            "mount: EPERM",
            "umount2: EPERM",
            "pivot_root: EPERM",
            "chroot: EPERM",
            "open_tree: ENOSYS",
            "move_mount: ENOSYS",
            "fsopen: ENOSYS",
            "fsconfig: ENOSYS",
            "fsmount: ENOSYS",
            "fspick: ENOSYS",
            "mount_setattr: ENOSYS",
            "add_key: EPERM",
            "keyctl: EPERM",
            "request_key: EPERM",
            "syslog: EPERM",
            "acct: EPERM",
            "quotactl: EPERM",
        ]
    }

    fn shared_tail() -> Vec<&'static str> {
        vec![
            "move_pages: EPERM",
            "mbind: EPERM",
            "get_mempolicy: EPERM",
            "set_mempolicy: EPERM",
            "migrate_pages: EPERM",
            "perf_event_open: EPERM",
            "ptrace: EPERM",
            "personality if arg0 as u32 not in {0x0}: EPERM",
            "process_vm_readv: EPERM",
            "process_vm_writev: EPERM",
            "bpf: EPERM",
            "userfaultfd: EPERM",
            "io_uring_setup: ENOSYS",
            "io_uring_enter: ENOSYS",
            "io_uring_register: ENOSYS",
            "ioctl if arg1 as u32 in {0x5412, 0x541c}: EPERM",
            "socket if arg0 as u32 not in {0x0, 0x1, 0x2, 0xa, 0x10}: EAFNOSUPPORT",
        ]
    }

    #[test]
    fn the_filter_on_x86_64_is_the_table_of_sec_6e() {
        // The snapshot of 19.4: a change to the filter is a change here and
        // to the table of SEC-6e.
        let mut expected = shared_head();
        expected.extend(["uselib: EPERM", "modify_ldt: EPERM"]);
        expected.extend(shared_tail());
        expected.extend([
            "every call from 0x40000000 (x32): ENOSYS",
            "every call of another architecture: the process is killed",
        ]);

        assert_eq!(snapshot(Arch::X86_64), expected);
    }

    #[test]
    fn the_filter_on_arm64_is_the_table_of_sec_6e() {
        let mut expected = shared_head();
        expected.extend(shared_tail());
        expected.push("every call of another architecture: the process is killed");

        assert_eq!(snapshot(Arch::Aarch64), expected);
    }

    #[test]
    fn the_x32_program_kills_another_architecture_and_refuses_x32_calls() {
        let x32 = [bytes(&x32())];
        let getpid = 39;

        assert_eq!(evaluate(&x32, AUDIT_ARCH_X86_64, getpid, [0; 6]), RET_ALLOW);
        assert_eq!(
            evaluate(&x32, AUDIT_ARCH_X86_64, X32_FIRST | getpid, [0; 6]),
            RET_ERRNO | 38
        );
        assert_eq!(
            evaluate(&x32, AUDIT_ARCH_X86_64, u32::MAX, [0; 6]),
            RET_ERRNO | 38
        );
        assert_eq!(evaluate(&x32, I386, 20, [0; 6]), RET_KILL_PROCESS);
    }

    #[test]
    fn this_build_is_of_its_architecture() {
        let expected = match std::env::consts::ARCH {
            "x86_64" => Some(Arch::X86_64),
            "aarch64" => Some(Arch::Aarch64),
            _ => None,
        };

        assert_eq!(Arch::THIS, expected);
    }

    #[test]
    fn a_program_is_its_instructions_eight_bytes_each() {
        let program = bytes(&x32());

        assert_eq!(program.len(), 7 * 8);
        assert_eq!(ops(&program), x32());
    }

    /// `AUDIT_ARCH_I386`, the 32-bit calls of x86-64.
    const I386: u32 = 0x4000_0003;
    /// The instructions of `program`.
    fn ops(program: &[u8]) -> Vec<Op> {
        program
            .as_chunks::<8>()
            .0
            .iter()
            .map(|op| Op {
                code: u16::from_ne_bytes([op[0], op[1]]),
                jt: op[2],
                jf: op[3],
                k: u32::from_ne_bytes([op[4], op[5], op[6], op[7]]),
            })
            .collect()
    }

    /// What one program returns for a call, as the kernel runs classic BPF
    /// over `struct seccomp_data`: the number, the architecture, the
    /// instruction pointer and the six arguments.
    fn run(program: &[Op], nr: u32, arch: u32, args: [u64; 6]) -> u32 {
        let mut data = Vec::with_capacity(64);
        data.extend(nr.to_ne_bytes());
        data.extend(arch.to_ne_bytes());
        data.extend(0_u64.to_ne_bytes());
        for arg in args {
            data.extend(arg.to_ne_bytes());
        }
        let mut a = 0_u32;
        let mut pc = 0_usize;
        loop {
            let op = program[pc];
            pc += 1;
            let skip = |taken: bool| usize::from(if taken { op.jt } else { op.jf });
            match op.code {
                0x20 => {
                    let at = usize::try_from(op.k).unwrap_or(usize::MAX);
                    a = u32::from_ne_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
                }
                0x54 => a &= op.k,
                0x05 => pc += usize::try_from(op.k).unwrap_or(usize::MAX),
                0x15 => pc += skip(a == op.k),
                0x25 => pc += skip(a > op.k),
                0x35 => pc += skip(a >= op.k),
                0x06 => return op.k,
                other => panic!("an operation that the test does not know: {other:#x}"),
            }
        }
    }

    /// What the kernel does with a call under every program: the action of
    /// highest precedence, where killing the process comes first and allowing
    /// the call last.
    fn evaluate(programs: &[Vec<u8>], arch: u32, nr: u32, args: [u64; 6]) -> u32 {
        programs
            .iter()
            .map(|program| run(&ops(program), nr, arch, args))
            .min_by_key(|ret| (ret & 0xFFFF_0000).cast_signed())
            .unwrap_or(RET_ALLOW)
    }

    #[cfg(target_os = "linux")]
    mod compiled {
        use super::*;

        /// `AUDIT_ARCH_ARM`, the 32-bit calls of arm64.
        const ARM: u32 = 0x4000_0028;

        fn compiled(arch: Arch) -> Vec<Vec<u8>> {
            compile(arch).unwrap_or_else(|error| panic!("{arch:?}: {error}"))
        }

        /// Arguments that meet the condition of a row, and arguments that
        /// do not, where it has one.
        fn arguments(condition: Condition) -> ([u64; 6], Option<[u64; 6]>) {
            let at = |arg: u8, value: u64| {
                let mut args = [0; 6];
                args[usize::from(arg)] = value;
                args
            };
            match condition {
                Condition::Always => ([0; 6], None),
                // CLONE_NEWUSER, and CLONE_THREAD with CLONE_SIGHAND.
                Condition::AnyBit { arg, .. } => (at(arg, 0x1000_0000), Some(at(arg, 0x0001_0800))),
                // TIOCSTI with high bits set, which the kernel drops, and
                // TCGETS.
                Condition::Low32In { arg, .. } => {
                    (at(arg, 0xFFFF_FFFF_0000_5412), Some(at(arg, 0x5401)))
                }
                // A value outside the list, and the first value of the list
                // with high bits set.
                Condition::Low32NotIn { arg, values } => (
                    at(arg, 0x26),
                    Some(at(arg, 0x1_0000_0000 | u64::from(values[0]))),
                ),
            }
        }

        #[test]
        fn each_architecture_gets_a_program_for_each_code_and_x86_64_one_for_x32() {
            assert_eq!(compiled(Arch::X86_64).len(), 4);
            assert_eq!(compiled(Arch::Aarch64).len(), 3);
            for program in compiled(Arch::X86_64)
                .iter()
                .chain(&compiled(Arch::Aarch64))
            {
                assert_eq!(program.len() % 8, 0);
            }
        }

        #[test]
        fn every_row_gets_its_code_and_every_other_call_is_allowed() {
            for (arch, audit) in [
                (Arch::X86_64, AUDIT_ARCH_X86_64),
                (Arch::Aarch64, AUDIT_ARCH_AARCH64),
            ] {
                let programs = compiled(arch);
                for refused in TABLE {
                    let Some(number) = refused.number(arch) else {
                        continue;
                    };
                    let number = u32::try_from(number).unwrap_or(u32::MAX);
                    let (met, unmet) = arguments(refused.condition);
                    assert_eq!(
                        evaluate(&programs, audit, number, met),
                        RET_ERRNO | refused.code.errno(),
                        "{arch:?} {}",
                        refused.name
                    );
                    if let Some(unmet) = unmet {
                        assert_eq!(
                            evaluate(&programs, audit, number, unmet),
                            RET_ALLOW,
                            "{arch:?} {} unmet",
                            refused.name
                        );
                    }
                }
                // `getpid`, `read` and `prctl` and `seccomp` (SEC-6e).
                let allowed: &[u32] = match arch {
                    Arch::X86_64 => &[39, 0, 157, 317],
                    Arch::Aarch64 => &[172, 63, 167, 277],
                };
                for number in allowed {
                    assert_eq!(evaluate(&programs, audit, *number, [0; 6]), RET_ALLOW);
                }
            }
        }

        #[test]
        fn a_call_of_another_architecture_kills_the_process() {
            for (arch, other) in [
                (Arch::X86_64, I386),
                (Arch::Aarch64, ARM),
                (Arch::X86_64, AUDIT_ARCH_AARCH64),
            ] {
                let programs = compiled(arch);
                for program in &programs {
                    assert_eq!(
                        run(&ops(program), 20, other, [0; 6]),
                        RET_KILL_PROCESS,
                        "{arch:?}"
                    );
                }
            }
        }

        #[test]
        fn an_x32_call_is_refused_on_x86_64_alone() {
            let x86_64 = compiled(Arch::X86_64);
            let aarch64 = compiled(Arch::Aarch64);

            assert_eq!(
                evaluate(&x86_64, AUDIT_ARCH_X86_64, X32_FIRST | 39, [0; 6]),
                RET_ERRNO | 38
            );
            assert_eq!(
                evaluate(&aarch64, AUDIT_ARCH_AARCH64, X32_FIRST | 172, [0; 6]),
                RET_ALLOW
            );
        }

        #[test]
        fn the_numbers_of_the_table_are_those_of_libc_here() {
            use nix::libc;

            let Some(arch) = Arch::THIS else {
                return;
            };
            #[cfg(target_arch = "x86_64")]
            let only_here: &[(&str, libc::c_long)] = &[
                ("uselib", libc::SYS_uselib),
                ("modify_ldt", libc::SYS_modify_ldt),
            ];
            #[cfg(not(target_arch = "x86_64"))]
            let only_here: &[(&str, libc::c_long)] = &[];
            let shared: &[(&str, libc::c_long)] = &[
                ("unshare", libc::SYS_unshare),
                ("setns", libc::SYS_setns),
                ("clone", libc::SYS_clone),
                ("clone3", libc::SYS_clone3),
                ("mount", libc::SYS_mount),
                ("umount2", libc::SYS_umount2),
                ("pivot_root", libc::SYS_pivot_root),
                ("chroot", libc::SYS_chroot),
                ("open_tree", libc::SYS_open_tree),
                ("move_mount", libc::SYS_move_mount),
                ("fsopen", libc::SYS_fsopen),
                ("fsconfig", libc::SYS_fsconfig),
                ("fsmount", libc::SYS_fsmount),
                ("fspick", libc::SYS_fspick),
                ("mount_setattr", libc::SYS_mount_setattr),
                ("add_key", libc::SYS_add_key),
                ("keyctl", libc::SYS_keyctl),
                ("request_key", libc::SYS_request_key),
                ("syslog", libc::SYS_syslog),
                ("acct", libc::SYS_acct),
                ("quotactl", libc::SYS_quotactl),
                ("move_pages", libc::SYS_move_pages),
                ("mbind", libc::SYS_mbind),
                ("get_mempolicy", libc::SYS_get_mempolicy),
                ("set_mempolicy", libc::SYS_set_mempolicy),
                ("migrate_pages", libc::SYS_migrate_pages),
                ("perf_event_open", libc::SYS_perf_event_open),
                ("ptrace", libc::SYS_ptrace),
                ("personality", libc::SYS_personality),
                ("process_vm_readv", libc::SYS_process_vm_readv),
                ("process_vm_writev", libc::SYS_process_vm_writev),
                ("bpf", libc::SYS_bpf),
                ("userfaultfd", libc::SYS_userfaultfd),
                ("io_uring_setup", libc::SYS_io_uring_setup),
                ("io_uring_enter", libc::SYS_io_uring_enter),
                ("io_uring_register", libc::SYS_io_uring_register),
                ("ioctl", libc::SYS_ioctl),
                ("socket", libc::SYS_socket),
            ];
            let numbers: Vec<(&str, Option<i64>)> = shared
                .iter()
                .chain(only_here)
                .map(|(name, number)| (*name, Some(*number)))
                .collect();
            for refused in TABLE {
                let expected = numbers
                    .iter()
                    .find(|(name, _)| *name == refused.name)
                    .and_then(|(_, number)| *number);
                assert_eq!(refused.number(arch), expected, "{}", refused.name);
            }
        }
    }
}
