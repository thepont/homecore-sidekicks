use serde_json::Value;
use std::{collections::BTreeMap, env, fs, process::ExitCode};

const COGNITIVE_LIMIT: u64 = 15;
const CYCLOMATIC_LIMIT: u64 = 12;

type Scores = BTreeMap<String, Vec<(u64, u64)>>;

fn number(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing or invalid {field}"))
}

fn collect(functions: &Value, scope: &str, scores: &mut Scores) -> Result<(), String> {
    for function in functions.as_array().ok_or("invalid functions")? {
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or("invalid function name")?;
        let kind = function
            .get("kind")
            .and_then(Value::as_str)
            .ok_or("invalid function kind")?;
        let key = format!("{scope}::{kind}::{name}");
        scores.entry(key.clone()).or_default().push((
            number(function, "cognitive")?,
            number(function, "cyclomatic")?,
        ));
        if let Some(children) = function.get("children") {
            collect(children, &key, scores)?;
        }
    }
    Ok(())
}

fn read_report(path: &str) -> Result<Scores, String> {
    let contents = fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let report: Value =
        serde_json::from_slice(&contents).map_err(|e| format!("invalid report {path}: {e}"))?;
    if number(
        report.get("summary").ok_or("missing summary")?,
        "parse_error_count",
    )? != 0
    {
        return Err(format!("{path} has source parse errors"));
    }
    let files = report
        .get("files")
        .and_then(Value::as_array)
        .ok_or("invalid files")?;
    if files.is_empty() {
        return Err(format!("{path} contains no scanned files"));
    }
    let mut scores = Scores::new();
    for file in files {
        let path = file
            .get("path")
            .and_then(Value::as_str)
            .ok_or("invalid file path")?;
        collect(
            file.get("functions").ok_or("missing functions")?,
            path,
            &mut scores,
        )?;
    }
    Ok(scores)
}

fn check(base: Scores, current: Scores) -> Result<(), String> {
    let mut failures = Vec::new();
    for (key, values) in current {
        let previous = base.get(&key).cloned().unwrap_or_default();
        for (index, (field, limit)) in [
            ("cognitive", COGNITIVE_LIMIT),
            ("cyclomatic", CYCLOMATIC_LIMIT),
        ]
        .into_iter()
        .enumerate()
        {
            let mut before: Vec<_> = previous
                .iter()
                .map(|v| if index == 0 { v.0 } else { v.1 })
                .collect();
            let mut after: Vec<_> = values
                .iter()
                .map(|v| if index == 0 { v.0 } else { v.1 })
                .collect();
            before.sort_unstable_by(|a, b| b.cmp(a));
            after.sort_unstable_by(|a, b| b.cmp(a));
            for (position, score) in after.into_iter().enumerate() {
                let allowed = before.get(position).copied().unwrap_or(0).max(limit);
                if score > allowed {
                    failures.push(format!("{key}: {field} {score} exceeds allowed {allowed}"));
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 3 || args[0] != "complexity" {
        return Err("usage: sidekicks-quality-gate complexity BASE.json CURRENT.json".into());
    }
    check(read_report(&args[1])?, read_report(&args[2])?)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("Complexity is within the baseline and thresholds");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
