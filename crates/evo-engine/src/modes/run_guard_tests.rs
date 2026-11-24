//! Commands refused during a run ask only whether a run is active. The lane's
//! full snapshot also reads every entry's label, one storage read per entry:
//! on a long chat `configure_external_tools` took 16–22 s and the app gave up
//! on it after 15 s, so the chat ran without the session tools.
use super::*;
use evo_harness::error::SessionResult;
use evo_harness::session::{types::*, Clock, CounterIdGenerator, InMemorySessionStorage, Session};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FixedClock;
impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        42
    }
}

/// Storage that counts label reads; everything else goes to `inner`.
struct LabelProbe {
    inner: Arc<dyn SessionStorage>,
    label_reads: AtomicUsize,
}

#[async_trait::async_trait]
impl SessionStorage for LabelProbe {
    fn metadata(&self) -> SessionMetadata {
        self.inner.metadata()
    }
    fn is_ephemeral(&self) -> bool {
        self.inner.is_ephemeral()
    }
    async fn get_lanes(&self) -> SessionResult<Vec<LanePointer>> {
        self.inner.get_lanes().await
    }
    async fn create_lane(&self, lane: &str, at: Option<&str>) -> SessionResult<()> {
        self.inner.create_lane(lane, at).await
    }
    async fn move_lane(&self, lane: &str, to: Option<&str>) -> SessionResult<()> {
        self.inner.move_lane(lane, to).await
    }
    async fn append_progress(
        &self,
        records: Vec<AssistantFrameRecord>,
    ) -> SessionResult<Vec<AssistantFrameRecord>> {
        self.inner.append_progress(records).await
    }
    async fn read_progress(&self, run_id: &str) -> SessionResult<Vec<AssistantFrameRecord>> {
        self.inner.read_progress(run_id).await
    }
    async fn prune_completed_progress(&self) -> SessionResult<usize> {
        self.inner.prune_completed_progress().await
    }
    async fn append_entry(&self, entry: ProvisionedEntry, lane: &str) -> SessionResult<Entry> {
        self.inner.append_entry(entry, lane).await
    }
    async fn append_record(&self, record: LaneRecord) -> SessionResult<LaneRecord> {
        self.inner.append_record(record).await
    }
    async fn append_queue_cancellations(
        &self,
        records: Vec<LaneRecord>,
    ) -> SessionResult<Vec<LaneRecord>> {
        self.inner.append_queue_cancellations(records).await
    }
    async fn get_entry(&self, id: &str) -> SessionResult<Option<Entry>> {
        self.inner.get_entry(id).await
    }
    async fn find_entries(&self, query: &EntryQuery) -> SessionResult<Vec<Entry>> {
        self.inner.find_entries(query).await
    }
    async fn find_entries_on_branch(
        &self,
        query: &EntryQuery,
        bounds: &BranchBounds,
        start: &str,
    ) -> SessionResult<Vec<Entry>> {
        self.inner
            .find_entries_on_branch(query, bounds, start)
            .await
    }
    async fn find_records(&self, query: &RecordQuery) -> SessionResult<Vec<LaneRecord>> {
        self.inner.find_records(query).await
    }
    async fn record_page(&self, query: &RecordPageQuery) -> SessionResult<RecordPage> {
        self.inner.record_page(query).await
    }
    async fn find_open_operations(
        &self,
        lane: &str,
        limit: Option<usize>,
    ) -> SessionResult<Vec<OperationStartedRecord>> {
        self.inner.find_open_operations(lane, limit).await
    }
    async fn get_log(&self, options: &LogOptions) -> SessionResult<Vec<LogItem>> {
        self.inner.get_log(options).await
    }
    async fn get_name(&self) -> SessionResult<Option<String>> {
        self.inner.get_name().await
    }
    async fn set_name(&self, name: Option<&str>) -> SessionResult<()> {
        self.inner.set_name(name).await
    }
    async fn get_label(&self, id: &str) -> SessionResult<Option<String>> {
        self.label_reads.fetch_add(1, Ordering::Relaxed);
        self.inner.get_label(id).await
    }
    async fn set_label(&self, id: &str, label: Option<&str>) -> SessionResult<()> {
        self.inner.set_label(id, label).await
    }
    async fn get_stats(&self) -> SessionResult<SessionStats> {
        self.inner.get_stats().await
    }
    async fn drain(&self) {
        self.inner.drain().await
    }
}

/// A harness over a chat of `messages` user messages whose label reads are counted.
async fn chat(messages: usize) -> (AgentHarness, Arc<LabelProbe>) {
    let clock: Arc<dyn Clock> = Arc::new(FixedClock);
    let ids = Arc::new(CounterIdGenerator::new());
    let metadata = SessionMetadata {
        id: "run-guard".into(),
        created_at: 0,
        parent_session_id: None,
    };
    let inner: Arc<dyn SessionStorage> =
        Arc::new(InMemorySessionStorage::new(metadata, clock, ids));
    for index in 0..messages {
        let id = format!("message-{index}");
        inner
            .append_entry(
                ProvisionedEntry {
                    id: id.clone(),
                    kind: ProvisionedKind::Message {
                        message: evo_ai::UserMessage::new(id.as_str(), 42).into(),
                        terminate: None,
                    },
                },
                "main",
            )
            .await
            .unwrap();
    }
    let probe = Arc::new(LabelProbe {
        inner,
        label_reads: AtomicUsize::new(0),
    });
    let harness = AgentHarness::create(evo_harness::types::AgentHarnessOptions {
        session: Session::new(probe.clone(), None),
        ..Default::default()
    })
    .await
    .unwrap();
    (harness, probe)
}

#[tokio::test]
async fn the_probe_sees_the_snapshot_read_every_label() {
    let (harness, probe) = chat(50).await;
    harness.lane("main").snapshot().await.unwrap();
    assert!(probe.label_reads.load(Ordering::Relaxed) >= 50);
}

#[tokio::test]
async fn configure_external_tools_does_not_read_labels() {
    let (harness, probe) = chat(50).await;
    let lane = harness.lane("main");
    let (writer, _task, _) =
        crate::rpc::writer::start_rpc_writer(std::io::sink(), Default::default());
    let backend = crate::rpc::external_tools::ExternalTools::detached(writer);
    let mut old_names = Vec::new();
    rpc_configure_external_tools(
        &lane,
        &harness,
        false,
        &backend,
        &mut old_names,
        &json!({ "tools": [] }),
    )
    .await
    .unwrap();
    assert_eq!(probe.label_reads.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn configure_file_io_and_navigate_do_not_read_labels() {
    let (harness, probe) = chat(50).await;
    let lane = harness.lane("main");
    let (writer, _task, _) =
        crate::rpc::writer::start_rpc_writer(std::io::sink(), Default::default());
    // Whatever they answer: the run check before it must not read labels.
    let _ = rpc_configure_file_io(&lane, false, None, &mut None, &writer, &json!({})).await;
    let _ = rpc_navigate(&lane, &json!({ "targetId": "message-0" })).await;
    assert_eq!(probe.label_reads.load(Ordering::Relaxed), 0);
}
