# Connected apps — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-28 |

## 1. Phases

### Phase 1 — Key, session and the connector

Done when: with a saved key, chats in both apps get the Connected apps
connector's meta-tools; with none, no request goes to Composio.

- [ ] T1 [FR-7] Pin Composio's shapes: record redacted fixtures of `POST /tool_router/session`, `tools/list` of the session MCP (meta-tool schemas, `MULTI_EXECUTE` arguments), `toolkits`, `link`, `connected_accounts` from a live run with the owner's key; a fake Composio server for tests — `apps/desktop/src-tauri/src/connected_apps/testdata/`, `connected_apps/fake.rs` — check: fixtures hold no key, user id or email
- [x] T2 [FR-7, NFR-1] Failing pool tests then `AppServer` slot: one code-supplied server, permissions and cached tools saved under `appServers`, its config and headers never written — `crates/evo-mcp/src/pool.rs` — check: `cargo test -p evo-mcp app_server`
- [x] T3 [FR-1, NFR-1, NFR-4] Key in the keychain, check with Composio before saving, remove; no request without a key — `connected_apps/{key,composio,mod}.rs` — check: shell tests `a_refused_key_is_not_saved`, `no_key_no_request` (fake server)
- [x] T4 [FR-7, FR-17, FR-18] Session create or reuse, `set_app_server`, re-send tools; key removed or changed clears everything; earlier user reused when exactly one — `connected_apps/mod.rs` — check: shell tests `a_restart_keeps_the_session`, `a_new_key_starts_clean`
- [ ] T5 [FR-1, FR-2] Key card UI with the Composio note — `apps/desktop/src/features/connected-apps/key-card.tsx` — check: manual

### Phase 2 — Catalogue and accounts

Done when: the person connects two labelled Gmail accounts, reconnects and
disconnects one, in both apps.

- [x] T6 [FR-4, FR-5] Failing tests then label rules: unique, required once active, dead label reused after removing its account, five usable — `connected_apps/accounts.rs` — check: `cargo test` shell `label_rules`
- [x] T7 [FR-3 – FR-6] Commands catalog, connect (browser), reopen, poll, disconnect with revoke, reconnect; `apps://changed` — `connected_apps/{catalog,accounts,mod}.rs`, `lib.rs` — check: shell tests against the fake server
- [x] T8 [FR-16] No managed sign-in: enable the person's newest enabled auth config for Tool Router, patch the session, retry once; else the steps — `connected_apps/accounts.rs`, `errors.rs` — check: shell test `a_toolkit_without_managed_auth_uses_the_persons_config`
- [ ] T9 [FR-3 – FR-6, NFR-3] Connected apps page (catalogue with search and categories, Connected tab, Connect dialog, Add account, Reconnect, Disconnect, pending link), placed in Customize, the More menu, Mates Settings and the mate's Apps tab — `features/connected-apps/`, `features/customize/customize-page.tsx`, `features/shell/sidebar-items.ts`, `features/settings/`, `features/mates/mate-panel.tsx` — check: manual, timed catalogue open

### Phase 3 — The gate

Done when: every call to an app tool is checked for account, grant, class,
schema and files before it leaves Evo.

- [x] T10 [FR-8, FR-9] Failing tests then `gate.rs`: parse `MULTI_EXECUTE` (unknown shape refused), account required with several, schema check from cached schemas with the "shape, not access" text — `connected_apps/gate.rs` — check: shell tests `an_account_is_required_with_two`, `a_bad_argument_is_refused_before_sending`
- [x] T11 [FR-13] Class read/send and the cards: read verbs or Composio's read-only mark → read; else sending through the existing ask and `mate_gate`, card with service, account, arguments — `gate.rs`, `agent_tools/mcp.rs`, `mates/grants.rs` — check: shell test `a_send_tool_asks_under_ask_every_time`
- [x] T12 [FR-7] Meta-tool policy: search, schemas, wait pass; workbench, bash, proxy Block by default and never for a mate with limited apps — `gate.rs`, `pool.rs` defaults — check: shell test `the_sandbox_is_closed_to_a_limited_mate`
- [x] T13 [FR-14] Failing tests then `files.rs`: allowed roots, hidden components, Evo data, 10 files, listed on the card, uploaded after the last check — `connected_apps/files.rs` — check: shell tests `a_hidden_file_is_never_uploaded`, `files_upload_only_after_approval`
- [x] T14 [FR-15] Scope refusal note — `connected_apps/errors.rs` — check: shell test with a fixture 403

### Phase 4 — Mates and Harness

Done when: mate grants hold for connected services and accounts; the
Connect card works; Harness chats switch them per chat.

- [x] T15 [FR-10] Failing tests then mate grants for connected services: tools offered, search results filtered, calls to other services or accounts refused; new services reach All, not Only these; template and team-file defaults — `mates/mod.rs`, `gate.rs`, `crates/evo-store/src/mates.rs` — check: shell tests `a_mate_on_only_these_cannot_use_personal`, `a_new_service_reaches_mates_on_all`
- [ ] T16 [FR-10] Mate panel Apps tab: per service All tools / These tools / None and Accounts this mate may use — `features/mates/mate-panel.tsx` — check: manual
- [ ] T17 [FR-11] `connect_app` standing and flow for connected services; `MANAGE_CONNECTIONS` turned into the card; the mate goes on after sign-in — `agent_tools/mates.rs`, `gate.rs` — check: shell test `manage_connections_shows_the_connect_card`; manual
- [x] T18 [FR-12] Harness `+` menu switch and project switch for Connected apps — `features/chat/composer-*`, `agent_tools/mcp.rs` — check: shell test `connected_apps_off_in_a_chat_offers_none`
- [x] T19 [FR-19] Transcript label "Called <service> · <tool> (<account>)" — `components/ai-elements/tool-labels.ts` — check: UI unit test

### Phase 5 — Verify and docs

- [ ] T20 Manual end to end with the owner's key on a scratch agent dir: connect Gmail twice, a mate on Only these reads "work" and is refused "personal", a send asks, a file from `~/.ssh` is refused — check: recorded in section 2
- [x] T21 `apps/desktop/README.md` UI map; Evo Mates spec FR-39/FR-40 History lines pointing here — check: review
- [ ] T22 Run every check in section 2 and record the results — check: table filled

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `a_refused_key_is_not_saved`; manual | passed (2026-11-28): test; in the e2e app a `uak_` key is refused before any request, a bogus `ak_` key is refused by Composio with its reason and not saved |
| FR-2 | review of the key card text | passed (2026-11-28): the note shows on the key card in Customize, Settings › Evo Mates and the Mates dialog |
| FR-3 | manual placements | not run |
| FR-4 | `label_rules`; fake-server connect test; manual | not run |
| FR-5 | `label_rules` | passed (2026-11-28) |
| FR-6 | fake-server disconnect and reconnect tests | passed (2026-11-28): `disconnect_revokes_one_and_reconnect_keeps_the_label` |
| FR-7 | `app_server` pool tests; `the_sandbox_is_closed_to_a_limited_mate` | not run |
| FR-8 | `an_account_is_required_with_two` | passed (2026-11-28) |
| FR-9 | `a_bad_argument_is_refused_before_sending` | passed (2026-11-28) |
| FR-10 | `a_mate_on_only_these_cannot_use_personal`; `a_new_service_reaches_mates_on_all`; manual | not run |
| FR-11 | `manage_connections_shows_the_connect_card`; manual | not run |
| FR-12 | `connected_apps_off_in_a_chat_offers_none` | passed (2026-11-28); also `connected_apps_off_in_a_project_offers_none` |
| FR-13 | `a_send_tool_asks_under_ask_every_time` | passed (2026-11-28) |
| FR-14 | `a_hidden_file_is_never_uploaded`; `files_upload_only_after_approval` | passed (2026-11-28) |
| FR-15 | scope fixture test | passed (2026-11-28): `a_scope_refusal_names_the_step` |
| FR-16 | `a_toolkit_without_managed_auth_uses_the_persons_config` | passed (2026-11-28) |
| FR-17 | `a_new_key_starts_clean`; manual offline | not run |
| FR-18 | `a_restart_keeps_the_session` | passed (2026-11-28); also `a_reinstall_finds_the_one_earlier_user` |
| FR-19 | tool-label unit test | passed (2026-11-28): `connected-app-label.test.ts` |
| NFR-1 | review: no secret in `evo.db`, logs, `mcp.json` | not run |
| NFR-2 | review: results are tool output | not run |
| NFR-3 | timed manual check | not run |
| NFR-4 | `no_key_no_request` | passed (2026-11-28) |
