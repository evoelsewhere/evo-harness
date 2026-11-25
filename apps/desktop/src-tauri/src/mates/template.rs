//! Gallery roles (docs/features/evo-mates/spec.md, FR-8, FR-37; ux.md 5).
//!
//! One Markdown file per role in `templates/`, built into the app: a front
//! matter of `key: value` lines, then the role's instructions. The same
//! shape as a mate's section of a team file (FR-27), without the channel:
//!
//! ```markdown
//! ---
//! id: researcher
//! name: Researcher
//! role: Finds and checks facts, then sums them up
//! category: Research
//! description: Digs into a question and comes back with sources.
//! face: scout
//! hue: 205
//! tools: [read, web_search, web_fetch]
//! ---
//! You research questions for the person …
//! ```
//!
//! `tools` lists the tools the role suggests; `approval` a looser approval
//! mode the role suggests, shown on its card (spec 6.2). A model is never
//! named: a new mate takes the app's default model.

use serde::Serialize;
use serde_json::{json, Value};

/// The face of a mate with none chosen.
pub const DEFAULT_FACE: &str = "pebble";

const FILES: [&str; 8] = [
    include_str!("templates/assistant.md"),
    include_str!("templates/researcher.md"),
    include_str!("templates/writer.md"),
    include_str!("templates/developer.md"),
    include_str!("templates/analyst.md"),
    include_str!("templates/planner.md"),
    include_str!("templates/sales.md"),
    include_str!("templates/support.md"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub role: String,
    pub category: String,
    pub description: String,
    pub face: String,
    pub hue: u32,
    pub tools: Option<Vec<String>>,
    pub approval: Option<String>,
    pub instructions: String,
}

/// What the gallery shows of a role.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub role: String,
    pub category: String,
    pub description: String,
    pub avatar: Value,
    pub approval: Option<String>,
}

impl Template {
    pub fn avatar(&self) -> Value {
        json!({ "definition": self.face, "hue": self.hue })
    }

    pub fn summary(&self) -> Summary {
        Summary {
            id: self.id.clone(),
            name: self.name.clone(),
            role: self.role.clone(),
            category: self.category.clone(),
            description: self.description.clone(),
            avatar: self.avatar(),
            approval: self.approval.clone(),
        }
    }
}

/// A `[a, b]` list or one value.
fn list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Read one role file; `None` when it has no front matter, id or name.
pub fn parse(text: &str) -> Option<Template> {
    let text = text.replace("\r\n", "\n");
    let rest = text.strip_prefix("---\n")?;
    let (head, body) = rest.split_once("\n---\n")?;
    let mut template = Template {
        id: String::new(),
        name: String::new(),
        role: String::new(),
        category: "General".into(),
        description: String::new(),
        face: DEFAULT_FACE.into(),
        hue: 210,
        tools: None,
        approval: None,
        instructions: body.trim().to_string(),
    };
    for line in head.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim().to_string();
        match key.trim() {
            "id" => template.id = value,
            "name" => template.name = value,
            "role" => template.role = value,
            "category" => template.category = value,
            "description" => template.description = value,
            "face" => template.face = value,
            "hue" => template.hue = value.parse().unwrap_or(210) % 360,
            "tools" => template.tools = Some(list(&value)),
            "approval" => template.approval = Some(value),
            _ => {}
        }
    }
    (!template.id.is_empty() && !template.name.is_empty()).then_some(template)
}

/// Every gallery role, in the gallery's order.
pub fn all() -> Vec<Template> {
    FILES.iter().filter_map(|text| parse(text)).collect()
}

pub fn find(id: &str) -> Option<Template> {
    all().into_iter().find(|template| template.id == id)
}

/// A hue for a mate with no role, the same for the same id.
pub fn hue_of(id: &str) -> u32 {
    id.bytes().fold(7u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte.into())) % 360
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gallery_role_reads() {
        let roles = all();
        assert_eq!(roles.len(), FILES.len(), "a role file failed to parse");
        let mut ids: Vec<&str> = roles.iter().map(|r| r.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), roles.len(), "role ids are unique");
        for role in &roles {
            assert!(!role.role.is_empty() && !role.description.is_empty() && !role.instructions.is_empty(), "{}", role.id);
            assert!(role.approval.as_deref().is_none_or(|mode| ["ask", "auto-edits", "auto"].contains(&mode)), "{}", role.id);
        }
    }

    #[test]
    fn a_role_file_reads_its_front_matter_and_body() {
        let role = parse("---\nid: x\nname: X\nrole: Does x\ntools: [read, web_search]\nhue: 400\n---\nBe brief.\n").unwrap();
        assert_eq!(role.tools, Some(vec!["read".to_string(), "web_search".to_string()]));
        assert_eq!(role.hue, 40);
        assert_eq!(role.instructions, "Be brief.");
        assert!(parse("no front matter").is_none());
        assert!(parse("---\nname: X\n---\nbody").is_none(), "an id is needed");
    }
}
