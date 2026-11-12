//! evo desktop shell.
//!
//! The shell owns no agent logic. It starts `evo-engine --mode rpc` as a child
//! process (see [`engine`]) and relays the engine's JSONL protocol to the web
//! UI: commands go in through the `engine_send` command, and every line the
//! engine prints comes back as an `engine://message` event. Running the engine
//! out of process means a crashing native extension cannot take the window down.
//! Which engines run, and their set-up, is the [`supervisor`]'s: windows ask
//! it with `engine_ensure` and attach to the engines they show
//! (docs/features/shell-hosted-engines/spec.md, FR-1, FR-2).

mod agent_tools;
mod apps;
mod browser;
mod chat_flags;
mod computer;
mod db;
mod engine;
mod files;
mod mcp;
mod memory_index;
mod mesh;
mod plugins;
mod shell_integration;
mod supervisor;
mod system;
mod updates;
mod worktrees;
mod terminal;
mod window;

use std::sync::Mutex;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        // First: a second launch (Start menu, dock, sign-in) shows this one instead.
        // The main window comes back, built again if it was destroyed
        // (docs/features/shell-hosted-engines/spec.md, FR-11).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            system::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![system::HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // App updates, driven from Rust only (updates.rs): the webview has no
        // updater permission.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(engine::EngineState::default())
        .manage(supervisor::EngineSupervisor::default())
        .manage(apps::AppsState::default())
        .manage(match db::Db::open() {
            Ok(db) => db,
            // Without its database the app cannot keep chats; say why and stop.
            Err(error) => panic!("could not open the evo database: {error}"),
        })
        .manage(window::MaterialState(Mutex::new(window::Material::None)))
        .manage(system::SystemState::default())
        .manage(terminal::TerminalState::default())
        .manage(agent_tools::AgentTools::default())
        .manage(memory_index::IndexWorker::default())
        .manage(browser::BrowserState::default())
        .setup(|app| {
            // The session mesh hub: mail between chats' engines.
            app.manage(mesh::MeshHub::new(app.handle().clone()));
            // Shell integration scripts for terminal tabs, refreshed on every start.
            match app
                .path()
                .app_data_dir()
                .and_then(|dir| shell_integration::install(&dir).map_err(tauri::Error::Io))
            {
                Ok(dir) => app.state::<terminal::TerminalState>().set_integration_dir(dir),
                Err(error) => eprintln!("could not install shell integration: {error}"),
            }
            system::dress_main(app.handle());
            app.state::<db::Db>().log(evo_store::logs::NewLog::new(
                "info",
                "shell",
                format!("evo desktop {} started", env!("CARGO_PKG_VERSION")),
            ));
            // The main window starts hidden (tauri.conf.json) and shows here,
            // unless evo was started at sign-in into the tray.
            system::setup(app.handle());
            // Connectors: one MCP pool for every chat.
            mcp::setup(app.handle());
            // Plugins' connectors join the pool.
            plugins::setup(app.handle());
            // Computer app control: put back windows a crashed run left off screen.
            computer::setup(app.handle());
            // App updates: the check timer, after the window is shown.
            updates::setup(app.handle());
            // Memory: index idle chats with the model the person chose.
            memory_index::setup(app.handle());
            // Chats with mail left undelivered when the app last closed get
            // their engines now, with or without a window (FR-4).
            supervisor::setup(app.handle());
            // A click on the shell's notice of a chat opens it (FR-9).
            supervisor::notices::setup(app.handle());
            // The Scheduler: missed runs now, then due tasks every 20 s, with
            // or without a window (shell-hosted-engines FR-7).
            agent_tools::scheduler::run::setup(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                system::on_close_requested(window, api);
            }
            if let tauri::WindowEvent::Destroyed = event {
                let label = window.label().to_string();
                // A window's terminals end with it.
                window.app_handle().state::<terminal::TerminalState>().kill_window(&label);
                window.app_handle().state::<browser::BrowserState>().close_window(window.app_handle(), &label);
                // A window is a viewer: its chat's engine goes on while it has
                // work (docs/features/shell-hosted-engines/spec.md, FR-2).
                supervisor::window_gone(window.app_handle(), &label);
            }
        })
        .invoke_handler(tauri::generate_handler![
            supervisor::engine_ensure,
            supervisor::engine_attach,
            supervisor::engine_detach,
            supervisor::engine_hold,
            supervisor::engine_questions,
            supervisor::engine_set_composing,
            supervisor::settings::settings_import_window,
            supervisor::settings::engine_reload_resources,
            agent_tools::scheduler::run::scheduler_save,
            agent_tools::scheduler::run::scheduler_delete,
            agent_tools::scheduler::run::scheduler_run_now,
            supervisor::notices::notifications_take,
            engine::engine_send,
            engine::engine_stop,
            engine::engine_info,
            engine::engine_running,
            updates::updates_state,
            updates::updates_check,
            updates::updates_install,
            mesh::mesh_pending,
            mesh::mesh_session_tools,
            agent_tools::agent_tools_definitions,
            agent_tools::agent_ask_answer,
            agent_tools::agent_window_chat,
            agent_tools::mcp::agent_mcp_chat_config,
            agent_tools::app::panel::agent_panels_report,
            agent_tools::user::agent_notify_muted,
            mesh::mesh_start_session,
            mesh::mesh_dismiss_proposal,
            db::db_info,
            db::db_reveal,
            db::workspaces_list,
            db::workspace_ensure,
            db::workspace_set_collapsed,
            db::workspace_remove,
            db::chats_list,
            db::chats_search,
            db::chat_create,
            db::chat_update,
            db::chat_record_turn,
            db::chat_fork,
            db::chat_move,
            db::chat_set_parent,
            db::chat_set_worktree,
            db::chat_delete,
            db::settings_all,
            db::settings_set,
            memory_index::memory_index_status,
            memory_index::memory_index_now,
            memory_index::memory_index_earlier_count,
            memory_index::memory_index_earlier,
            memory_index::memory_index_poke,
            db::logs_append,
            db::logs_query,
            db::logs_clear,
            db::logs_export,
            db::usage_summary,
            db::usage_events,
            db::usage_clear,
            window::window_material,
            window::window_set_theme,
            window::window_devtools,
            window::chat_window_open,
            window::chat_window_focus,
            window::chat_window_close,
            system::system_settings,
            system::engine_working,
            system::quit_confirmed,
            system::quit_dismissed,
            system::quit_pending,
            system::app_open_pending,
            mcp::mcp_state,
            mcp::mcp_upsert,
            mcp::mcp_remove,
            mcp::mcp_set_enabled,
            mcp::mcp_reconnect,
            mcp::mcp_set_permission,
            mcp::mcp_get_prompt,
            mcp::mcp_sign_in,
            mcp::mcp_sign_out,
            mcp::mcp_project_state,
            mcp::mcp_open_project,
            mcp::mcp_set_project_approval,
            mcp::mcp_read_resource,
            plugins::plugins_state,
            plugins::plugin_marketplace_add,
            plugins::plugin_marketplace_update,
            plugins::plugin_marketplace_remove,
            plugins::plugin_install,
            plugins::plugin_install_folder,
            plugins::plugin_uninstall,
            plugins::plugin_update,
            plugins::plugin_set_enabled,
            plugins::plugin_set_config,
            worktrees::worktree_create,
            worktrees::worktree_list,
            worktrees::worktree_changes,
            worktrees::worktree_apply,
            worktrees::worktree_discard,
            worktrees::git_status,
            worktrees::git_ignored,
            worktrees::git_file_base,
            system::system_set,
            apps::open_apps,
            apps::open_in_app,
            files::fs_list_dir,
            files::fs_stat,
            files::fs_read_file,
            files::fs_read_bytes,
            files::fs_write_file,
            files::fs_save_paste,
            files::fs_search,
            files::fs_grep,
            browser::browser_create,
            browser::browser_bounds,
            browser::browser_visible,
            browser::browser_stage,
            browser::browser_navigate,
            browser::browser_history,
            browser::browser_url,
            browser::browser_close,
            browser::browser_devtools,
            browser::browser_list,
            browser::browser_select_tab,
            browser::browser_set_chat,
            browser::browser_zoom,
            browser::browser_eval,
            browser::browser_cdp,
            browser::browser_status,
            browser::browser_cdp_subscribe,
            browser::browser_cdp_events,
            browser::browser_dialog_pending,
            browser::browser_dialog_answer,
            browser::browser_download_config,
            browser::browser_downloads,
            terminal::terminal_shells,
            terminal::terminal_shells_extra,
            terminal::terminal_spawn,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_kill,
            terminal::terminal_list,
            terminal::terminal_attach,
            terminal::terminal_close,
            terminal::terminal_set_chat,
            terminal::terminal_set_default_shell,
            computer::computer_stage,
            computer::computer_status,
            computer::computer_stop,
            computer::computer_resume,
            computer::computer_reveal,
            computer::computer_close,
            computer::computer_input,
            computer::computer_list_apps,
            computer::computer_permissions,
            computer::computer_request_permission,
            computer::computer_restart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the evo desktop app")
        .run(|_app, _event| {
            // With the tray on, evo stays when the last window goes; with it
            // off it asks first while chats work (FR-11).
            if let tauri::RunEvent::ExitRequested { code, api, .. } = &_event {
                system::on_exit_requested(_app, *code, api);
            }
            // Connectors, controlled apps and engines end with the app.
            if let tauri::RunEvent::Exit = _event {
                system::shutdown(_app);
            }
            // The dock icon reopens the window hidden behind the tray.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                system::show_main(_app);
            }
        });
}
