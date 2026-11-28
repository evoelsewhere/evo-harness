//! When a channel job's retro runs: after the job ended and the channel
//! has been quiet for two minutes, if it had a hand-off (always after a
//! limit, a stop or a failure), or at once from **Run retro**; then its card
//! is posted in the channel, and the person is notified only after a limit,
//! a stop or a failure (docs/features/learning-layers/spec.md, FR-19, FR-21;
//! plan 3.5).

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// The channel must be quiet this long after a job before its retro (A-2).
pub const QUIET_MS: i64 = 2 * 60 * 1000;

/// A job waiting for its retro.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub job: String,
    pub status: String,
    pub handoffs: u64,
    /// The channel's last line.
    pub last_at: i64,
}

/// Jobs waiting for their retro, by channel root chat.
#[derive(Default)]
pub struct Retros(Mutex<HashMap<String, Pending>>);

impl Retros {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pending>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A job that gets a retro: one with a hand-off, and always one that hit a
/// limit, was stopped by the person or had a mate fail (FR-19).
pub fn wants_retro(status: &str, handoffs: u64) -> bool {
    handoffs >= 1 || matches!(status, "limit" | "stopped" | "failed")
}

/// The retro is due: the channel has been quiet for [`QUIET_MS`].
pub fn retro_due(pending: &Pending, now_ms: i64) -> bool {
    now_ms - pending.last_at >= QUIET_MS
}

/// Whether the person hears of the retro (FR-19).
pub fn notifies(status: &str) -> bool {
    matches!(status, "limit" | "stopped" | "failed")
}

/// A channel line went out (`channel_event`): a job's end waits for its
/// retro; any other line keeps the channel from being quiet.
pub fn on_channel_event(app: &AppHandle, chat: &str, entry: &Value) {
    let now = evo_store::chats::now_ms();
    let retros = app.state::<Retros>();
    let mut waiting = retros.lock();
    match entry.get("kind").and_then(Value::as_str) {
        Some("job") => {
            let status = entry.get("status").and_then(Value::as_str).unwrap_or("completed").to_string();
            let handoffs = entry.get("handoffs").and_then(Value::as_u64).unwrap_or(0);
            let job = entry.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            if wants_retro(&status, handoffs) && !job.is_empty() && skills_on(app) {
                waiting.insert(chat.to_string(), Pending { job, status, handoffs, last_at: now });
            }
        }
        Some("retro") => {}
        _ => {
            if let Some(pending) = waiting.get_mut(chat) {
                pending.last_at = now;
            }
        }
    }
}

/// **Learn skills automatically** in Evo Mates covers the retro (FR-23).
fn skills_on(app: &AppHandle) -> bool {
    let all = app.state::<Db>().with(|conn| evo_store::settings::all(conn)).unwrap_or_default();
    super::settings_from(&all).mates_skills
}

/// Every retro that is due now.
pub async fn run_due(app: &AppHandle) {
    let now = evo_store::chats::now_ms();
    let due: Vec<(String, Pending)> = {
        let retros = app.state::<Retros>();
        let mut waiting = retros.lock();
        let ready: Vec<String> = waiting.iter().filter(|(_, pending)| retro_due(pending, now)).map(|(chat, _)| chat.clone()).collect();
        ready.into_iter().filter_map(|chat| waiting.remove(&chat).map(|pending| (chat, pending))).collect()
    };
    for (chat, pending) in due {
        if let Err(error) = run(app, &chat, Some(&pending.job)).await {
            app.state::<Db>().log(evo_store::logs::NewLog::new("warn", "learning", "retro failed").chat(Some(&chat)).data(json!({ "error": error })));
        }
    }
}

/// The model a channel's retro runs on when the person set no Background
/// model: its lead's, as a mate's own steps run on the mate's model
/// (learning-layers FR-24; evo-mates A-13). `None`: the engine's own choice.
pub fn retro_model(settings: &serde_json::Map<String, Value>, lead_provider: Option<&str>, lead_model: Option<&str>) -> Option<(String, String)> {
    let set = settings
        .get(super::BACKGROUND_MODEL_KEY)
        .is_some_and(|value| ["provider", "modelId"].iter().all(|key| value.get(key).and_then(Value::as_str).is_some_and(|text| !text.is_empty())));
    if set {
        return None;
    }
    match (lead_provider.filter(|p| !p.is_empty()), lead_model.filter(|m| !m.is_empty())) {
        (Some(provider), Some(model)) => Some((provider.to_string(), model.to_string())),
        _ => None,
    }
}

/// [`retro_model`] for the channel whose root chat is `chat`.
fn lead_model(app: &AppHandle, chat: &str) -> Option<(String, String)> {
    app.state::<Db>()
        .with(|conn| {
            let settings = evo_store::settings::all(conn)?;
            let lead = evo_store::channels::channel_of_chat(conn, chat)?
                .and_then(|channel| channel.lead_mate_id)
                .map(|id| evo_store::mates::get_mate(conn, &id))
                .transpose()?
                .flatten();
            Ok(retro_model(&settings, lead.as_ref().and_then(|m| m.provider.as_deref()), lead.as_ref().and_then(|m| m.model.as_deref())))
        })
        .ok()
        .flatten()
}

/// Run the retro of `job` (the channel's last job for `None`) and post its card.
pub async fn run(app: &AppHandle, chat: &str, job: Option<&str>) -> Result<Value, String> {
    let mut extra = vec!["--chat".to_string(), chat.to_string()];
    if let Some(job) = job {
        extra.extend(["--job".to_string(), job.to_string()]);
    }
    if let Some((provider, model)) = lead_model(app, chat) {
        extra.extend(["--provider".to_string(), provider, "--model".to_string(), model]);
    }
    let result = super::jobs::run_step(app, "retro", "personal", extra).await?;
    if let Some(skipped) = result.get("skipped").and_then(Value::as_str) {
        return Err(format!("no retro: {skipped}"));
    }
    let card = result.get("card").cloned().ok_or("the retro made no card")?;
    post(app, chat, &card).await?;
    if card.get("status").and_then(Value::as_str).is_some_and(notifies) {
        crate::supervisor::notices::announce(app, chat.to_string(), crate::supervisor::notices::Event::Retro);
    }
    Ok(card)
}

/// Post the card through the channel's engine, started when it has none.
async fn post(app: &AppHandle, chat: &str, card: &Value) -> Result<(), String> {
    use crate::supervisor::{AppHost, EngineSupervisor, EnsureRequest, Host, Reason};
    let host = AppHost(app.clone());
    let running = app.state::<crate::engine::EngineState>().running().await.iter().any(|c| c == chat);
    if !running {
        let request = EnsureRequest { chat: chat.to_string(), cwd: None, options: None, restore: true, restart: false, reason: Reason::Learning };
        app.state::<EngineSupervisor>().ensure(&host, request).await?;
    }
    host.request(chat, "channel_post_retro", json!({ "card": card })).await.map(|_| ())
}

/// **Run retro** in a channel's menu: the retro of its last job, at once.
#[tauri::command]
pub async fn channel_run_retro(app: AppHandle, chat: String) -> Result<Value, String> {
    app.state::<Retros>().lock().remove(&chat);
    run(&app, &chat, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // learning-layers FR-24 for a channel: with no Background model the
    // retro runs on the lead's model (qa-2026-11-28, C4; evo-mates A-13).
    #[test]
    fn a_retro_with_no_background_model_runs_on_the_leads_model() {
        let settings = |value: Value| value.as_object().cloned().unwrap_or_default();
        let none = settings(json!({}));
        assert_eq!(retro_model(&none, Some("acme"), Some("small")), Some(("acme".into(), "small".into())));
        // The Background model wins: the engine reads it itself.
        let set = settings(json!({ "learning.backgroundModel": { "provider": "p", "modelId": "m" } }));
        assert_eq!(retro_model(&set, Some("acme"), Some("small")), None);
        // A lead with no model of its own, or no lead: nothing to add.
        assert_eq!(retro_model(&none, None, None), None);
        assert_eq!(retro_model(&none, Some("acme"), None), None);
    }

    // FR-19: two quiet minutes after a job worth a retro.
    #[test]
    fn a_retro_waits_for_two_quiet_minutes() {
        let pending = Pending { job: "j-1".into(), status: "completed".into(), handoffs: 3, last_at: 1_000_000 };
        assert!(!retro_due(&pending, 1_000_000 + 60_000));
        assert!(!retro_due(&pending, 1_000_000 + QUIET_MS - 1));
        assert!(retro_due(&pending, 1_000_000 + QUIET_MS));
        assert!(wants_retro("completed", 1));
        assert!(!wants_retro("completed", 0), "no hand-off, nothing went wrong: no retro");
        for status in ["limit", "stopped", "failed"] {
            assert!(wants_retro(status, 0), "{status}");
            assert!(notifies(status), "{status}");
        }
        assert!(!notifies("completed") && !notifies("silent"), "otherwise the card alone shows");
    }
}
