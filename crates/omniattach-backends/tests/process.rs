#![cfg(unix)]
use omniattach_backends::*;
use std::time::{Duration, Instant};

fn execute(script: &str, options: &RunOptions) -> Result<ProcessOutput> {
    ProcessRunner.run(
        &CommandSpec::new("/bin/sh", "/tmp").args(["-c", script]),
        options,
    )
}
#[test]
fn captures_dual_streams_without_shell_expansion_of_arguments() {
    let scratch = tempfile::tempdir().unwrap();
    let command = CommandSpec::new("/usr/bin/printf", scratch.path())
        .args(["%s", "$(touch should-not-exist); * | xyz"]);
    let result = ProcessRunner.run(&command, &RunOptions::default()).unwrap();
    assert_eq!(
        result.stdout_text().unwrap(),
        "$(touch should-not-exist); * | xyz"
    );
    assert!(!scratch.path().join("should-not-exist").exists());
    let result = execute(
        "printf output; printf diagnostic >&2",
        &RunOptions::default(),
    )
    .unwrap();
    assert_eq!(result.stdout, b"output");
    assert_eq!(result.stderr, b"diagnostic");
}
#[test]
fn return_codes_and_stderr_are_preserved() {
    let error = execute("printf failure >&2; exit 7", &RunOptions::default()).unwrap_err();
    assert!(
        matches!(error, Error::ProcessFailed { code:Some(7), stderr } if stderr.contains("failure"))
    );
}
#[test]
fn missing_program_is_an_explicit_spawn_error() {
    assert!(matches!(
        ProcessRunner.run(
            &CommandSpec::new("/nonexistent/omniattach-tool", "/tmp"),
            &RunOptions::default()
        ),
        Err(Error::Spawn { .. })
    ));
}
#[test]
fn timeout_kills_descendant_holding_pipes_after_parent_exit() {
    let options = RunOptions {
        timeout: Duration::from_millis(100),
        ..Default::default()
    };
    let start = Instant::now();
    assert!(matches!(
        execute("sleep 30 & exit 0", &options),
        Err(Error::Timeout(_))
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
}
#[test]
fn timeout_kills_descendants_before_they_modify_files() {
    let scratch = tempfile::tempdir().unwrap();
    let mut command = CommandSpec::new("/bin/sh", scratch.path())
        .args(["-c", "(sleep 0.35; touch survivor) & wait"]);
    command.inherit_env = false;
    let options = RunOptions {
        timeout: Duration::from_millis(50),
        ..Default::default()
    };
    assert!(matches!(
        ProcessRunner.run(&command, &options),
        Err(Error::Timeout(_))
    ));
    std::thread::sleep(Duration::from_millis(450));
    assert!(!scratch.path().join("survivor").exists());
}
#[test]
fn cancelled_before_start_never_spawns() {
    let options = RunOptions::default();
    options.cancellation.cancel();
    assert!(matches!(execute("exit 0", &options), Err(Error::Cancelled)));
}
#[test]
fn cancellation_interrupts_running_process() {
    let options = RunOptions::default();
    let token = options.cancellation.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        token.cancel();
    });
    let start = Instant::now();
    assert!(matches!(
        execute("sleep 30", &options),
        Err(Error::Cancelled)
    ));
    thread.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
}
#[test]
fn stdout_limit_and_stderr_limit_are_independent() {
    let options = RunOptions {
        max_stdout_bytes: 10,
        ..Default::default()
    };
    assert!(matches!(
        execute("printf 12345678901", &options),
        Err(Error::Limit(_))
    ));
    let options = RunOptions {
        max_stderr_bytes: 10,
        ..Default::default()
    };
    assert!(matches!(
        execute("printf 12345678901 >&2", &options),
        Err(Error::Limit(_))
    ));
}
#[test]
fn concurrent_large_streams_do_not_deadlock() {
    let options = RunOptions {
        timeout: Duration::from_secs(3),
        max_stdout_bytes: 200000,
        max_stderr_bytes: 200000,
        ..Default::default()
    };
    let output = execute(
        "head -c 100000 /dev/zero & head -c 100000 /dev/zero >&2 & wait",
        &options,
    )
    .unwrap();
    assert_eq!(output.stdout.len(), 100000);
    assert_eq!(output.stderr.len(), 100000);
}
#[test]
fn environment_override_is_literal_and_stdin_is_closed() {
    let mut command = CommandSpec::new("/bin/sh", "/tmp").args([
        "-c",
        "printf '%s' \"$OMNI_TEST\"; if read x; then exit 9; fi",
    ]);
    command
        .env
        .insert("OMNI_TEST".into(), "two words; $(false)".into());
    let output = ProcessRunner.run(&command, &RunOptions::default()).unwrap();
    assert_eq!(output.stdout_text().unwrap(), "two words; $(false)");
}
#[test]
fn utf8_decoding_is_explicit_and_strict() {
    let output = ProcessOutput {
        stdout: vec![255],
        stderr: vec![],
        elapsed: Duration::ZERO,
    };
    assert!(output.stdout_text().is_err());
}
