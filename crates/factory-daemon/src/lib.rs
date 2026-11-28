//! Supervisor, scheduler, reconciler, effect executors and control API (SPEC 19.1).

pub mod host;
pub mod sandbox_exec;
pub mod sandbox_init;
pub mod supervise;

/// The processes of `factory-proc`, for `factory-cli` and its tests, which
/// depend on this crate alone (19.1).
pub use factory_proc as process;
