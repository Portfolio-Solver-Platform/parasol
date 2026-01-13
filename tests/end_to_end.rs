use assert_cmd::Command;

use predicates::prelude::predicate::str::contains; // Used for string matching
#[test]
fn test_cli_success() {
    let path = assert_cmd::cargo::cargo_bin!("portfolio-solver-framework");
    let mut cmd = Command::new(path);
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
    let path = assert_cmd::cargo::cargo_bin!("portfolio-solver-framework");
    let mut cmd = Command::new(path);
    cmd.args(["tests/data/accap_instance6.dzn", "-v", "info"])
        .assert()
        .failure()
        .stderr(contains("ERROR"));
}
