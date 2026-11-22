//! Code mode on or off for this engine (docs/features/code-mode/spec.md,
//! FR-1 – FR-3, FR-20; plan 3.3, 3.8).
//!
//! Off unless the engine starts with `--code-mode` (the desktop passes it
//! from the person's **Settings → Experimental → Code mode**). Project
//! settings have no key for it. `set_code_mode` changes it live: [`apply`]
//! adds or removes `run_code` in the session's tools, so the next model
//! request has it or not, with no restart. Subagents take the parent's active
//! tools, so they follow it.

use std::sync::atomic::{AtomicBool, Ordering};

use evo_harness::agent_harness::AgentHarness;
use evo_harness::types::HarnessTool;

use crate::tools::code::{create_code_tool_for, NAME};

static ON: AtomicBool = AtomicBool::new(false);

/// Whether code mode is on.
pub fn enabled() -> bool {
    ON.load(Ordering::SeqCst)
}

/// Turn code mode on or off; [`apply`] makes a running session follow.
pub fn set(on: bool) {
    ON.store(on, Ordering::SeqCst);
}

/// The code tool for a session whose tools are `tools`: its description
/// carries typed declarations of them (FR-18).
pub fn code_tool(tools: &[evo_ai::types::Tool]) -> HarnessTool {
    let refs: Vec<&evo_ai::types::Tool> = tools.iter().collect();
    HarnessTool::new(create_code_tool_for(&refs))
}

/// Add `run_code` to the session's tools and active set when code mode is
/// on, remove it when off. Other tools stay as they are.
pub async fn apply(harness: &AgentHarness) -> Result<(), String> {
    let on = enabled();
    let mut tools = harness.get_tools().await.map_err(|e| e.to_string())?;
    let mut active = harness
        .lane("main")
        .get_active_tools()
        .await
        .map_err(|e| e.to_string())?;
    let has = tools.iter().any(|tool| tool.tool.schema().name == NAME);
    if on == has && (active.iter().any(|name| name == NAME) == on) {
        return Ok(());
    }
    if on {
        if !has {
            let schemas: Vec<evo_ai::types::Tool> = tools
                .iter()
                .filter(|tool| active.contains(&tool.tool.schema().name))
                .map(|tool| tool.tool.schema().clone())
                .collect();
            tools.push(code_tool(&schemas));
        }
        if !active.iter().any(|name| name == NAME) {
            active.push(NAME.to_owned());
        }
    } else {
        tools.retain(|tool| tool.tool.schema().name != NAME);
        active.retain(|name| name != NAME);
    }
    harness
        .set_tools(tools, Some(active))
        .await
        .map_err(|e| e.to_string())
}

/// Tests that switch code mode take this lock, so they do not race.
#[cfg(test)]
pub(crate) static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use super::*;
    use evo_harness::types::AgentHarnessOptions;

    fn read_tool() -> HarnessTool {
        let env = std::sync::Arc::new(evo_tools::OsExecutionEnv::with_cwd(".".into()));
        let ctx = evo_tools::ExecutionToolContext::new(env.clone(), Some(env));
        HarnessTool::new(evo_tools::create_read_tool(&ctx, None))
    }

    async fn names(harness: &AgentHarness) -> (Vec<String>, Vec<String>) {
        let tools = harness
            .get_tools()
            .await
            .unwrap()
            .iter()
            .map(|t| t.tool.schema().name.clone())
            .collect();
        let active = harness.lane("main").get_active_tools().await.unwrap();
        (tools, active)
    }

    // FR-1: off unless asked for on the command line.
    #[test]
    fn the_argument_turns_it_on() {
        assert!(!crate::args::parse_args(&[]).code_mode);
        assert!(crate::args::parse_args(&["--code-mode".to_string()]).code_mode);
    }

    // FR-2, FR-3, FR-18: on adds `run_code` (declaring the session's tools),
    // off removes it and leaves the other tools as they were.
    #[tokio::test]
    async fn switching_adds_and_removes_the_code_tool() {
        let _lock = TEST_LOCK.lock().await;
        let harness = AgentHarness::create(AgentHarnessOptions {
            tools: vec![read_tool()],
            active_tool_names: vec!["read".into()],
            ..Default::default()
        })
        .await
        .unwrap();
        set(true);
        apply(&harness).await.unwrap();
        let (tools, active) = names(&harness).await;
        assert_eq!(tools, ["read", "run_code"]);
        assert_eq!(active, ["read", "run_code"]);
        let code = harness
            .get_tools()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.tool.schema().name == NAME)
            .unwrap();
        assert!(code.tool.schema().description.contains("tools.read({path"));
        // Applying again changes nothing.
        apply(&harness).await.unwrap();
        assert_eq!(names(&harness).await.0, ["read", "run_code"]);

        set(false);
        apply(&harness).await.unwrap();
        let (tools, active) = names(&harness).await;
        assert_eq!(tools, ["read"]);
        assert_eq!(active, ["read"]);
    }
}
