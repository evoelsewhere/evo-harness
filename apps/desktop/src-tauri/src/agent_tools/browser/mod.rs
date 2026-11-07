//! Browser tools of the agent (ADR 0013): the built-in browser's pages, driven
//! from the shell. `driver.rs` is the page driver every tool goes through,
//! `core.rs` the 13 original tools; later groups (pointer, wait, screenshot,
//! dialog, …) sit beside them, one module each.
//!
//! To add a group: one `pub mod x;` line and one `tools.extend(x::tools());`
//! line in [`tools`]. A tool with the same name as an earlier one replaces it
//! (a group may improve a core tool without touching `core.rs`).
//!
//! Spec: docs/features/agent-browser (FR-1 – FR-17, FR-22, FR-24, FR-25), plan 3.2.

pub mod batch;
pub mod core;
pub mod dialog;
pub mod downloads;
pub mod driver;
pub mod mock;
pub mod network;
pub mod performance;
pub mod pointer;
pub mod read;
pub mod screenshot;
pub mod storage;
pub mod tabs;
pub mod upload;
pub mod viewport;
pub mod wait;

// What later tool groups import from here.
#[allow(unused_imports)]
pub use driver::{
    dialog_error, explain, finish, gate_page, is_dialog_error, js, key_events, resolve_page, resolve_tab, settle, show, show_browser, site_of,
    summary_line, tab_id_arg, Page, PageInfo, Stop, ToolResult, FOCUSED_STATE, KEYS, PAGE_INFO,
};

use super::Tool;

/// Every browser tool.
pub fn tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    tools.extend(core::tools());
    tools.extend(pointer::tools());
    tools.extend(wait::tools());
    tools.extend(screenshot::tools());
    tools.extend(dialog::tools());
    tools.extend(upload::tools());
    tools.extend(read::tools());
    tools.extend(batch::tools());
    tools.extend(tabs::tools());
    tools.extend(viewport::tools());
    tools.extend(storage::tools());
    tools.extend(network::tools());
    tools.extend(mock::tools());
    tools.extend(downloads::tools());
    tools.extend(performance::tools());
    replace_by_name(tools)
}

/// Keep one tool per name: a later definition replaces the earlier one, in the earlier one's place.
fn replace_by_name(tools: Vec<Tool>) -> Vec<Tool> {
    let mut kept: Vec<Tool> = Vec::with_capacity(tools.len());
    for tool in tools {
        match kept.iter().position(|earlier| earlier.name == tool.name) {
            Some(index) => kept[index] = tool,
            None => kept.push(tool),
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tools::{BoxFuture, Class, ToolCtx, ToolOutput};
    use serde_json::{json, Value};

    fn schema() -> Value {
        json!({ "type": "object" })
    }

    fn run(_ctx: ToolCtx, _args: Value) -> BoxFuture<ToolOutput> {
        Box::pin(async { ToolOutput::text("") })
    }

    fn tool(name: &'static str, description: &'static str) -> Tool {
        Tool { name, description, schema, class: Class::Observe, run }
    }

    #[test]
    fn a_later_tool_replaces_an_earlier_one_of_the_same_name_in_place() {
        let merged = replace_by_name(vec![tool("a", "first"), tool("b", "b"), tool("a", "second"), tool("c", "c")]);
        let names: Vec<_> = merged.iter().map(|t| t.name).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(merged[0].description, "second");
    }

    #[test]
    fn the_group_offers_the_thirteen_core_tools_and_the_wave_one_tools() {
        let names: Vec<_> = tools().iter().map(|t| t.name).collect();
        for core in core::tools() {
            assert!(names.contains(&core.name), "{} is missing", core.name);
        }
        for wave_one in [
            "browser_hover",
            "browser_drag",
            "browser_wait",
            "browser_dialog",
            "browser_upload",
            "browser_frames",
            "browser_batch",
            "browser_status",
        ] {
            assert!(names.contains(&wave_one), "{wave_one} is missing");
        }
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "one tool per name");
    }

    #[test]
    fn the_wave_one_tools_replace_the_core_ones_in_place() {
        let merged = tools();
        let described = |name: &str| merged.iter().find(|t| t.name == name).map(|t| t.description).unwrap_or_default();
        assert!(described("browser_click").contains("clickCount"));
        assert!(described("browser_screenshot").contains("fullPage"));
        assert!(described("browser_read_page").contains("format"));
        assert!(described("browser_find").contains("frame"));
        assert!(described("browser_tabs").contains("create"));
        // The core tools keep their place at the front of the list.
        let order: Vec<_> = merged.iter().take(13).map(|t| t.name).collect();
        let core_order: Vec<_> = core::tools().iter().map(|t| t.name).collect();
        assert_eq!(order, core_order);
    }
}
