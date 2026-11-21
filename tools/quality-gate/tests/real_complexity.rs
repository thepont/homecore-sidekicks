use std::{fs, process::Command};

#[test]
#[ignore = "requires the pinned cccc executable"]
fn analyzer_and_gate_detect_complex_rust_code() {
    let root = std::env::temp_dir().join(format!("sidekicks-complexity-{}", std::process::id()));
    let base = root.join("base");
    let current = root.join("current");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::create_dir_all(current.join("src")).unwrap();
    fs::write(
        base.join("src/example.rs"),
        "fn evaluate(value: i32) -> i32 { value }",
    )
    .unwrap();
    let mut code = "fn evaluate(value: i32) -> i32 {".to_owned();
    for value in 0..16 {
        code.push_str(&format!("if value == {value} {{ return {value}; }}"));
    }
    code.push_str("value }");
    fs::write(current.join("src/example.rs"), code).unwrap();
    let analyzer = std::env::var("CCCC_BIN").expect("set CCCC_BIN to the pinned analyzer");
    let scan = |dir: &std::path::Path, destination: &std::path::Path| {
        let output = Command::new(&analyzer)
            .current_dir(dir)
            .args(["--lang", "rust", "src"])
            .output()
            .unwrap();
        assert!(output.status.success(), "analyzer failed");
        fs::write(destination, output.stdout).unwrap();
    };
    let baseline = root.join("baseline.json");
    let report = root.join("current.json");
    scan(&base, &baseline);
    scan(&current, &report);
    let gate = |candidate: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_sidekicks-quality-gate"))
            .arg("complexity")
            .arg(&baseline)
            .arg(candidate)
            .output()
            .unwrap()
    };
    assert!(gate(&baseline).status.success());
    let failure = gate(&report);
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("src/example.rs"));
    fs::remove_file(base.join("src/example.rs")).unwrap();
    fs::remove_file(current.join("src/example.rs")).unwrap();
    fs::remove_file(baseline).unwrap();
    fs::remove_file(report).unwrap();
    fs::remove_dir(base.join("src")).unwrap();
    fs::remove_dir(current.join("src")).unwrap();
    fs::remove_dir(base).unwrap();
    fs::remove_dir(current).unwrap();
    fs::remove_dir(root).unwrap();
}
