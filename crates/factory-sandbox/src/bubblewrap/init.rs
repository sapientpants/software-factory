//! The first process of a sandbox at level `process`, `sf __sandbox-init`
//! (SEC-10a): the half of the bubblewrap backend that runs inside the
//! namespace.
//!
//! bubblewrap starts it with `--as-pid-1`, so it is PID 1 of the sandbox. It
//! binds `127.0.0.1`, port [`PORT`], then starts the command through
//! `factory-proc` as the leader of a process group of its own, on the stdio
//! it was itself started with (AG-11). It passes each connection on to the
//! proxy's socket at [`PROXY`], passes a SIGINT and a SIGTERM on to the
//! command's group, reaps every process left to it, and exits with the
//! command's status once the command ends. The kernel then ends whatever is
//! left of the namespace.
//!
//! The protocol is `<program> [<arg>...]` after the subcommand.

use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, ExitStatus};
use std::time::Duration;

use factory_proc::Command;
use factory_proc::supervise::{FAILED, exit_code};
use nix::sys::signal::{Signal, killpg};
use nix::sys::wait::{Id, WaitPidFlag, waitid, waitpid};
use nix::unistd::Pid;
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::signal::unix::{SignalKind, signal};
use tokio::task::JoinSet;

/// The name of the hidden subcommand of `sf`.
pub const SUBCOMMAND: &str = "__sandbox-init";

/// The port of the loopback interface that the first process forwards to
/// the proxy, one number for every instance (SEC-10a).
pub const PORT: u16 = crate::PROXY_PORT;

/// Where the sandbox shows the `sf` binary, which is the first process
/// (SEC-6, SEC-10a).
pub const BINARY: &str = "/sf/sf";

/// Where the sandbox shows the proxy's socket (SEC-10a, SEC-10b).
pub const PROXY: &str = "/sf/proxy.sock";

/// How long the first process waits after an accept that failed
/// *(tunable)*.
const ACCEPT_PAUSE: Duration = Duration::from_millis(50);

/// The first process's arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Init {
    program: OsString,
    args: Vec<OsString>,
}

impl Init {
    /// Reads the arguments that follow the subcommand.
    ///
    /// # Errors
    ///
    /// A message where they name no program.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let program = args.next().ok_or("expected <program> [<arg>...]")?;
        Ok(Self {
            program,
            args: args.collect(),
        })
    }

    /// Runs the first process, forwarding to the socket at [`PROXY`]. It
    /// returns the command's exit code, or 128 and the number of the signal
    /// that ended the command, and [`FAILED`] where the command did not
    /// start.
    pub fn run(self) -> ExitCode {
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("cannot start the runtime: {error}"))
            .and_then(|runtime| runtime.block_on(self.first(Path::new(PROXY))));
        match outcome {
            Ok(status) => exit_code(status),
            Err(message) => {
                eprintln!("sf {SUBCOMMAND}: {message}");
                ExitCode::from(FAILED)
            }
        }
    }

    async fn first(self, proxy: &Path) -> Result<ExitStatus, String> {
        // As PID 1 of a namespace this process gets no signal from outside
        // it for which it has no handler, but SIGKILL: each is installed
        // before anything else, so that none is lost (SEC-10a).
        let handler = |kind: SignalKind| {
            signal(kind).map_err(|error| format!("cannot handle a signal: {error}"))
        };
        let mut interrupt = handler(SignalKind::interrupt())?;
        let mut terminate = handler(SignalKind::terminate())?;
        let mut child = handler(SignalKind::child())?;
        // Bound before the command exists, so that no command finds the
        // proxy missing.
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, PORT)))
            .await
            .map_err(|error| format!("cannot bind 127.0.0.1:{PORT}: {error}"))?;
        let mut command = Command::new(&self.program);
        command.args(&self.args).inherit_stdio();
        for (key, value) in std::env::vars_os() {
            command.env(key, value);
        }
        let mut process = command
            .spawn(tokio::io::sink())
            .await
            .map_err(|error| error.to_string())?;
        let pid = |id: u32| {
            i32::try_from(id)
                .map(Pid::from_raw)
                .map_err(|_| format!("the command's pid {id} is out of range"))
        };
        let (leader, group) = (pid(process.id())?, pid(process.group())?);
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                status = process.wait() => {
                    return status.map_err(|error| error.to_string());
                }
                accepted = listener.accept() => {
                    // A connection that fails is the worker's: the next is
                    // accepted all the same, after a pause where the accept
                    // itself failed, as it does with no descriptor left, so
                    // that PID 1 does not spin.
                    match accepted {
                        Ok((stream, _)) => {
                            connections.spawn(forward(stream, proxy.to_owned()));
                        }
                        Err(_) => tokio::time::sleep(ACCEPT_PAUSE).await,
                    }
                }
                // An empty set answers at once, and the branch then waits
                // for the next turn of the loop.
                Some(_) = connections.join_next() => {}
                Some(()) = interrupt.recv() => {
                    let _ = killpg(group, Signal::SIGINT);
                }
                Some(()) = terminate.recv() => {
                    let _ = killpg(group, Signal::SIGTERM);
                }
                Some(()) = child.recv() => reap(leader),
            }
        }
    }
}

/// Copies the bytes of `stream` both ways to a connection of its own to the
/// socket at `proxy`, until both directions have ended. It reads nothing of
/// them: the proxy decides.
async fn forward(mut stream: TcpStream, proxy: PathBuf) {
    if let Ok(mut upstream) = UnixStream::connect(&proxy).await {
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
    }
}

/// The flags that look at a process that has ended, if there is one,
/// without reaping it. A union of constants, which the mutation step leaves
/// as it is: their bits do not overlap, so `^` would change nothing.
const PEEK: WaitPidFlag = WaitPidFlag::WEXITED
    .union(WaitPidFlag::WNOHANG)
    .union(WaitPidFlag::WNOWAIT);

/// Reaps every process that has ended and was left to this one, but the
/// command, whose status `factory-proc` waits for. A zombie is looked at
/// first and reaped after, so that the command's is never taken.
fn reap(command: Pid) {
    while let Some(pid) = waitid(Id::All, PEEK).ok().and_then(|status| status.pid()) {
        if pid == command {
            // The command has ended, and this process ends after it.
            return;
        }
        let _ = waitpid(pid, Some(WaitPidFlag::WNOHANG));
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::UnixListener;

    use super::*;

    /// Runs `test` on `runtime`, and fails it after 10 s, so that a
    /// forwarding that hangs fails the mutation step and does not time out
    /// there.
    fn within(runtime: &tokio::runtime::Runtime, test: impl Future<Output = ()>) {
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), test)
                .await
                .expect("the test took more than 10 s");
        });
    }

    #[test]
    fn the_arguments_are_a_program_and_its_own() {
        let words = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();

        assert_eq!(
            Init::parse(words(&["/bin/sh", "-c", "true", "--"])),
            Ok(Init {
                program: "/bin/sh".into(),
                args: words(&["-c", "true", "--"]),
            })
        );
        assert!(Init::parse(Vec::new()).is_err());
    }

    #[test]
    fn a_connection_is_carried_both_ways_to_the_socket() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        within(&runtime, async {
            let dir = std::env::temp_dir().join(format!("sf-init-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir(&dir).unwrap();
            let socket = dir.join("proxy.sock");
            let proxy = UnixListener::bind(&socket).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();

            let mut tasks = JoinSet::new();
            tasks.spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                forward(stream, socket).await;
            });
            let mut worker = TcpStream::connect(address).await.unwrap();
            let (mut upstream, _) = proxy.accept().await.unwrap();
            worker.write_all(b"CONNECT").await.unwrap();
            worker.shutdown().await.unwrap();
            let mut heard = Vec::new();
            upstream.read_to_end(&mut heard).await.unwrap();
            // One direction has ended, and the other still carries bytes.
            upstream.write_all(b"200").await.unwrap();
            upstream.shutdown().await.unwrap();
            let mut answer = Vec::new();
            worker.read_to_end(&mut answer).await.unwrap();
            tasks.join_next().await.unwrap().unwrap();
            std::fs::remove_dir_all(&dir).unwrap();

            assert_eq!(heard, b"CONNECT");
            assert_eq!(answer, b"200");
        });
    }

    #[test]
    fn a_connection_where_no_socket_listens_is_closed() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        within(&runtime, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let mut tasks = JoinSet::new();
            tasks.spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                forward(stream, "/sf-no-such-socket".into()).await;
            });
            let mut worker = TcpStream::connect(address).await.unwrap();
            // The read ends, at the end of the stream or at a reset.
            let mut answer = Vec::new();
            let _ = worker.read_to_end(&mut answer).await;
            tasks.join_next().await.unwrap().unwrap();

            assert!(answer.is_empty());
        });
    }
}
