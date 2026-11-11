//! Foreground calls: the id a result ends with and what `message_agent`
//! takes for it, a stopped agent that keeps its view and usage, and calls
//! that wait for a free slot.

use super::background::{node_by, script, text_of, until, user_texts};
use super::*;
use evo_ai::providers::faux::{faux_assistant_message, FauxBlock, FauxStep};
use evo_ai::types::{Context, Message, StopReason};

/// The `agent` and `message_agent` tools of `parent`, to call as the model would.
fn tools_of(
    parent: &AgentHarness,
    cwd: &std::path::Path,
) -> (Arc<dyn AgentTool>, Arc<dyn AgentTool>) {
    let cell: HarnessCell = Arc::new(OnceLock::new());
    let _ = cell.set(parent.downgrade());
    (
        create_agent_tool(cell.clone(), cwd.to_path_buf()),
        create_message_agent_tool(cell),
    )
}

fn noop() -> Update {
    Arc::new(|_| {})
}

/// The agent with this description (each test uses its own).
fn job_by(description: &str) -> Option<Arc<Job>> {
    lock(&registry().jobs)
        .iter()
        .find(|job| lock(&job.progress).description == description)
        .cloned()
}

/// Agents running now whose description starts with `prefix`.
fn running_named(prefix: &str) -> usize {
    lock(&registry().jobs)
        .iter()
        .filter(|job| {
            job.running.load(Ordering::Acquire)
                && lock(&job.progress).description.starts_with(prefix)
        })
        .count()
}

fn is_child(ctx: &Context) -> bool {
    user_texts(ctx)
        .first()
        .is_some_and(|u| u.contains("You are a subagent"))
}

fn tool_results(ctx: &Context) -> usize {
    ctx.messages
        .iter()
        .filter(|m| matches!(m, Message::ToolResult(_)))
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_foreground_result_names_the_agent_and_message_agent_finds_it() {
    let dir = tempfile::tempdir().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    // No run of the parent: the tools are called as its model would.
    let parent = parent(
        dir.path(),
        script(json!({}), log, false),
        Arc::new(Collector::default()),
    )
    .await;
    let (agent, message) = tools_of(&parent, dir.path());
    let start = |call: &'static str| {
        let agent = agent.clone();
        async move {
            agent
                .execute(
                    call,
                    json!({"description":"Named one","prompt":"Look around."}),
                    CancellationToken::new(),
                    noop(),
                )
                .await
                .unwrap()
        }
    };
    let first = start("c1").await;
    let id = first.details["child"]["agentId"]
        .as_str()
        .unwrap()
        .to_string();
    // The report, then a last line with the id.
    let text = text_of(&first);
    assert!(
        text.starts_with("background report: three call sites\n\n["),
        "{text}"
    );
    let last = text.lines().last().unwrap();
    assert!(
        last.contains(&id) && last.contains("message_agent"),
        "{text}"
    );

    // The id, `a_<id>`, the node address and the description all name it.
    let session = Address::session(&parent.session().storage().metadata().id);
    for wanted in [
        id.clone(),
        format!("a_{id}"),
        format!("{session}/a_{id}"),
        "named ONE".to_string(),
        format!(" `{id}` "),
    ] {
        let reply = message
            .execute(
                "c2",
                json!({"agentId": wanted, "message": "FOLLOW-UP: more"}),
                CancellationToken::new(),
                noop(),
            )
            .await
            .unwrap_or_else(|error| panic!("`{wanted}`: {error}"));
        assert!(
            text_of(&reply).starts_with("follow-up answer"),
            "{}",
            text_of(&reply)
        );
    }

    // A second agent of that description: the description names two.
    let second = start("c3").await;
    let second_id = second.details["child"]["agentId"].as_str().unwrap();
    let send = |wanted: String| {
        let message = message.clone();
        async move {
            message
                .execute(
                    "c4",
                    json!({"agentId": wanted, "message": "FOLLOW-UP: more"}),
                    CancellationToken::new(),
                    noop(),
                )
                .await
        }
    };
    let error = send("Named one".into()).await.unwrap_err().to_string();
    assert!(error.contains(&id) && error.contains(second_id), "{error}");
    // Another session's address is not this session's agent; the refusal
    // lists the ones there are.
    let error = send(format!("s_elsewhere/a_{id}"))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no subagent") && error.contains(&id) && error.contains("Named one"),
        "{error}"
    );
}

/// A parent whose foreground agent reads a file, then writes slowly.
fn slow_child(description: &'static str) -> FauxScript {
    let step = FauxStep::factory(move |ctx: &Context| {
        if is_child(ctx) {
            return if tool_results(ctx) == 0 {
                faux_assistant_message(
                    vec![FauxBlock::tool_call("read", json!({"path":"notes.md"}))],
                    StopReason::ToolUse,
                )
            } else {
                faux_assistant_message("partial finding ".repeat(400), StopReason::Stop)
            };
        }
        if tool_results(ctx) > 0 {
            return faux_assistant_message("ok", StopReason::Stop);
        }
        faux_assistant_message(
            vec![FauxBlock::tool_call(
                "agent",
                json!({"description":description,"prompt":"Survey the crate."}),
            )],
            StopReason::ToolUse,
        )
    });
    let script = FauxScript::new().with_tokens_per_second(60.0);
    script.append_responses(vec![step; 20]);
    script
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stopped_foreground_agent_keeps_its_view_and_usage() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.md"), "notes\n").unwrap();
    let events = Arc::new(Collector::default());
    let parent = parent(dir.path(), slow_child("fg-stop"), events.clone()).await;
    let run = {
        let parent = parent.clone();
        tokio::spawn(async move {
            parent
                .lane("main")
                .prompt_text("survey it", Vec::new())
                .await
        })
    };
    // It read a file and is writing its report.
    until(|| {
        job_by("fg-stop").is_some_and(|job| {
            let progress = lock(&job.progress);
            progress.tool_uses == 1 && progress.text.len() > 40
        })
    })
    .await;
    // The person stops the turn.
    let _ = parent.lane("main").abort().await;
    let _ = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the run ended");

    let (result, is_error) = agent_end(&events);
    assert!(is_error, "{result:?}");
    // The model reads the same error as before, with what the agent last said.
    let text = text_of(&result);
    let id = result.details["child"]["agentId"].as_str().unwrap();
    assert!(
        text.starts_with(&format!("tool error: subagent {id} stopped: cancelled")),
        "{text}"
    );
    assert!(
        text.contains("Its last words:") && text.contains("partial finding"),
        "{text}"
    );
    // The result keeps the agent's view and what this run used.
    let child = &result.details["child"];
    assert_eq!(child["status"], "cancelled");
    assert_eq!(child["tools"][0]["name"], "read");
    assert!(child["endedAt"].as_u64().is_some());
    let usage = result.usage.clone().expect("the run's usage");
    assert!(usage.total_tokens > 0, "{usage:?}");
    // And so does the saved message.
    let history = serde_json::to_string(
        &parent
            .session()
            .storage()
            .find_entries(&Default::default())
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(history.contains("\"status\":\"cancelled\""), "{history}");
    assert!(!job_by("fg-stop").unwrap().running.load(Ordering::Acquire));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_that_runs_out_of_turns_hands_over_what_it_said() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.md"), "notes\n").unwrap();
    let agents = dir.path().join(".evo/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("brief.md"),
        "---\ndescription: Two turns.\nmaxTurns: 2\n---\nBe brief.\n",
    )
    .unwrap();
    let read = || FauxBlock::tool_call("read", json!({"path":"notes.md"}));
    let step = FauxStep::factory(move |ctx: &Context| {
        if is_child(ctx) {
            return match tool_results(ctx) {
                0 => faux_assistant_message(
                    vec![FauxBlock::text("Found the parser in src/parse.rs."), read()],
                    StopReason::ToolUse,
                ),
                1 => faux_assistant_message(vec![read()], StopReason::ToolUse),
                _ => faux_assistant_message("a third turn", StopReason::Stop),
            };
        }
        if tool_results(ctx) > 0 {
            return faux_assistant_message("ok", StopReason::Stop);
        }
        faux_assistant_message(
            vec![FauxBlock::tool_call(
                "agent",
                json!({"description":"out-of-turns","prompt":"Find the parser.","agentType":"brief"}),
            )],
            StopReason::ToolUse,
        )
    });
    let script = FauxScript::new();
    script.append_responses(vec![step; 20]);
    let events = Arc::new(Collector::default());
    let parent = parent(dir.path(), script, events.clone()).await;
    parent
        .lane("main")
        .prompt_text("go", Vec::new())
        .await
        .unwrap();
    let (result, _) = agent_end(&events);
    let text = text_of(&result);
    assert!(!text.contains("a third turn"), "{text}");
    assert!(
        text.starts_with(
            "(the subagent returned no text) Its last words:\nFound the parser in src/parse.rs."
        ),
        "{text}"
    );
    assert_eq!(result.details["child"]["toolUses"], 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn six_foreground_agents_in_one_message_all_run_four_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let step = FauxStep::factory(move |ctx: &Context| {
        if is_child(ctx) {
            return faux_assistant_message("checked ".repeat(30), StopReason::Stop);
        }
        if tool_results(ctx) > 0 {
            return faux_assistant_message("ok", StopReason::Stop);
        }
        let calls: Vec<FauxBlock> = (0..6)
            .map(|n| {
                FauxBlock::tool_call(
                    "agent",
                    json!({"description":format!("six-{n}"),"prompt":"Check."}),
                )
            })
            .collect();
        faux_assistant_message(calls, StopReason::ToolUse)
    });
    let script = FauxScript::new().with_tokens_per_second(150.0);
    script.append_responses(vec![step; 30]);
    let events = Arc::new(Collector::default());
    let parent = parent(dir.path(), script, events.clone()).await;
    // How many run at once, sampled while the turn goes on.
    let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let watch = {
        let peak = peak.clone();
        tokio::spawn(async move {
            loop {
                peak.fetch_max(running_named("six-"), Ordering::AcqRel);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    };
    parent
        .lane("main")
        .prompt_text("check six things", Vec::new())
        .await
        .unwrap();
    watch.abort();

    let ends: Vec<(String, bool)> = events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolExecutionEnd {
                tool_name,
                result,
                is_error,
                ..
            } if tool_name == "agent" => Some((text_of(result), *is_error)),
            _ => None,
        })
        .collect();
    assert_eq!(ends.len(), 6, "{ends:?}");
    assert!(ends.iter().all(|(_, is_error)| !is_error), "{ends:?}");
    let peak = peak.load(Ordering::Acquire);
    assert!((2..=MAX_RUNNING).contains(&peak), "{peak} ran at once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_foreground_call_waits_for_a_slot_and_a_background_one_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    // Slow children: they hold their slots for the whole test.
    let parent = parent(
        dir.path(),
        script(json!({}), log, true),
        Arc::new(Collector::default()),
    )
    .await;
    let (agent, _) = tools_of(&parent, dir.path());
    let call = |description: String, background: bool, signal: CancellationToken| {
        let agent = agent.clone();
        async move {
            agent
                .execute(
                    "c",
                    json!({"description":description,"prompt":"Write a lot.","background":background}),
                    signal,
                    noop(),
                )
                .await
        }
    };
    let mut ids = Vec::new();
    for n in 0..MAX_RUNNING {
        let started = call(format!("hold-{n}"), true, CancellationToken::new())
            .await
            .unwrap();
        ids.push(
            started.details["child"]["agentId"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    // A fifth in the background is refused.
    let error = call("hold-bg".into(), true, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("at most 4"), "{error}");

    // A fifth in the foreground waits, and ends at once when its call is cancelled.
    let signal = CancellationToken::new();
    let waiting = tokio::spawn(call("hold-cancelled".into(), false, signal.clone()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!waiting.is_finished(), "it waits for a free slot");
    assert!(job_by("hold-cancelled").is_none(), "nothing was started");
    signal.cancel();
    let error = tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .expect("it ended at once")
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("slot"), "{error}");

    // A steering message ends the wait too: the parent's run has to read it.
    let waiting = tokio::spawn(call("hold-steered".into(), false, CancellationToken::new()));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!waiting.is_finished());
    steered(&parent.session().storage().metadata().id);
    let error = tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .expect("it ended at once")
        .unwrap()
        .unwrap_err();
    assert!(
        error.to_string().contains("a new message arrived"),
        "{error}"
    );
    assert!(job_by("hold-steered").is_none());

    // One that keeps waiting starts when a slot frees.
    let signal = CancellationToken::new();
    let waiting = tokio::spawn(call("hold-next".into(), false, signal.clone()));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(job_by("hold-next").is_none());
    assert!(registry().cancel(&ids[0]));
    until(|| node_by("hold-next").is_some_and(|n| n.state == crate::mesh::NodeState::Running))
        .await;
    assert!(running_named("hold-") <= MAX_RUNNING);

    // Stop them all.
    signal.cancel();
    for id in &ids[1..] {
        registry().cancel(id);
    }
    let stopped = tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("it stopped")
        .unwrap();
    assert!(stopped.is_err());
    until(|| running_named("hold-") == 0).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_call_whose_session_is_gone_does_not_leave_the_agent_running() {
    let dir = tempfile::tempdir().unwrap();
    let quick = || script(json!({}), Arc::new(Mutex::new(Vec::new())), false);
    let parent = parent(dir.path(), quick(), Arc::new(Collector::default())).await;
    let (agent, _) = tools_of(&parent, dir.path());
    agent
        .execute(
            "c1",
            json!({"description":"orphan","prompt":"Look around."}),
            CancellationToken::new(),
            noop(),
        )
        .await
        .unwrap();
    let job = job_by("orphan").unwrap();
    // The same agent, marked running for a session that no longer exists.
    let gone = super::parent(dir.path(), quick(), Arc::new(Collector::default())).await;
    let weak = gone.downgrade();
    drop(gone);
    assert!(weak.upgrade().is_none());
    let orphan = Arc::new(Job {
        id: "orphan-2".into(),
        address: job.address.clone(),
        parent: weak,
        session_id: job.session_id.clone(),
        call_id: Mutex::new("c2".into()),
        progress: job.progress.clone(),
        update: job.update.clone(),
        child: Mutex::new(lock(&job.child).clone()),
        cancel: Mutex::new(CancellationToken::new()),
        running: AtomicBool::new(true),
        background: AtomicBool::new(false),
        quiet: AtomicBool::new(false),
        worktree: Mutex::new(None),
        cwd: dir.path().to_path_buf(),
        last_used: Mutex::new(Instant::now()),
        slot: Mutex::new(Some(slot_now(&job.session_id).unwrap())),
    });
    let error = drive(
        orphan.clone(),
        "more".into(),
        false,
        CancellationToken::new(),
        CancellationToken::new(),
        noop(),
    )
    .await
    .unwrap_err();
    let AgentError::StructuredTool(result) = error else {
        panic!("{error}")
    };
    assert!(
        text_of(&result).contains("the session is gone"),
        "{result:?}"
    );
    assert_eq!(result.details["child"]["status"], "error");
    assert!(!orphan.running.load(Ordering::Acquire));
    assert!(lock(&orphan.slot).is_none(), "its slot is free again");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_questions_to_an_idle_agent_start_one_run() {
    let dir = tempfile::tempdir().unwrap();
    // The agent answers its brief at once and a question slowly.
    let step = FauxStep::factory(move |ctx: &Context| {
        let asked = user_texts(ctx).iter().any(|u| u.contains("QUESTION"));
        faux_assistant_message(
            if asked {
                "thinking it over ".repeat(40)
            } else {
                "ok".to_string()
            },
            StopReason::Stop,
        )
    });
    let script = FauxScript::new().with_tokens_per_second(20.0);
    script.append_responses(vec![step; 10]);
    let parent = parent(dir.path(), script, Arc::new(Collector::default())).await;
    let (agent, _) = tools_of(&parent, dir.path());
    agent
        .execute(
            "c1",
            json!({"description":"asked-twice","prompt":"Wait for questions."}),
            CancellationToken::new(),
            noop(),
        )
        .await
        .unwrap();
    let job = job_by("asked-twice").unwrap();
    let ask = |n: usize| {
        let address = job.address.clone();
        tokio::spawn(async move { deliver(&address, format!("QUESTION {n}"), true).await })
    };
    let (first, second) = (ask(1), ask(2));
    let mut how = [
        first.await.unwrap().unwrap(),
        second.await.unwrap().unwrap(),
    ];
    how.sort();
    assert_eq!(
        how,
        [
            "delivered into its current run",
            "it was continued to answer"
        ]
    );
    assert!(registry().cancel(&job.id));
    until(|| !job.running.load(Ordering::Acquire)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_session_keeps_its_agents_whatever_another_session_starts() {
    let dir = tempfile::tempdir().unwrap();
    let quick = || script(json!({}), Arc::new(Mutex::new(Vec::new())), false);
    let start = |agent: Arc<dyn AgentTool>, description: String| async move {
        let result = agent
            .execute(
                "c",
                json!({"description":description,"prompt":"Look around."}),
                CancellationToken::new(),
                noop(),
            )
            .await
            .unwrap();
        result.details["child"]["agentId"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let one = parent(dir.path(), quick(), Arc::new(Collector::default())).await;
    let other = super::parent(dir.path(), quick(), Arc::new(Collector::default())).await;
    let session = |harness: &AgentHarness| harness.session().storage().metadata().id.clone();
    let kept = start(tools_of(&one, dir.path()).0, "kept-one".into()).await;
    // The other session goes one past what a session keeps.
    let (agent, _) = tools_of(&other, dir.path());
    let mut ids = Vec::new();
    for n in 0..=MAX_KEPT {
        ids.push(start(agent.clone(), format!("kept-other-{n}")).await);
    }
    // Its own oldest was closed; the first session's agent was not touched.
    assert!(registry().find(&session(&other), &ids[0]).is_err());
    assert!(registry().find(&session(&other), &ids[MAX_KEPT]).is_ok());
    assert!(registry().find(&session(&one), &kept).is_ok());
}
