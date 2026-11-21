use std::{fs, process::Command};

#[test]
fn rejects_new_rust_readability_warnings() {
    let root = std::env::temp_dir().join(format!("sidekicks-clippy-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let base = root.join("base.jsonl");
    let current = root.join("current.jsonl");
    let message = r#"{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::min_ident_chars"},"spans":[{"file_name":"src/lib.rs","is_primary":true}]}}"#;
    let finished = "{\"reason\":\"build-finished\",\"success\":true}\n";
    fs::write(&base, format!("{message}\n{finished}")).unwrap();
    fs::write(&current, format!("{message}\n{message}\n{finished}")).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_sidekicks-quality-gate"))
            .args(["clippy", base.to_str().unwrap(), current.to_str().unwrap()])
            .output()
            .unwrap()
    };
    let worse = run();
    assert!(!worse.status.success());
    assert!(String::from_utf8_lossy(&worse.stderr).contains("min_ident_chars"));
    fs::write(&current, format!("{message}\n{finished}")).unwrap();
    assert!(run().status.success());
    fs::remove_file(base).unwrap();
    fs::remove_file(current).unwrap();
    fs::remove_dir(root).unwrap();
}
