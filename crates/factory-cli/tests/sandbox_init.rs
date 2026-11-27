//! `sf __sandbox-init`, the first process of SPEC SEC-10a, as the binary
//! `sf` runs it. What the first process does is tested in the sandbox tests
//! of `factory-sandbox`, whose binary plays it inside an instance; this shows
//! that `sf` reaches it (19.1). It is not run here with a command: outside a
//! sandbox it would bind the host's port 3128.

use factory_daemon::process::Command;

/// The binary under test.
const SF: &str = env!("CARGO_BIN_EXE_sf");

#[test]
fn sf_reaches_the_first_process_which_refuses_to_run_without_a_command() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (code, stderr) = runtime.block_on(async {
        let mut command = Command::new(SF);
        command.arg("__sandbox-init");
        let (sink, mut read) = tokio::io::duplex(1 << 16);
        let mut process = command.spawn(sink).await.unwrap();
        let code = process.wait().await.unwrap().code();
        drop(process);
        let mut stderr = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut read, &mut stderr)
            .await
            .unwrap();
        (code, stderr)
    });

    // The exit code of a first process that started no command.
    assert_eq!(code, Some(125));
    assert!(stderr.starts_with("sf __sandbox-init: "), "{stderr}");
}
