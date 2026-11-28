//! A run's report: JSON and readable text, one pair per run under
//! `evals/reports/runs/` (docs/features/learning-layers/spec.md, FR-32).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::cases::Graded;
use crate::scenario::Outcome;

#[derive(Debug, Default)]
pub struct Report {
    pub scenarios: Vec<Outcome>,
    /// Golden replays: name and differences (empty: it matches).
    pub golden: Vec<(String, Vec<String>)>,
    pub cases: Vec<Graded>,
    pub seconds: f64,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.scenarios.iter().all(|s| s.passed)
            && self.golden.iter().all(|(_, d)| d.is_empty())
            && self.cases.iter().all(|c| c.pass)
    }

    pub fn json(&self) -> Value {
        json!({
            "passed": self.passed(),
            "seconds": self.seconds,
            "scenarios": self.scenarios,
            "golden": self.golden.iter().map(|(name, diff)| json!({ "name": name, "differences": diff })).collect::<Vec<_>>(),
            "cases": self.cases,
        })
    }

    pub fn text(&self) -> String {
        let mut out = format!(
            "# Evo evals\n\n{} in {:.1} s\n\n## Scenarios\n\n",
            if self.passed() { "PASSED" } else { "FAILED" },
            self.seconds
        );
        for s in &self.scenarios {
            out.push_str(&format!(
                "- {} `{}` ({})\n",
                if s.passed { "pass" } else { "FAIL" },
                s.name,
                s.world
            ));
            for failure in s.failures() {
                out.push_str(&format!("  - {failure}\n"));
            }
        }
        out.push_str("\n## Golden replay\n\n");
        for (name, diff) in &self.golden {
            out.push_str(&format!(
                "- {} `{name}`\n",
                if diff.is_empty() { "pass" } else { "FAIL" }
            ));
            for d in diff {
                out.push_str(&format!("  - {d}\n"));
            }
        }
        out.push_str("\n## Cases\n\n");
        for c in &self.cases {
            let live = match (c.ms, c.cost_usd) {
                (Some(ms), Some(cost)) => format!(" — {ms} ms, ${cost:.4}"),
                _ => String::new(),
            };
            out.push_str(&format!(
                "- {} {} / `{}`{live}\n",
                if c.pass { "pass" } else { "miss" },
                c.set,
                c.case
            ));
            for m in &c.misses {
                out.push_str(&format!("  - {m}\n"));
            }
        }
        out
    }

    /// Write `<time>.json` and `<time>.md` under `dir`.
    pub fn write(&self, dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let base = dir.join(format!("{stamp}"));
        std::fs::write(
            base.with_extension("json"),
            serde_json::to_string_pretty(&self.json()).unwrap_or_default(),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(base.with_extension("md"), self.text()).map_err(|e| e.to_string())?;
        Ok(base.with_extension("md"))
    }
}
