//! End-to-end `morphod import` behavior, driven through the real binary
//! against the committed dev fixtures.

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> PathBuf {
    // `cargo test` puts integration test binaries next to the crate binaries.
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop(); // deps/
    path.pop(); // debug/
    path.join("morphod")
}

fn fixtures() -> PathBuf {
    // crates/morphod -> crates -> core -> core/fixtures
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("fixtures")
}

fn run(data_dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(binary())
        .arg("--data-dir")
        .arg(data_dir)
        .args(args)
        // Run in an empty directory with no ambient configuration so the test
        // sees pure defaults.
        .current_dir(data_dir)
        .env_remove("MORPHOD_CONFIG")
        .env_remove("MORPHOD_DATA_DIR")
        .env_remove("MORPHOD_BIND")
        .env_remove("MORPHOD_ADMIN_UI_DIST")
        .env_remove("RUST_LOG")
        .output()
        .expect("failed to run morphod");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

#[test]
fn imports_fixtures_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path();
    let base = fixtures().join("base-words.txt");
    let target = fixtures().join("target-words.jsonl");
    assert!(base.is_file(), "missing fixture {}", base.display());
    assert!(target.is_file(), "missing fixture {}", target.display());

    let (ok, out) = run(
        data_dir,
        &[
            "import",
            "--wordlist",
            base.to_str().unwrap(),
            "--role",
            "base",
        ],
    );
    assert!(ok, "{out}");
    assert!(out.contains("64 new"), "{out}");

    let (ok, out) = run(
        data_dir,
        &[
            "import",
            "--wordlist",
            target.to_str().unwrap(),
            "--role",
            "target",
        ],
    );
    assert!(ok, "{out}");
    assert!(out.contains("25 new"), "{out}");

    // Re-importing both lists changes nothing at all.
    for (path, role) in [(&base, "base"), (&target, "target")] {
        let (ok, out) = run(
            data_dir,
            &[
                "import",
                "--wordlist",
                path.to_str().unwrap(),
                "--role",
                role,
            ],
        );
        assert!(ok, "{out}");
        assert!(out.contains("0 new"), "{out}");
        assert!(out.contains("0 updated"), "{out}");
    }

    let (ok, out) = run(data_dir, &["status"]);
    assert!(ok, "{out}");
    assert!(out.contains("89 total"), "{out}");
    assert!(out.contains("25 target"), "{out}");
    assert!(out.contains("64 base"), "{out}");
    assert!(out.contains("plan            none built yet"), "{out}");
}

#[test]
fn missing_wordlist_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(
        dir.path(),
        &[
            "import",
            "--wordlist",
            "/nonexistent/list.txt",
            "--role",
            "base",
        ],
    );
    assert!(!ok);
    assert!(out.contains("reading word list"), "{out}");
}

#[test]
fn status_works_on_a_fresh_database() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(dir.path(), &["status"]);
    assert!(ok, "{out}");
    assert!(out.contains("0 total"), "{out}");
    assert!(dir.path().join("working.db").is_file());
}
