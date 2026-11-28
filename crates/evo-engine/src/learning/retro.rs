//! The retro after a channel job: what went well, what went wrong, and for
//! each mate that took part at most two lessons, which go into its learned
//! skills through the skill review's rules
//! (docs/features/learning-layers/spec.md, FR-19 – FR-21; plan 3.5).
//!
//! It reads the job's lines with their authors and kinds (asks, assigns,
//! tells), the person's messages and any notice. A lesson about how the team
//! hands off goes to the lead. The card it returns is posted in the channel
//! by the shell; it is not a message and wakes nobody.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::run::{self, Owner, Step, Task};

pub const LESSONS_PER_MATE: usize = 2;
pub const MAX_POINTS: usize = 5;

/// A member of the job, by mate id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub id: String,
    pub name: String,
}

/// The job a retro reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: String,
    pub status: String,
    pub handoffs: u64,
    /// The job's lines as text, oldest first.
    pub text: String,
    /// The person's message that started it (where the card's lessons anchor).
    pub message: String,
    pub members: Vec<Member>,
}

/// The job `job_id` (the last one when `None`) out of a channel's lines.
pub fn job_of(lines: &[Value], job_id: Option<&str>) -> Option<Job> {
    let at = lines.iter().rposition(|line| {
        line.get("kind").and_then(Value::as_str) == Some("job")
            && job_id.is_none_or(|id| line.get("id").and_then(Value::as_str) == Some(id))
    })?;
    let job = &lines[at];
    let message = job.get("message").and_then(Value::as_str)?.to_string();
    let start = lines[..at]
        .iter()
        .rposition(|line| line.get("id").and_then(Value::as_str) == Some(message.as_str()))
        .unwrap_or(0);
    // A late report after the job's end belongs to it, up to the next message of the person.
    let end = lines[at + 1..]
        .iter()
        .position(|line| {
            line.get("kind").and_then(Value::as_str) == Some("message")
                && line.get("author").and_then(Value::as_str) == Some("person")
        })
        .map_or(lines.len(), |next| at + 1 + next);
    let mut members: BTreeMap<String, String> = BTreeMap::new();
    let mut text = Vec::new();
    for line in lines[start..end]
        .iter()
        .filter(|line| line.get("kind").and_then(Value::as_str) != Some("job"))
    {
        let get = |key: &str| {
            line.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string()
        };
        match line.get("kind").and_then(Value::as_str) {
            Some("message") if get("author") == "person" => {
                text.push(format!("The person: {}", get("text")))
            }
            Some("message") => {
                let name = if get("name").is_empty() {
                    get("author")
                } else {
                    get("name")
                };
                members.insert(get("author"), name.clone());
                let failed = line
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(|status| status != "done");
                text.push(format!(
                    "{name} ({}){}: {}",
                    get("author"),
                    if failed { " [failed]" } else { "" },
                    get("text")
                ));
            }
            Some("handoff") => {
                members.insert(get("from"), get("fromName"));
                members.insert(get("to"), get("toName"));
                text.push(format!(
                    "{} {} {}: {}",
                    get("fromName"),
                    get("handoff"),
                    get("toName"),
                    get("text")
                ));
            }
            Some("notice") => text.push(format!("[notice: {}] {}", get("notice"), get("text"))),
            Some("route") => text.push(format!("[route] {}", get("line"))),
            _ => {}
        }
    }
    members.remove("");
    Some(Job {
        id: job
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        status: job
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("completed")
            .to_string(),
        handoffs: job.get("handoffs").and_then(Value::as_u64).unwrap_or(0),
        text: text.join("\n"),
        message,
        members: members
            .into_iter()
            .map(|(id, name)| Member { id, name })
            .collect(),
    })
}

pub const SYSTEM: &str = "You run a short retrospective of one job a team of AI mates did \
for a person in a channel. You get the job's lines with their authors and kinds (asks, \
assigns, tells), the person's messages and any notice (a limit, a stop, a failure). Say what \
went well and what went wrong, in short plain sentences, and give each mate that took part at \
most two lessons about how it should work in this team next time (handing off, asking, \
reporting back, scope). A lesson about how the team hands off goes to the lead. A lesson is \
for a kind of work, never one task's details; no lessons when nothing went wrong.\n\n\
Answer with one JSON object: {\"well\":[\"…\"],\"wrong\":[\"…\"],\"lessons\":[{\"mate\":\"<mate id>\",\
\"text\":\"…\"}]}. Use the mate ids shown in brackets; \"team\" for the lead.";

pub fn prompt(job: &Job, lead: Option<&str>) -> String {
    let members = job
        .members
        .iter()
        .map(|m| {
            format!(
                "- {} ({}){}",
                m.name,
                m.id,
                if Some(m.id.as_str()) == lead {
                    ", the lead"
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{}\n\nThe job ended: {} after {} hand-offs.\n\n{}\n\n{}",
        run::DATA_RULE,
        job.status,
        job.handoffs,
        run::fenced("members", &members),
        run::fenced("job_lines", &job.text)
    )
}

/// What a retro's answer holds, checked: points cut to a few, at most two
/// lessons per mate, only mates of the job, team lessons to the lead.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Retro {
    pub well: Vec<String>,
    pub wrong: Vec<String>,
    /// mate id → lessons.
    pub lessons: BTreeMap<String, Vec<String>>,
}

pub fn check(answer: &Value, job: &Job, lead: Option<&str>) -> Retro {
    let points = |key: &str| -> Vec<String> {
        answer
            .get(key)
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(|text| {
                        super::redact::redact(
                            &text.split_whitespace().collect::<Vec<_>>().join(" "),
                        )
                    })
                    .filter(|text| !text.is_empty())
                    .map(|text| text.chars().take(300).collect())
                    .take(MAX_POINTS)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut lessons: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for lesson in answer
        .get("lessons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let text = lesson
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let named = lesson.get("mate").and_then(Value::as_str).unwrap_or("");
        let mate = if named == "team" || named == "lead" {
            lead.unwrap_or("")
        } else {
            named
        };
        let known =
            job.members.iter().any(|m| m.id == mate) || (Some(mate) == lead && !mate.is_empty());
        if text.is_empty() || !known || super::redact::has_secret(&text) {
            continue;
        }
        let list = lessons.entry(mate.to_string()).or_default();
        if list.len() < LESSONS_PER_MATE {
            list.push(text.chars().take(400).collect());
        }
    }
    Retro {
        well: points("well"),
        wrong: points("wrong"),
        lessons,
    }
}

/// A mate learns skills: the Mates switch and its own are not off (FR-23, FR-25).
fn mate_learns(mate: &str) -> bool {
    let Ok(conn) = crate::app_db::open() else {
        return true;
    };
    let get = |key: &str| crate::app_db::settings::get(&conn, key).ok().flatten();
    get("learning.mates.skills").and_then(|value| value.as_bool()) != Some(false)
        && get(&format!("learning.mate.{mate}"))
            .and_then(|value| value.get("skills").and_then(Value::as_bool))
            != Some(false)
}

/// The channel's lead, from `evo.db`.
fn lead_of(channel_chat: &str) -> Option<String> {
    let conn = crate::app_db::open().ok()?;
    conn.query_row(
        "SELECT lead_mate_id FROM channels WHERE chat_id = ?1",
        [channel_chat],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

/// Run the retro for the step's channel.
pub async fn run(step: &Step<'_>) -> Value {
    let Some(chat) = step.args.learn_chat.as_deref() else {
        return run::result_line(
            Task::Retro,
            &step.owner.id,
            json!({ "error": "`--chat` is required" }),
        );
    };
    let state = crate::app_db::transcripts::path(chat)
        .ok()
        .and_then(|path| {
            crate::app_db::transcripts::read_state(&path, chat)
                .ok()
                .flatten()
        })
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let lines = state
        .as_ref()
        .map(super::channels::channel_lines)
        .unwrap_or_default();
    let Some(job) = job_of(&lines, step.args.learn_job.as_deref()) else {
        return run::skipped(Task::Retro, &step.owner.id, "no job");
    };
    retro_with(step, chat, &job, lead_of(chat).as_deref()).await
}

/// The retro over a job already read.
pub async fn retro_with(step: &Step<'_>, chat: &str, job: &Job, lead: Option<&str>) -> Value {
    let owner = format!("channel:{chat}");
    if step.model.is_none() {
        return run::skipped(Task::Retro, &owner, "no model");
    }
    let answer = match step.ask(SYSTEM, prompt(job, lead)).await {
        Ok(text) => text,
        Err(error) => return run::result_line(Task::Retro, &owner, json!({ "error": error })),
    };
    let Some(answer) = run::json_answer(&answer) else {
        return run::result_line(
            Task::Retro,
            &owner,
            json!({ "error": "the answer was not valid" }),
        );
    };
    let retro = check(&answer, job, lead);
    let agent = crate::config::agent_dir().unwrap_or_default();
    let mut card_lessons = Vec::new();
    let mut changed = Vec::new();
    let mut errors = Vec::new();
    for (mate, lessons) in &retro.lessons {
        let Ok(mate_owner) = Owner::parse(&format!("mate:{mate}"), &agent) else {
            continue;
        };
        // The mate's own switch for learning skills (FR-25) is the shell's;
        // here the lessons go through the review's rules (FR-20).
        if !mate_learns(mate) {
            let name = job
                .members
                .iter()
                .find(|m| &m.id == mate)
                .map_or(mate.clone(), |m| m.name.clone());
            for lesson in lessons {
                card_lessons.push(json!({ "mate": mate, "name": name, "text": lesson, "skill": null, "row": null }));
            }
            continue;
        }
        let mate_step = Step {
            task: Task::Retro,
            owner: mate_owner,
            args: step.args,
            today: step.today.clone(),
            model: step.model.clone(),
        };
        let conversation = format!(
            "Lessons for you from the retro of a channel job ({} after {} hand-offs):\n{}",
            job.status,
            job.handoffs,
            lessons
                .iter()
                .map(|lesson| format!("- {lesson}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let result = super::review::review_with(
            &mate_step,
            chat,
            Some(&job.message),
            &conversation,
            Task::Retro,
        )
        .await;
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            errors.push(format!("{mate}: {error}"));
        }
        let name = job
            .members
            .iter()
            .find(|m| &m.id == mate)
            .map_or(mate.clone(), |m| m.name.clone());
        let rows = result
            .get("changed")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (index, lesson) in lessons.iter().enumerate() {
            let row = rows.get(index).or(rows.first());
            card_lessons.push(json!({
                "mate": mate, "name": name, "text": lesson,
                "skill": row.and_then(|row| row.get("name")).cloned(),
                "row": row.and_then(|row| row.get("row")).cloned(),
            }));
        }
        changed.extend(rows);
    }
    let card = json!({
        "job": job.id, "status": job.status, "handoffs": job.handoffs,
        "well": retro.well, "wrong": retro.wrong, "lessons": card_lessons,
        // The person hears of it only after a limit, a stop or a failure (FR-19).
        "notify": matches!(job.status.as_str(), "limit" | "stopped" | "failed"),
    });
    run::result_line(
        Task::Retro,
        &owner,
        json!({ "changed": changed, "card": card, "error": (!errors.is_empty()).then(|| errors.join("; ")) }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Vec<Value> {
        vec![
            json!({ "kind": "message", "id": "m-0", "author": "person", "text": "Earlier job.", "at": 1 }),
            json!({ "kind": "job", "id": "j-0", "message": "m-0", "status": "completed", "handoffs": 0 }),
            json!({ "kind": "message", "id": "m-1", "author": "person", "text": "Research and draft the launch post.", "at": 2 }),
            json!({ "kind": "route", "message": "m-1", "picked": ["lead"], "line": "Planner answers" }),
            json!({ "kind": "handoff", "handoff": "asks", "from": "lead", "fromName": "Planner", "to": "w1", "toName": "Writer", "text": "Who is the audience?" }),
            json!({ "kind": "handoff", "handoff": "asks", "from": "w1", "fromName": "Writer", "to": "lead", "toName": "Planner", "text": "Who is the audience?" }),
            json!({ "kind": "notice", "notice": "limit", "text": "Every mate stopped." }),
            json!({ "kind": "job", "id": "j-1", "message": "m-1", "status": "limit", "handoffs": 12 }),
            json!({ "kind": "message", "id": "r-9", "author": "w1", "name": "Writer", "text": "Late report: draft attached.", "status": "done" }),
            json!({ "kind": "message", "id": "m-2", "author": "person", "text": "Next job.", "at": 9 }),
        ]
    }

    // FR-20: the job's lines with their authors and kinds.
    #[test]
    fn retro_reads_the_jobs_lines() {
        let job = job_of(&lines(), None).unwrap();
        assert_eq!(
            (
                job.id.as_str(),
                job.status.as_str(),
                job.handoffs,
                job.message.as_str()
            ),
            ("j-1", "limit", 12, "m-1")
        );
        assert!(job
            .text
            .starts_with("The person: Research and draft the launch post."));
        assert!(job
            .text
            .contains("Planner asks Writer: Who is the audience?"));
        assert!(job.text.contains("[notice: limit] Every mate stopped."));
        assert!(
            job.text.contains("Late report: draft attached."),
            "a late report belongs to the job"
        );
        assert!(!job.text.contains("Earlier job") && !job.text.contains("Next job"));
        assert_eq!(
            job.members
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["lead", "w1"]
        );
        assert_eq!(
            job_of(&lines(), Some("j-0")).unwrap().text,
            "The person: Earlier job."
        );
        assert!(job_of(&lines()[..1], None).is_none());
        assert!(prompt(&job, Some("lead")).contains("- Planner (lead), the lead"));
    }

    // FR-20: at most two lessons per mate; team lessons to the lead; only members.
    #[test]
    fn retro_keeps_two_lessons_per_mate() {
        let job = job_of(&lines(), None).unwrap();
        let answer = json!({
            "well": ["The writer drafted fast."],
            "wrong": ["Planner and Writer asked each other the same question twice."],
            "lessons": [
                { "mate": "w1", "text": "Answer a question before asking it back." },
                { "mate": "w1", "text": "Report back once the draft is done." },
                { "mate": "w1", "text": "A third lesson." },
                { "mate": "team", "text": "Agree on the audience before handing off." },
                { "mate": "stranger", "text": "Not in this job." },
                { "mate": "lead", "text": "token=abc123def456ghi" }
            ]
        });
        let retro = check(&answer, &job, Some("lead"));
        assert_eq!(retro.lessons["w1"].len(), LESSONS_PER_MATE);
        assert_eq!(
            retro.lessons["lead"],
            ["Agree on the audience before handing off."]
        );
        assert!(!retro.lessons.contains_key("stranger"));
        assert_eq!(retro.wrong.len(), 1);
    }
}
