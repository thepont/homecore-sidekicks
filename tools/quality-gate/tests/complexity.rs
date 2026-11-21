use std::{fs, process::Command};

#[test]
fn rejects_new_complexity_and_accepts_existing_findings() {
    let root = std::env::temp_dir().join(format!("sidekicks-quality-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let baseline = root.join("base.json");
    let current = root.join("current.json");
    let report = |complexity| {
        format!(
            r#"{{"summary":{{"parse_error_count":0}},"files":[{{"path":"src/main.rs","functions":[{{"name":"read","kind":"function","line":1,"cognitive":{complexity},"cyclomatic":4}}]}}]}}"#
        )
    };
    fs::write(&baseline, report(19)).unwrap();
    fs::write(&current, report(20)).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_sidekicks-quality-gate"))
            .args([
                "complexity",
                baseline.to_str().unwrap(),
                current.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let worse = run();
    assert!(
        !worse.status.success(),
        "increased existing complexity must fail"
    );
    assert!(String::from_utf8_lossy(&worse.stderr).contains("src/main.rs"));
    fs::write(&current, report(19)).unwrap();
    assert!(
        run().status.success(),
        "unchanged existing findings should pass"
    );
    fs::remove_file(baseline).unwrap();
    fs::remove_file(current).unwrap();
    fs::remove_dir(root).unwrap();
}
