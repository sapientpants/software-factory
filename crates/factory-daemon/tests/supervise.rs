//! The entry of the supervisor shim that `factory-cli` reaches (SPEC AG-16,
//! 19.1).

use std::ffi::OsString;
use std::process::ExitCode;

use factory_daemon::supervise::run;

#[test]
fn arguments_outside_the_protocol_fail_the_shim() {
    let args = ["--pipe-fd", "3"].map(OsString::from).to_vec();
    assert_eq!(run(args), ExitCode::from(125));
}
