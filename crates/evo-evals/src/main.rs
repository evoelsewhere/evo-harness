//! `evo-evals`: the Evo harness's eval suite
//! (docs/features/learning-layers/spec.md, FR-26 – FR-32).
//!
//! ```text
//! cargo run -p evo-evals -- offline            # scenarios, golden replay, case sets (CI)
//! cargo run -p evo-evals -- live               # real models, with EVO_EVAL_LIVE=1
//! cargo run -p evo-evals -- redact <in> <out>  # a channel → a golden file
//! cargo run -p evo-evals -- baseline <golden>  # save a golden's replay as its baseline
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use evo_evals::{cases, golden, live, report::Report, scenario};
use serde_json::Value;

/// The repository's `evals/` folder.
fn evals_dir() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.join("..").join("..").join("evals")
}

fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_str(
        &std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", path.display()))
}

async fn offline(evals: &Path) -> Result<Report, String> {
    let started = Instant::now();
    let mut report = Report::default();
    for (_, s) in scenario::load_dir(&evals.join("scenarios"))? {
        report.scenarios.push(scenario::run(&s).await);
    }
    let mut goldens: Vec<PathBuf> = std::fs::read_dir(evals.join("golden/channels"))
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    goldens.sort();
    for path in goldens {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let replayed = golden::replay(&read_json(&path)?);
        let baseline = read_json(&evals.join("golden/baselines").join(format!("{name}.json")))?;
        report
            .golden
            .push((name, golden::compare(&replayed, &baseline)));
    }
    for set in ["dream", "router", "tool-choice"] {
        let list: Vec<Value> =
            serde_json::from_value(read_json(&evals.join("cases").join(format!("{set}.json")))?)
                .map_err(|e| e.to_string())?;
        report.cases.extend(cases::offline(set, &list));
    }
    report.seconds = started.elapsed().as_secs_f64();
    Ok(report)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let evals = evals_dir();
    let code = match args.first().map(String::as_str).unwrap_or("offline") {
        "offline" => match offline(&evals).await {
            Ok(report) => {
                print!("{}", report.text());
                match report.write(&evals.join("reports/runs")) {
                    Ok(path) => println!("\nreport: {}", path.display()),
                    Err(error) => eprintln!("the report was not written: {error}"),
                }
                i32::from(!report.passed())
            }
            Err(error) => {
                eprintln!("{error}");
                2
            }
        },
        "live" => match live::run(&evals).await {
            Ok(None) => 0,
            Ok(Some((graded, ok))) => {
                let report = Report {
                    cases: graded,
                    ..Default::default()
                };
                print!("{}", report.text());
                let _ = report.write(&evals.join("reports/runs"));
                i32::from(!(ok && report.passed()))
            }
            Err(error) => {
                eprintln!("{error}");
                2
            }
        },
        "redact" => match (args.get(1), args.get(2)) {
            (Some(input), Some(output)) => {
                match read_json(Path::new(input)).and_then(|source| golden::redact(&source)) {
                    Ok(redacted) => match std::fs::write(
                        output,
                        serde_json::to_string_pretty(&redacted).unwrap_or_default(),
                    ) {
                        Ok(()) => 0,
                        Err(error) => {
                            eprintln!("{error}");
                            1
                        }
                    },
                    Err(error) => {
                        eprintln!("{error}");
                        1
                    }
                }
            }
            _ => {
                eprintln!("usage: evo-evals redact <channel.json> <golden.json>");
                2
            }
        },
        "baseline" => match args.get(1).map(|p| read_json(Path::new(p))) {
            Some(Ok(golden_file)) => {
                let name = Path::new(&args[1])
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let path = evals.join("golden/baselines").join(format!("{name}.json"));
                match std::fs::write(
                    &path,
                    serde_json::to_string_pretty(&golden::replay(&golden_file)).unwrap_or_default(),
                ) {
                    Ok(()) => {
                        println!("baseline: {}", path.display());
                        0
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        1
                    }
                }
            }
            Some(Err(error)) => {
                eprintln!("{error}");
                1
            }
            None => {
                eprintln!("usage: evo-evals baseline <golden.json>");
                2
            }
        },
        other => {
            eprintln!("unknown command `{other}` (offline, live, redact, baseline)");
            2
        }
    };
    std::process::exit(code);
}
