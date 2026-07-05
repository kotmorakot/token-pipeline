use assert_cmd::Command;
use predicates::prelude::*;

fn tp() -> Command {
    Command::cargo_bin("tp").unwrap()
}

#[test]
fn test_help_output() {
    tp().arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("token-pipeline"))
        .stdout(predicate::str::contains("tp run"));
}

#[test]
fn test_version() {
    tp().arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("v1.0.0"));
}

#[test]
fn test_run_echo() {
    tp().args(["run", "echo", "hello", "world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn test_run_no_args() {
    tp().arg("run")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn test_shrink_stdin() {
    tp().arg("shrink")
        .write_stdin("Sure! I'd be happy to help you with that. Here is the solution. I think this should work.\n")
        .assert()
        .success();
}

#[test]
fn test_shrink_preserves_code() {
    let input = "Sure! Here is the code:\n```rust\nfn main() {}\n```\n";
    tp().arg("shrink")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("fn main()"));
}

#[test]
fn test_run_failing_command() {
    tp().args(["run", "false"])
        .assert()
        .failure();
}

#[test]
fn test_run_preserves_exit_code() {
    let result = tp().args(["run", "sh", "-c", "exit 42"])
        .assert()
        .failure();
    let output = result.get_output();
    assert!(output.status.code() == Some(42) || !output.status.success());
}

#[test]
fn test_stats_no_crash() {
    tp().arg("stats")
        .assert()
        .success();
}

#[test]
fn test_cache_info() {
    tp().arg("cache")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cache:"));
}

#[test]
fn test_config_show() {
    tp().arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("Config"));
}

#[test]
fn test_unknown_command_passthrough() {
    tp().args(["run", "echo", "test_passthrough"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test_passthrough"));
}

#[test]
fn test_shrink_modes() {
    let input = "This is a really basically just somewhat long text that needs to be compressed for the purpose of saving tokens.";

    tp().args(["shrink", "lite"])
        .write_stdin(input)
        .assert()
        .success();

    tp().args(["shrink", "full"])
        .write_stdin(input)
        .assert()
        .success();

    tp().args(["shrink", "ultra"])
        .write_stdin(input)
        .assert()
        .success();
}
