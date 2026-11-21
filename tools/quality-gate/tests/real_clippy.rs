use std::{fs, process::Command};

#[test]
#[ignore = "requires Rust Clippy"]
fn clippy_and_gate_detect_a_new_short_rust_identifier() {
    let root = std::env::temp_dir().join(format!("sidekicks-real-clippy-{}", std::process::id()));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let source = root.join("src/lib.rs");
    let base = root.join("base.jsonl");
    let current = root.join("current.jsonl");
    let scan = |code: &str, report: &std::path::Path| {
        fs::write(&source, code).unwrap();
        let output = Command::new("cargo")
            .current_dir(&root)
            .env("CARGO_TARGET_DIR", root.join("target"))
            .args([
                "clippy",
                "--offline",
                "--message-format=json",
                "--lib",
                "--",
                "-W",
                "clippy::min_ident_chars",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture Clippy failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(report, output.stdout).unwrap();
    };
    scan("pub fn compute(value: i32) -> i32 { value }", &base);
    scan(
        "pub fn compute(value: i32) -> i32 { let a = value; a }",
        &current,
    );
    let gate = std::env::var_os("QUALITY_GATE_EXEC")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_sidekicks-quality-gate").into());
    let output = Command::new(gate)
        .arg("clippy")
        .arg(&base)
        .arg(&current)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "new short identifier must fail the gate"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("min_ident_chars"));
    fs::remove_dir_all(root).unwrap();
}
