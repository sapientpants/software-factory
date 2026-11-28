//! The egress proxy against the requests of SEC-10b (SPEC 19.4). A listener
//! on the loopback interface stands in for every listed host, so the tests
//! need no network and no sandbox.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use factory_core::engine::{ActivityId, RunId};
use factory_net::proxy::{
    Allowlist, BindError, Denial, Failure, Limits, ListError, Log, Proxy, Reason, Sent,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// The names that the tests list.
const LISTED: [&str; 2] = ["index.crates.io", "api.anthropic.com"];

/// The stage execution that the tests' listeners name.
const ACTIVITY: ActivityId = ActivityId {
    run: RunId(7),
    seq: 3,
};

/// What the proxy logged.
#[derive(Default)]
struct Records {
    denied: Mutex<Vec<Denial>>,
    failed: Mutex<Vec<Failure>>,
}

impl Log for Records {
    fn denied(&self, denial: Denial) {
        self.denied
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(denial);
    }

    fn failed(&self, failure: Failure) {
        self.failed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(failure);
    }
}

/// A proxy that serves a socket of its own until `stop` is cancelled.
struct Harness {
    dir: PathBuf,
    socket: PathBuf,
    records: Arc<Records>,
    /// The names that the proxy asked the connector for.
    connected: Arc<Mutex<Vec<String>>>,
    stop: CancellationToken,
    /// The task that serves the socket.
    serving: JoinSet<()>,
    /// The listener that stands in for every listed host, if there is one.
    _hosts: JoinSet<()>,
}

impl Harness {
    /// A proxy with the limits of SEC-10b, and an echo listener on the
    /// loopback interface in for every listed host.
    async fn start(name: &str) -> Self {
        Self::limited(name, Limits::default()).await
    }

    /// The same with other limits.
    async fn limited(name: &str, limits: Limits) -> Self {
        let echo = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let address = echo.local_addr().unwrap_or_else(|error| panic!("{error}"));
        let mut hosts = JoinSet::new();
        hosts.spawn(async move {
            let mut sessions = JoinSet::new();
            loop {
                let (mut stream, _) = echo
                    .accept()
                    .await
                    .unwrap_or_else(|error| panic!("{error}"));
                sessions.spawn(async move {
                    let (mut read, mut write) = stream.split();
                    let _ = tokio::io::copy(&mut read, &mut write).await;
                    let _ = write.shutdown().await;
                });
            }
        });
        let mut harness = Self::with(
            name,
            move |_| async move { TcpStream::connect(address).await },
            limits,
        )
        .await;
        harness._hosts = hosts;
        harness
    }

    /// A proxy whose connector is `connect`.
    async fn with<F, Fut>(name: &str, connect: F, limits: Limits) -> Self
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = std::io::Result<TcpStream>> + Send + 'static,
    {
        let dir = std::env::temp_dir().join(format!("sf-proxy-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap_or_else(|error| panic!("{error}"));
        let socket = dir.join("proxy.sock");
        let records = Arc::new(Records::default());
        let connected = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&connected);
        let proxy = Proxy::new(
            Allowlist::new(LISTED).unwrap_or_else(|error| panic!("{error}")),
            move |name: String| {
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(name.clone());
                connect(name)
            },
            Arc::clone(&records) as Arc<dyn Log>,
        )
        .activity(ACTIVITY)
        .limits(limits);
        let bound = proxy
            .bind(&socket)
            .unwrap_or_else(|error| panic!("{error}"));
        let stop = CancellationToken::new();
        let until = stop.clone();
        let mut serving = JoinSet::new();
        serving.spawn(async move { bound.serve(until).await });
        Self {
            dir,
            socket,
            records,
            connected,
            stop,
            serving,
            _hosts: JoinSet::new(),
        }
    }

    async fn connect(&self) -> UnixStream {
        UnixStream::connect(&self.socket)
            .await
            .unwrap_or_else(|error| panic!("{error}"))
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Fails a test that takes more than 10 s, so that a proxy that hangs fails
/// the mutation step and does not time out there.
async fn within(test: impl Future<Output = ()>) {
    tokio::time::timeout(Duration::from_secs(10), test)
        .await
        .unwrap_or_else(|_| panic!("the test took more than 10 s"));
}

/// Reads the head of a response, up to its blank line, byte by byte so that
/// nothing after it is taken.
async fn head(stream: &mut UnixStream) -> String {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        if stream
            .read(&mut byte)
            .await
            .unwrap_or_else(|error| panic!("{error}"))
            == 0
        {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8(head).unwrap_or_else(|error| panic!("{error}"))
}

#[tokio::test]
async fn a_connect_to_a_listed_name_carries_bytes_both_ways() {
    within(async {
        for target in ["index.crates.io:443", "Index.Crates.IO:443"] {
            let harness = Harness::start("allowed").await;
            let mut stream = harness.connect().await;
            stream
                .write_all(
                    format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes(),
                )
                .await
                .unwrap();
            let answer = head(&mut stream).await;
            assert!(answer.starts_with("HTTP/1.1 200 "), "{answer}");
            // A tunnel has no body, so the answer gives no length.
            assert!(
                !answer.to_ascii_lowercase().contains("content-length"),
                "{answer}"
            );
            stream.write_all(b"hello through the tunnel").await.unwrap();
            stream.shutdown().await.unwrap();
            let mut echoed = Vec::new();
            stream.read_to_end(&mut echoed).await.unwrap();

            assert_eq!(echoed, b"hello through the tunnel");
            assert_eq!(*harness.connected.lock().unwrap(), ["index.crates.io"]);
            assert!(harness.records.denied.lock().unwrap().is_empty());
        }
    })
    .await;
}

/// Sends `request` and reads the answer to its end.
async fn ask(harness: &Harness, request: &[u8]) -> String {
    let mut stream = harness.connect().await;
    // The write breaks where the proxy closes before it has read all of a
    // head that is too long.
    let _ = stream.write_all(request).await;
    let mut answer = Vec::new();
    // The read ends at the end of the stream, or at a reset where the proxy
    // closed before it read all that was sent.
    let _ = stream.read_to_end(&mut answer).await;
    String::from_utf8_lossy(&answer).into_owned()
}

#[tokio::test]
async fn every_other_request_is_refused_with_its_reason() {
    use factory_net::proxy::Reason::{Host, Method, Port};

    let cases: &[(&str, &str, Reason, &str)] = &[
        // A name that is not listed.
        ("CONNECT", "example.com:443", Host, "host"),
        // A name of which a listed one is only the end.
        ("CONNECT", "evil.index.crates.io:443", Host, "host"),
        ("CONNECT", "evilindex.crates.io:443", Host, "host"),
        // A listed one is only the start, or has a dot more.
        ("CONNECT", "index.crates.io.evil:443", Host, "host"),
        ("CONNECT", "index.crates.io.:443", Host, "host"),
        // An IP address.
        ("CONNECT", "127.0.0.1:443", Host, "host"),
        ("CONNECT", "[::1]:443", Host, "host"),
        // A user before the name, and a target that is no name and port.
        ("CONNECT", "someone@index.crates.io:443", Host, "host"),
        ("CONNECT", "/index.crates.io:443", Host, "host"),
        // Another port, and none.
        ("CONNECT", "index.crates.io:80", Port, "port"),
        ("CONNECT", "index.crates.io", Port, "port"),
        // A plain request, through `HTTP_PROXY`, also to a listed name.
        (
            "GET",
            "http://index.crates.io/config.json",
            Method,
            "method",
        ),
        ("GET", "http://example.com:8080/", Method, "method"),
        ("POST", "/", Method, "method"),
    ];
    within(async {
        for (method, target, reason, name) in cases {
            let harness = Harness::start("refused").await;
            let answer = ask(
                &harness,
                format!(
                    "{method} {target} HTTP/1.1\r\nHost: {target}\r\nProxy-Authorization: Basic eDp5\r\n\r\n"
                )
                .as_bytes(),
            )
            .await;
            let denied = harness.records.denied.lock().unwrap().clone();

            assert!(answer.starts_with("HTTP/1.1 403 "), "{target}: {answer}");
            assert!(answer.ends_with(&format!("\r\n\r\nrefused: {name}\n")), "{target}: {answer}");
            assert_eq!(denied.len(), 1, "{target}");
            assert_eq!(denied[0].reason, *reason, "{target}");
            assert_eq!(denied[0].activity, Some(ACTIVITY), "{target}");
            assert_eq!(
                denied[0].sent,
                Some(Sent {
                    method: (*method).to_owned(),
                    target: (*target).to_owned(),
                }),
            );
            assert!(harness.connected.lock().unwrap().is_empty(), "{target}");
            assert!(harness.records.failed.lock().unwrap().is_empty(), "{target}");
        }
    })
    .await;
}

#[tokio::test]
async fn the_method_and_the_target_are_kept_to_255_bytes() {
    within(async {
        let harness = Harness::start("cut").await;
        let method = "M".repeat(300);
        // Byte 255 lies in the middle of an `é`, which is left out whole.
        let target = format!("//{}", "é".repeat(200));
        let _ = ask(
            &harness,
            format!("{method} {target} HTTP/1.1\r\n\r\n").as_bytes(),
        )
        .await;
        let denied = harness.records.denied.lock().unwrap().clone();

        assert_eq!(denied.len(), 1);
        let sent = denied[0].sent.clone().unwrap();
        assert_eq!(sent.method, "M".repeat(255));
        assert_eq!(sent.target, format!("//{}", "é".repeat(126)));
    })
    .await;
}

#[tokio::test]
async fn a_head_that_hyper_cannot_read_is_malformed() {
    let long = format!(
        "CONNECT index.crates.io:443 HTTP/1.1\r\nX-Pad: {}\r\n\r\n",
        "a".repeat(16 * 1024)
    );
    let cases: &[(&[u8], &str)] = &[
        (long.as_bytes(), "HTTP/1.1 431 "),
        (
            b"\x16\x03\x01\x02\x00\x01\x00\x01\xfc\x03\x03\r\n\r\n",
            "HTTP/1.1 400 ",
        ),
        (b"CONNECT index.crates.io:443 HTTP/9.9\r\n\r\n", "HTTP/1.1 "),
    ];
    within(async {
        for (request, status) in cases {
            let harness = Harness::start("malformed").await;
            let answer = ask(&harness, request).await;
            let denied = harness.records.denied.lock().unwrap().clone();

            assert!(answer.starts_with(status), "{answer}");
            assert!(!answer.starts_with("HTTP/1.1 200 "), "{answer}");
            assert_eq!(denied.len(), 1, "{answer}");
            assert_eq!(denied[0].reason, Reason::Malformed);
            assert_eq!(denied[0].sent, None);
            assert!(harness.connected.lock().unwrap().is_empty());
        }
    })
    .await;
}

#[tokio::test]
async fn a_head_cut_short_is_malformed() {
    within(async {
        let harness = Harness::start("cut-short").await;
        let mut stream = harness.connect().await;
        stream
            .write_all(b"CONNECT index.crates.io:443 HTTP/1.1\r\nHost: index")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer).await;
        drop(stream);
        let mut harness = harness;
        harness.stop.cancel();
        harness.serving.join_next().await.unwrap().unwrap();
        let denied = harness.records.denied.lock().unwrap().clone();

        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].reason, Reason::Malformed);
        assert!(harness.connected.lock().unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn a_head_that_is_too_slow_is_malformed_and_gets_no_answer() {
    within(async {
        let harness = Harness::limited(
            "slow",
            Limits {
                head: Duration::from_millis(200),
                ..Limits::default()
            },
        )
        .await;
        let mut stream = harness.connect().await;
        stream
            .write_all(b"CONNECT index.crates.io:443 HTTP/1.1\r\n")
            .await
            .unwrap();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer).await;
        let denied = harness.records.denied.lock().unwrap().clone();

        assert_eq!(answer, b"");
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].reason, Reason::Malformed);
    })
    .await;
}

#[tokio::test]
async fn a_host_that_does_not_answer_gets_502_and_no_denial() {
    within(async {
        // One that refuses the connection, and one that never accepts it.
        let refused = Harness::with(
            "unreachable",
            |_| async { Err(std::io::Error::from(std::io::ErrorKind::ConnectionRefused)) },
            Limits::default(),
        )
        .await;
        let silent = Harness::with(
            "silent",
            |_| std::future::pending::<std::io::Result<TcpStream>>(),
            Limits {
                connect: Duration::from_millis(200),
                ..Limits::default()
            },
        )
        .await;
        for harness in [refused, silent] {
            let answer = ask(&harness, b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n").await;
            let failed = harness.records.failed.lock().unwrap().clone();

            assert!(answer.starts_with("HTTP/1.1 502 "), "{answer}");
            assert!(harness.records.denied.lock().unwrap().is_empty());
            assert_eq!(failed.len(), 1);
            assert_eq!(failed[0].name, "api.anthropic.com");
            assert_eq!(failed[0].activity, Some(ACTIVITY));
        }
    })
    .await;
}

#[tokio::test]
async fn the_end_of_the_activity_closes_the_tunnels_and_the_socket() {
    within(async {
        let mut harness = Harness::start("ended").await;
        let mut stream = harness.connect().await;
        stream
            .write_all(b"CONNECT index.crates.io:443 HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        assert!(head(&mut stream).await.starts_with("HTTP/1.1 200 "));
        stream.write_all(b"ping").await.unwrap();
        let mut echoed = [0; 4];
        stream.read_exact(&mut echoed).await.unwrap();
        assert_eq!(&echoed, b"ping");

        harness.stop.cancel();
        // The serving task ends, with the tunnel that it carried.
        harness.serving.join_next().await.unwrap().unwrap();
        let mut rest = Vec::new();
        let _ = stream.read_to_end(&mut rest).await;

        assert_eq!(rest, b"");
        assert!(UnixStream::connect(&harness.socket).await.is_err());
        assert!(!harness.socket.exists());
    })
    .await;
}

#[tokio::test]
async fn the_socket_has_mode_0600() {
    use std::os::unix::fs::PermissionsExt as _;

    within(async {
        let harness = Harness::start("mode").await;
        let mode = std::fs::metadata(&harness.socket)
            .unwrap()
            .permissions()
            .mode();

        assert_eq!(mode & 0o777, 0o600);
    })
    .await;
}

#[tokio::test]
async fn a_path_that_does_not_fit_a_unix_socket_is_refused_by_name() {
    within(async {
        let path = std::env::temp_dir().join("s".repeat(200));
        let proxy = Proxy::new(
            Allowlist::new(LISTED).unwrap(),
            factory_net::proxy::resolve,
            Arc::new(Records::default()) as Arc<dyn Log>,
        );

        assert!(matches!(proxy.bind(&path), Err(BindError::TooLong(p)) if p == path));
    })
    .await;
}

#[tokio::test]
async fn a_path_with_a_nul_byte_is_not_called_too_long() {
    within(async {
        let path = std::env::temp_dir().join("sf\0proxy.sock");
        let proxy = Proxy::new(
            Allowlist::new(LISTED).unwrap(),
            factory_net::proxy::resolve,
            Arc::new(Records::default()) as Arc<dyn Log>,
        );

        assert!(matches!(proxy.bind(&path), Err(BindError::Io { .. })));
    })
    .await;
}

#[tokio::test]
async fn a_request_cut_short_after_its_head_leaves_one_record() {
    within(async {
        // The head reached the proxy, which refused it, and the body that it
        // announced never came: no `malformed` record follows the refusal.
        let harness = Harness::start("short").await;
        let mut stream = harness.connect().await;
        stream
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 100\r\n\r\nabc")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer).await;
        drop(stream);
        harness.stop.cancel();
        let mut harness = harness;
        harness.serving.join_next().await.unwrap().unwrap();
        let denied = harness.records.denied.lock().unwrap().clone();

        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].reason, Reason::Method);
    })
    .await;
}

#[test]
fn the_list_holds_names_and_no_address() {
    let list = Allowlist::new(["Index.Crates.IO"]).unwrap();

    assert!(list.allows("index.crates.io"));
    assert!(list.allows("INDEX.CRATES.IO"));
    assert!(!list.allows("crates.io"));
    assert!(matches!(
        Allowlist::new(["127.0.0.1"]),
        Err(ListError::Address(_))
    ));
    assert!(matches!(
        Allowlist::new(["::1"]),
        Err(ListError::Address(_))
    ));
    assert!(matches!(
        Allowlist::new(["[::1]"]),
        Err(ListError::Address(_))
    ));
    assert_eq!(Allowlist::new([""]), Err(ListError::Empty));
    // What the host's resolver also reads as an address.
    for address in ["127.1", "2130706433", "0x7f000001", "10.0.0.010"] {
        assert_eq!(
            Allowlist::new([address]),
            Err(ListError::Address(address.to_owned())),
            "{address}"
        );
    }
    for entry in [
        "index.crates.io:443",
        "index.crates.io.",
        "a..b",
        "*.crates.io",
        "a b",
    ] {
        assert_eq!(
            Allowlist::new([entry]),
            Err(ListError::NotAName(entry.to_owned())),
            "{entry}"
        );
    }
    assert!(Allowlist::new(["localhost", "xn--bcher-kva.example", "a-1.b2.io"]).is_ok());
}
