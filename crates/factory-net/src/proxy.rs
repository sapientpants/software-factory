//! The egress proxy (SEC-10b): one listener on a Unix socket for each sandbox
//! instance, which tunnels a `CONNECT` to a listed name on port 443 and
//! refuses everything else.
//!
//! The proxy reads one HTTP/1.1 request head from a connection, with `hyper`
//! and HTTP/1 alone (19.2). A head that `hyper` cannot read it answers by
//! itself; the proxy records it as `malformed`. A `CONNECT` whose host is on
//! the [`Allowlist`] and whose port is 443 gets `200`, and the proxy then
//! copies the bytes both ways, unread, until both directions have ended.
//! Every other request is refused with `403` and its [`Reason`], and leaves
//! one [`Denial`] in the [`Log`]. A listed host that cannot be reached gets
//! `502` and is a [`Failure`], no denial.
//!
//! The function that turns a listed name into a connection is a parameter,
//! [`Connect`], so that a test can stand a listener on the loopback interface
//! in for a host. [`resolve`] is the one that the host uses.

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::fmt;
use std::io;
use std::net::IpAddr;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use factory_core::engine::ActivityId;
use hyper::body::Incoming;
use hyper::header::{CONNECTION, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::OnUpgrade;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// The one port that the proxy tunnels to (SEC-10b).
pub const PORT: u16 = 443;

/// The longest request head that the proxy reads, 8 KiB *(tunable)*: the
/// least that `hyper` accepts as its cap (SEC-10b).
pub const MAX_HEAD: usize = 8 * 1024;

/// How much of the method and of the target a [`Denial`] keeps, in bytes
/// *(tunable)* (SEC-10b).
pub const KEPT: usize = 255;

/// The mode of the proxy's socket (SEC-10b).
const MODE: u32 = 0o600;

/// How long the proxy waits after an accept that failed *(tunable)*, so that
/// a listener with no descriptor left does not spin.
const ACCEPT_PAUSE: Duration = Duration::from_millis(50);

/// The times of SEC-10b.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How long a connection has to send its request head, 10 s
    /// *(tunable)*.
    pub head: Duration,
    /// How long a listed host has to accept the connection, 10 s
    /// *(tunable)*.
    pub connect: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            head: Duration::from_secs(10),
            connect: Duration::from_secs(10),
        }
    }
}

/// The names that the proxy tunnels to, each folded to lower case in the
/// letters A to Z (SEC-10b, SEC-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowlist(BTreeSet<String>);

/// An entry that cannot be on the [`Allowlist`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ListError {
    /// An entry that is empty.
    #[error("an entry of the egress list is empty")]
    Empty,
    /// An entry that is an IP address, which is never on the list
    /// (SEC-10b): one that `IpAddr` reads, an IPv6 one in brackets, or one whose last label starts
    /// with a digit, as `127.1` and `2130706433` do, which the host's
    /// resolver also reads as an address.
    #[error("the egress list names hosts, and {0:?} is an IP address")]
    Address(String),
    /// An entry that is no host name: a label that is empty or holds
    /// another character than a letter, a digit or `-`, as a port does.
    #[error("the egress list names hosts, and {0:?} is no host name")]
    NotAName(String),
}

impl Allowlist {
    /// The list of `names`.
    ///
    /// # Errors
    ///
    /// Where an entry is empty, an IP address or no host name.
    pub fn new<I>(names: I) -> Result<Self, ListError>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        names
            .into_iter()
            .map(|name| {
                let name = name.as_ref();
                let labels = || name.split('.');
                let character = |c: char| c.is_ascii_alphanumeric() || c == '-';
                if name.is_empty() {
                    Err(ListError::Empty)
                } else if name.starts_with('[')
                    || name.parse::<IpAddr>().is_ok()
                    || labels()
                        .next_back()
                        .is_some_and(|last| last.starts_with(|c: char| c.is_ascii_digit()))
                {
                    Err(ListError::Address(name.to_owned()))
                } else if labels().any(|label| label.is_empty() || !label.chars().all(character)) {
                    Err(ListError::NotAName(name.to_owned()))
                } else {
                    Ok(name.to_ascii_lowercase())
                }
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// Whether `name`, folded, is on the list. It is compared whole, with no
    /// wildcard and no suffix (SEC-10b).
    #[must_use]
    pub fn allows(&self, name: &str) -> bool {
        self.0.contains(&name.to_ascii_lowercase())
    }
}

/// Why the proxy refused a request (SEC-10b). Where more than one applies,
/// the first of this order is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// A head that cannot be read as HTTP/1.1, or one that is too long or
    /// too slow.
    Malformed,
    /// A request that is no `CONNECT`.
    Method,
    /// A `CONNECT` to a name that is not on the list, or to an IP address.
    Host,
    /// A `CONNECT` to a listed name on a port other than 443.
    Port,
}

impl Reason {
    /// The reason's name, as the denied-request log and the answer give it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::Method => "method",
            Self::Host => "host",
            Self::Port => "port",
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The method and the target of a request as they were sent, each cut at
/// [`KEPT`] bytes. They are a worker's text, and they are data (INV-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// The method.
    pub method: String,
    /// The target.
    pub target: String,
}

/// A record of the denied-request log (SEC-10b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    /// When the proxy refused the request.
    pub time: SystemTime,
    /// The run and the stage execution that the listener names; none for
    /// `sf sandbox exec`, which has no run (CLI-6).
    pub activity: Option<ActivityId>,
    /// What was sent; none for the reason `malformed`.
    pub sent: Option<Sent>,
    /// Why it was refused.
    pub reason: Reason,
}

/// A listed host that the proxy could not reach. It is no denial (SEC-10b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// When the connection failed.
    pub time: SystemTime,
    /// The run and the stage execution that the listener names; none for
    /// `sf sandbox exec`.
    pub activity: Option<ActivityId>,
    /// The listed name, folded.
    pub name: String,
    /// What went wrong.
    pub error: String,
}

/// Where the proxy writes what it refused and what it could not reach.
pub trait Log: Send + Sync + 'static {
    /// Adds a record to the denied-request log.
    fn denied(&self, denial: Denial);

    /// Logs a listed host that could not be reached.
    fn failed(&self, failure: Failure);
}

/// The function that turns a listed name, folded, into a connection to it on
/// port 443 (SEC-10b). A closure of a `String` is one.
pub trait Connect: Send + Sync + 'static {
    /// The connection.
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Connects to `name`.
    fn connect(&self, name: String) -> impl Future<Output = io::Result<Self::Stream>> + Send;
}

impl<F, Fut, S> Connect for F
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = io::Result<S>> + Send,
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = S;

    fn connect(&self, name: String) -> impl Future<Output = io::Result<S>> + Send {
        self(name)
    }
}

/// Resolves `name` with the host's resolver and connects to it on port 443
/// (SEC-10b).
///
/// # Errors
///
/// Where the name does not resolve or no address accepts the connection.
pub async fn resolve(name: String) -> io::Result<TcpStream> {
    TcpStream::connect((name.as_str(), PORT)).await
}

/// A proxy for one sandbox instance, before it has a socket.
pub struct Proxy<C> {
    list: Allowlist,
    connect: C,
    log: Arc<dyn Log>,
    activity: Option<ActivityId>,
    limits: Limits,
}

impl<C: Connect> Proxy<C> {
    /// A proxy that tunnels to the names of `list` through `connect` and
    /// logs to `log`, with the [`Limits`] of SEC-10b and no activity.
    pub fn new(list: Allowlist, connect: C, log: Arc<dyn Log>) -> Self {
        Self {
            list,
            connect,
            log,
            activity: None,
            limits: Limits::default(),
        }
    }

    /// The stage execution that the listener names (SEC-10).
    #[must_use]
    pub fn activity(mut self, activity: ActivityId) -> Self {
        self.activity = Some(activity);
        self
    }

    /// Other times than those of SEC-10b.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Creates the proxy's socket at `path`, with mode 0600 (SEC-10b). The
    /// socket accepts connections from here on, and [`Bound::serve`] answers
    /// them.
    ///
    /// # Errors
    ///
    /// Where the path does not fit in the address of a Unix socket, or the
    /// socket cannot be created.
    pub fn bind(self, path: &Path) -> Result<Bound<C>, BindError> {
        let listener = UnixListener::bind(path).map_err(|source| {
            // A NUL byte is the other path that the address refuses.
            if source.kind() == io::ErrorKind::InvalidInput
                && !path.as_os_str().as_encoded_bytes().contains(&0)
            {
                BindError::TooLong(path.to_owned())
            } else {
                BindError::Io {
                    path: path.to_owned(),
                    source,
                }
            }
        })?;
        let socket = Socket {
            listener,
            path: path.to_owned(),
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(MODE)).map_err(
            |source| BindError::Io {
                path: path.to_owned(),
                source,
            },
        )?;
        Ok(Bound {
            socket,
            shared: Arc::new(self),
        })
    }
}

/// Why the proxy's socket could not be created.
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// The path is longer than the address of a Unix socket holds (SEC-10b).
    #[error("the path {0:?} is too long for a Unix socket")]
    TooLong(PathBuf),
    /// Another error.
    #[error("cannot create the socket {path:?}: {source}")]
    Io {
        /// The socket's path.
        path: PathBuf,
        /// The error.
        source: io::Error,
    },
}

/// The listener, which removes its socket when it is dropped.
struct Socket {
    listener: UnixListener,
    path: PathBuf,
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A proxy with its socket.
pub struct Bound<C> {
    socket: Socket,
    shared: Arc<Proxy<C>>,
}

impl<C: Connect> Bound<C> {
    /// Answers the socket's connections until `activity` is cancelled. It
    /// then closes the listener, removes the socket and closes every tunnel
    /// that is still open (AG-19).
    pub async fn serve(self, activity: CancellationToken) {
        let Self { socket, shared } = self;
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                () = activity.cancelled() => break,
                accepted = socket.listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        connections.spawn(connection(stream, Arc::clone(&shared)));
                    }
                    Err(_) => tokio::time::sleep(ACCEPT_PAUSE).await,
                },
                // An empty set answers at once, and the branch then waits for
                // the next turn of the loop.
                Some(_) = connections.join_next() => {}
            }
        }
        drop(socket);
        connections.shutdown().await;
    }
}

/// A tunnel that the answer to a `CONNECT` opened: the worker's side, once
/// `hyper` hands it over, and the host's.
type Tunnel<S> = Arc<Mutex<Option<(OnUpgrade, S)>>>;

/// Reads one request head from `stream`, answers it, and carries the bytes
/// of the tunnel that the answer opened, if it opened one.
async fn connection<C: Connect>(stream: UnixStream, proxy: Arc<Proxy<C>>) {
    let tunnel: Tunnel<C::Stream> = Arc::new(Mutex::new(None));
    let service = {
        let (proxy, tunnel) = (Arc::clone(&proxy), Arc::clone(&tunnel));
        service_fn(move |request| {
            let (proxy, tunnel) = (Arc::clone(&proxy), Arc::clone(&tunnel));
            async move { Ok::<_, Infallible>(proxy.answer(request, &tunnel).await) }
        })
    };
    let served = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(proxy.limits.head)
        .max_buf_size(MAX_HEAD)
        .keep_alive(false)
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades()
        .await;
    // A head that never reached the proxy's own code: `hyper` has answered
    // it by itself (SEC-10b). With no keep-alive it reads no second head, so
    // none of the three follows a head that the proxy answered.
    if let Err(error) = served
        && (error.is_parse() || error.is_timeout() || error.is_incomplete_message())
    {
        proxy.log.denied(Denial {
            time: SystemTime::now(),
            activity: proxy.activity,
            sent: None,
            reason: Reason::Malformed,
        });
    }
    let opened = tunnel.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some((upgrade, mut host)) = opened
        && let Ok(upgraded) = upgrade.await
    {
        let _ = tokio::io::copy_bidirectional(&mut TokioIo::new(upgraded), &mut host).await;
    }
}

impl<C: Connect> Proxy<C> {
    /// The answer to `request`. A tunnel that it opens is left in `tunnel`.
    async fn answer(
        &self,
        mut request: Request<Incoming>,
        tunnel: &Tunnel<C::Stream>,
    ) -> Response<String> {
        let name = match self.check(&request) {
            Ok(name) => name,
            Err(reason) => {
                self.log.denied(Denial {
                    time: SystemTime::now(),
                    activity: self.activity,
                    sent: Some(Sent {
                        method: cut(request.method().as_str()),
                        target: cut(&request.uri().to_string()),
                    }),
                    reason,
                });
                return closing(StatusCode::FORBIDDEN, format!("refused: {reason}\n"));
            }
        };
        let error =
            match tokio::time::timeout(self.limits.connect, self.connect.connect(name.clone()))
                .await
            {
                Ok(Ok(host)) => {
                    let upgrade = hyper::upgrade::on(&mut request);
                    *tunnel.lock().unwrap_or_else(PoisonError::into_inner) = Some((upgrade, host));
                    return Response::new(String::new());
                }
                Ok(Err(error)) => error.to_string(),
                Err(_) => format!("no answer within {:?}", self.limits.connect),
            };
        self.log.failed(Failure {
            time: SystemTime::now(),
            activity: self.activity,
            name,
            error,
        });
        closing(
            StatusCode::BAD_GATEWAY,
            "the host cannot be reached\n".to_owned(),
        )
    }

    /// The listed name, folded, that `request` asks for a tunnel to, or why
    /// it is refused.
    fn check(&self, request: &Request<Incoming>) -> Result<String, Reason> {
        if request.method() != Method::CONNECT {
            return Err(Reason::Method);
        }
        let uri = request.uri();
        // The target is `<host>:<port>` and nothing else: no scheme, no path
        // and no user.
        let authority = uri
            .authority()
            .filter(|authority| uri.scheme().is_none() && !authority.as_str().contains('@'))
            .ok_or(Reason::Host)?;
        let host = authority.host();
        // The list holds no address, so a target that is one is not on it.
        if !self.list.allows(host) {
            return Err(Reason::Host);
        }
        if authority.port_u16() != Some(PORT) {
            return Err(Reason::Port);
        }
        Ok(host.to_ascii_lowercase())
    }
}

/// An answer with `status` and the one line `text`, after which the proxy
/// closes the connection.
fn closing(status: StatusCode, text: String) -> Response<String> {
    let mut response = Response::new(text);
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONNECTION, HeaderValue::from_static("close"));
    response
}

/// `text` cut at [`KEPT`] bytes, at the boundary of a character.
fn cut(text: &str) -> String {
    text[..text.floor_char_boundary(KEPT)].to_owned()
}
