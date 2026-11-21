use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
#[ignore = "requires the pinned ESLint dependencies"]
fn javascript_rules_find_quality_problems() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lint = |source: &str| {
        let mut process = Command::new("node")
            .current_dir(&root)
            .args([
                "tools/javascript-quality/node_modules/eslint/bin/eslint.js",
                "--no-ignore",
                "--config",
                "tools/javascript-quality/eslint.config.mjs",
                "--stdin",
                "--stdin-filename",
                "services/homekit/ui/quality-fixture.js",
                "--format",
                "json",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        process
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let result = process.wait_with_output().unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&result.stdout).expect("ESLint JSON result");
        (result.status.success(), report)
    };
    assert!(lint("export function greet(person) { return person; }").0);
    let (success, report) =
        lint("export function example(a) { const abandoned = 4; for (;;) { return a + 42; } }");
    assert!(!success, "bad JavaScript must be rejected");
    let messages = report[0]["messages"].as_array().unwrap();
    for rule in [
        "id-length",
        "no-unused-vars",
        "no-magic-numbers",
        "no-restricted-syntax",
    ] {
        assert!(
            messages.iter().any(|m| m["ruleId"] == rule),
            "missing {rule}"
        );
    }
}
