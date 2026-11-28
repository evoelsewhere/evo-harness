//! Ready-made teams in the gallery (docs/features/evo-mates/spec.md, FR-75;
//! plan 3.10): a channel with its rule, lead and instructions, and mates made
//! from gallery roles. A team is previewed and imported as a team file is
//! ([`super::team_file`]), so it gets the same safe defaults (FR-21).

use serde::Serialize;
use serde_json::{json, Value};
use tauri::AppHandle;

use super::team_file::{TeamChannel, TeamFile, TeamMate};
use super::template;

/// A built-in team: gallery roles in a channel.
pub struct Team {
    pub id: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub reply_rule: &'static str,
    /// Gallery role ids; the first is the lead.
    pub roles: &'static [&'static str],
    pub instructions: &'static str,
}

pub const TEAMS: [Team; 4] = [
    Team {
        id: "content-studio",
        name: "Content studio",
        about: "Research, draft and edit posts, pages and newsletters.",
        reply_rule: "lead",
        roles: &["editor", "researcher", "writer"],
        instructions: "We make content for the person: posts, pages and newsletters. The Researcher checks facts and \
            finds sources, the Writer drafts, the Editor leads, edits and hands the final text to the person. Nothing is \
            published by a mate.",
    },
    Team {
        id: "launch-room",
        name: "Launch room",
        about: "Plan a launch, write its copy and track the numbers.",
        reply_rule: "auto",
        roles: &["planner", "writer", "analyst"],
        instructions: "We prepare and run a launch. The Planner leads: keeps the plan with dates and owners. The Writer \
            drafts announcements and pages; the Analyst sets the numbers to watch and reports them.",
    },
    Team {
        id: "sales-desk",
        name: "Sales desk",
        about: "Find leads, research them and write the outreach.",
        reply_rule: "lead",
        roles: &["sales", "researcher", "writer"],
        instructions: "We help the person sell. Sales leads and decides who to contact; the Researcher looks into each \
            lead; the Writer drafts emails. Every email is a draft for the person to send.",
    },
    Team {
        id: "personal-office",
        name: "Personal office",
        about: "Your week, your trips and your spending, in one room.",
        reply_rule: "auto",
        roles: &["assistant", "travel", "bookkeeper"],
        instructions: "We keep the person's own life in order: the Assistant leads and keeps the week, Travel plans \
            trips, the Bookkeeper keeps receipts and spending. Nobody books, buys or pays: the person does.",
    },
];

/// A team as a team file, its mates made from their gallery roles.
pub fn file_of(team: &Team) -> Result<TeamFile, String> {
    let mut mates = Vec::new();
    for id in team.roles {
        let role =
            template::find(id).ok_or_else(|| format!("the team {} names no role {id}", team.id))?;
        mates.push(TeamMate {
            name: role.name.clone(),
            role: role.role.clone(),
            avatar: role.avatar(),
            tools: role.tools.clone(),
            instructions: role.instructions.clone(),
            ..TeamMate::default()
        });
    }
    let lead = mates.first().map(|m| m.name.clone());
    Ok(TeamFile {
        channel: Some(TeamChannel {
            name: team.name.into(),
            reply_rule: Some(team.reply_rule.into()),
            lead,
            instructions: team
                .instructions
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        }),
        mates,
        unknown: Vec::new(),
    })
}

fn find(id: &str) -> Result<&'static Team, String> {
    TEAMS
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("No team \"{id}\"."))
}

/// What the gallery's Teams tab shows of a team.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub about: String,
    pub mates: Vec<Value>,
}

#[tauri::command]
pub fn mates_teams() -> Vec<TeamSummary> {
    TEAMS
        .iter()
        .filter_map(|team| {
            let file = file_of(team).ok()?;
            Some(TeamSummary {
                id: team.id.into(),
                name: team.name.into(),
                about: team.about.into(),
                mates: file
                    .mates
                    .iter()
                    .map(|m| json!({ "name": m.name, "role": m.role, "avatar": m.avatar }))
                    .collect(),
            })
        })
        .collect()
}

#[tauri::command]
pub fn team_preview_builtin(app: AppHandle, id: String) -> Result<Value, String> {
    Ok(super::team_file::preview_file(&app, &file_of(find(&id)?)?))
}

#[tauri::command]
pub async fn team_import_builtin(
    app: AppHandle,
    id: String,
    skip: Vec<String>,
) -> Result<Value, String> {
    let file = file_of(find(&id)?)?;
    super::team_file::import_file(app, file, skip).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_team_is_made_of_gallery_roles_with_its_lead_among_them() {
        assert!(TEAMS.len() >= 3);
        for team in &TEAMS {
            let file = file_of(team).unwrap();
            assert!(file.mates.len() >= 2, "{}", team.id);
            let channel = file.channel.as_ref().unwrap();
            let lead = channel.lead.as_deref().unwrap();
            assert!(file.mates.iter().any(|m| m.name == lead), "{}", team.id);
            assert!(
                evo_store::channels::REPLY_RULES.contains(&team.reply_rule),
                "{}",
                team.id
            );
            assert!(
                file.mates
                    .iter()
                    .all(|m| !m.instructions.is_empty() && m.approval_mode.is_none()),
                "{}",
                team.id
            );
            assert!(!channel.instructions.contains("  "), "{}", team.id);
        }
        assert_eq!(mates_teams().len(), TEAMS.len());
    }

    #[test]
    fn a_team_previews_with_the_apps_mode_like_a_file() {
        let file = file_of(&TEAMS[0]).unwrap();
        let preview = super::super::team_file::preview_value(&file, "ask", &[]);
        assert_eq!(preview["mates"].as_array().unwrap().len(), file.mates.len());
        assert!(preview["mates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["approvalMode"] == "ask"));
        assert_eq!(preview["channel"]["name"], "Content studio");
    }
}
