use assert_cmd::Command;
use predicates::prelude::predicate::str::contains; // Used for string matching

fn command() -> Command {
    let path = assert_cmd::cargo::cargo_bin!("portfolio-solver-framework");
    let mut cmd = Command::new(path);
    cmd.arg("run");
    cmd
}

#[test]
fn test_cli_success() {
    let mut cmd = command();
    cmd.args([
        "tests/data/accap.mzn",
        "tests/data/accap_instance6.dzn",
        "-v",
        "info",
    ])
    .assert()
    .success()
    .stdout(contains("=========="));
}

#[test]
fn test_cli_failure() {
    let mut cmd = command();
    cmd.args(["tests/data/accap_instance6.dzn", "-v", "info"])
        .assert()
        .failure()
        .stderr(contains("ERROR"));
}
