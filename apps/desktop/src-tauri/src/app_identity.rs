//! Two front ends in one app process: Evo Harness (the `main` window) and
//! Evo Mates (the `mates` window) (docs/features/evo-mates/spec.md, FR-1 –
//! FR-5, NFR-4; plan 3.3 "Launch target"; ADR 0028).
//!
//! * Every launch names its front end with `--app harness|mates` (default
//!   `harness`). A second launch hands its arguments to the running app
//!   (single-instance), which opens that front end's window; a cold start
//!   with `--app mates` shows only the Mates window (FR-2).
//! * [`open`] shows a front end's window, building the Mates window when it
//!   is not there. The `app_open` command does the same for the switch
//!   buttons (FR-3).
//! * On Windows the Mates window has an application user model ID of its
//!   own, so the taskbar shows Evo Mates as its own app, and a pinned Mates
//!   button relaunches with `--app mates`. Installed builds keep an
//!   "Evo Mates" Start menu entry with the same ID (FR-1, NFR-4).

use std::sync::atomic::{AtomicBool, Ordering};

use evo_store::logs::NewLog;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::db::Db;

/// The argument that names the front end a launch opens.
pub const APP_ARG: &str = "--app";
/// The Mates window's label and page.
pub const MATES: &str = "mates";
const MATES_PAGE: &str = "mates.html";

/// A front end of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    Harness,
    Mates,
}

impl Front {
    /// `harness` or `mates`, as `--app` and `app_open` name them.
    pub fn parse(name: &str) -> Option<Front> {
        match name.trim().to_ascii_lowercase().as_str() {
            "harness" => Some(Front::Harness),
            "mates" => Some(Front::Mates),
            _ => None,
        }
    }
}

/// The front end a launch's arguments name: `--app mates` or `--app=mates`;
/// `None` when they name none (or one this build does not know).
pub fn launch_target<S: AsRef<str>>(args: &[S]) -> Option<Front> {
    let mut args = args.iter().map(AsRef::as_ref);
    while let Some(arg) = args.next() {
        if arg == APP_ARG {
            return args.next().and_then(Front::parse);
        }
        if let Some(value) = arg.strip_prefix("--app=") {
            return Front::parse(value);
        }
    }
    None
}

/// The name the Mates window and its taskbar entry show. A debug build says
/// "Dev", as [`crate::window::app_name`] does.
pub fn mates_name() -> &'static str {
    if cfg!(debug_assertions) {
        "Evo Mates Dev"
    } else {
        "Evo Mates"
    }
}

/// The Mates window's application user model ID on Windows: its own, so the
/// taskbar groups it apart from Evo Harness.
pub fn mates_app_id(identifier: &str) -> String {
    if cfg!(debug_assertions) {
        format!("{identifier}.mates.dev")
    } else {
        format!("{identifier}.mates")
    }
}

/// A Mates window is being built (off the event loop's thread).
static BUILDING: AtomicBool = AtomicBool::new(false);

/// A chat Evo Mates should show once its window has loaded.
static PENDING_CHAT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Show `chat` (a mate's conversation) in Evo Mates: from Harness's Needs
/// you or a notice (docs/features/evo-mates/spec.md, FR-12). A window
/// still being built reads it when it loads (`mates_open_pending`).
pub fn open_mates_chat(app: &AppHandle, chat: &str) {
    *PENDING_CHAT.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(chat.to_string());
    show_mates(app);
    if app.get_window(MATES).is_some() {
        let _ = tauri::Emitter::emit_to(app, MATES, "mates://open-chat", serde_json::json!({ "chat": chat }));
    }
}

/// The chat Evo Mates was asked to show before it loaded, once.
#[tauri::command]
pub fn mates_open_pending() -> Option<String> {
    PENDING_CHAT.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
}

/// Show `front`'s window: the main window (built again if it was destroyed),
/// or the Mates window, built on first need.
pub fn open(app: &AppHandle, front: Front) {
    match front {
        Front::Harness => {
            crate::system::show_main(app);
        }
        Front::Mates => show_mates(app),
    }
}

/// [`open`] at the app's start, on the setup thread. The Mates window is
/// built here and not off-thread: WebView2 refuses ("the requested resource
/// is in use") a webview created while the main window's is still being
/// created at start, as the quick entry box's is built here too.
pub fn open_at_start(app: &AppHandle, front: Front) {
    match front {
        Front::Harness => open(app, front),
        Front::Mates => {
            if let Err(error) = build_mates(app) {
                app.state::<Db>().log(NewLog::new("error", "shell", format!("could not open Evo Mates: {error}")));
                crate::system::show_main(app);
            }
        }
    }
}

/// A second launch (Start menu, taskbar, dock, desktop entry): open the front
/// end it names, else Evo Harness, as before.
pub fn on_second_launch(app: &AppHandle, args: &[String]) {
    open(app, launch_target(args).unwrap_or(Front::Harness));
}

fn front(window: &tauri::Window) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Bring the Mates window forward, building it when it is not there.
pub fn show_mates(app: &AppHandle) {
    if crate::system::quitting(app) {
        return;
    }
    if let Some(window) = app.get_window(MATES) {
        front(&window);
        return;
    }
    if BUILDING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    // Off the caller: on Windows a window built on the event loop's thread
    // (a tray click, a synchronous command) deadlocks.
    tauri::async_runtime::spawn(async move {
        let built = build_mates(&app);
        BUILDING.store(false, Ordering::Release);
        if let Err(error) = built {
            app.state::<Db>().log(NewLog::new("error", "shell", format!("could not open Evo Mates: {error}")));
        }
    });
}

fn build_mates(app: &AppHandle) -> Result<(), String> {
    let builder = WebviewWindowBuilder::new(app, MATES, WebviewUrl::App(MATES_PAGE.into()))
        .title(mates_name())
        .inner_size(1180.0, 780.0)
        .min_inner_size(760.0, 520.0)
        .shadow(true)
        .visible(false);
    // The main window's frame on each platform (tauri.*.conf.json): native
    // traffic lights over the page on macOS, frameless elsewhere, opaque on
    // Linux.
    #[cfg(target_os = "macos")]
    let builder = builder
        .decorations(true)
        .transparent(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(16.0, 20.0));
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false).transparent(!cfg!(target_os = "linux"));
    let window = builder.build().map_err(|error| error.to_string())?;
    let _ = window.set_icon(mates_icon());
    if let Some(window) = app.get_window(MATES) {
        // Same material as the main window; `window_material` reports it.
        crate::window::apply(&window, None);
        crate::window::own_webview_size(&window);
        #[cfg(windows)]
        if let Err(error) = win::set_window_identity(&window, &mates_app_id(&app.config().identifier), mates_name()) {
            app.state::<Db>().log(NewLog::new("warn", "shell", format!("Evo Mates has no taskbar identity of its own: {error}")));
        }
        front(&window);
    }
    Ok(())
}

/// The Mates window's icon, built into the app (decoded at compile time).
fn mates_icon() -> tauri::image::Image<'static> {
    tauri::include_image!("icons/mates/128x128.png")
}

/// Switch to the other front end (the **Switch to** buttons, FR-3).
#[tauri::command]
pub async fn app_open(app: AppHandle, target: String, chat: Option<String>) -> Result<(), String> {
    let front = Front::parse(&target).ok_or_else(|| format!("unknown app \"{target}\""))?;
    match (front, chat) {
        (Front::Mates, Some(chat)) => open_mates_chat(&app, &chat),
        (front, _) => open(&app, front),
    }
    Ok(())
}

/// At start: keep the Evo Mates Start menu entry of an installed build
/// (Windows), and say which front end this launch opens.
pub fn setup(app: &AppHandle) -> Front {
    #[cfg(all(windows, not(debug_assertions)))]
    if let Err(error) = win::ensure_start_menu_entry(&app.config().identifier) {
        app.state::<Db>().log(NewLog::new("warn", "shell", format!("could not add Evo Mates to the Start menu: {error}")));
    }
    #[cfg(not(all(windows, not(debug_assertions))))]
    let _ = app;
    let args: Vec<String> = std::env::args().collect();
    launch_target(&args).unwrap_or(Front::Harness)
}

#[cfg(windows)]
mod win {
    use windows::core::{Interface, HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Storage::EnhancedStorage::{
        PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand, PKEY_AppUserModel_RelaunchDisplayNameResource,
        PKEY_AppUserModel_RelaunchIconResource,
    };
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    use windows::Win32::UI::Shell::{FOLDERID_Programs, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT};

    /// Set a string property: the store copies the value, so the buffer
    /// only has to outlive the call. The variant is never dropped: the
    /// crate's `Drop` would `PropVariantClear` it, freeing our Rust buffer
    /// with `CoTaskMemFree` (heap corruption, 0xC0000374).
    unsafe fn set_string(store: &IPropertyStore, key: &PROPERTYKEY, value: &str) -> windows::core::Result<()> {
        let mut wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let mut variant = std::mem::ManuallyDrop::new(PROPVARIANT::default());
        unsafe {
            let inner = &mut *variant.Anonymous.Anonymous;
            inner.vt = VT_LPWSTR;
            inner.Anonymous.pwszVal = PWSTR(wide.as_mut_ptr());
            store.SetValue(key, &*variant)
        }
    }

    fn exe() -> Result<String, String> {
        std::env::current_exe().map(|path| path.display().to_string()).map_err(|error| error.to_string())
    }

    /// The Mates icon next to the app (bundle resources), for the taskbar's
    /// pinned button and the Start menu entry; the app's own icon without it.
    fn icon_path(exe: &str) -> String {
        std::path::Path::new(exe)
            .parent()
            .map(|dir| dir.join("icons").join("mates.ico"))
            .filter(|path| path.exists())
            .map(|path| format!("{},0", path.display()))
            .unwrap_or_else(|| format!("{exe},0"))
    }

    /// The window's taskbar identity: its own ID, and how a pinned button
    /// starts it again (all four properties are needed for the relaunch).
    pub fn set_window_identity(window: &tauri::Window, id: &str, name: &str) -> Result<(), String> {
        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        let exe = exe()?;
        unsafe {
            let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).map_err(|error| error.to_string())?;
            set_string(&store, &PKEY_AppUserModel_ID, id).map_err(|error| error.to_string())?;
            set_string(&store, &PKEY_AppUserModel_RelaunchCommand, &format!("\"{exe}\" {} mates", super::APP_ARG))
                .map_err(|error| error.to_string())?;
            set_string(&store, &PKEY_AppUserModel_RelaunchDisplayNameResource, name).map_err(|error| error.to_string())?;
            set_string(&store, &PKEY_AppUserModel_RelaunchIconResource, &icon_path(&exe)).map_err(|error| error.to_string())?;
            store.Commit().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// The installed app's "Evo Mates" Start menu shortcut, with the Mates
    /// window's ID so the two group on the taskbar. Made when missing; the
    /// installer's uninstall hook removes it (`windows/hooks.nsh`).
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub fn ensure_start_menu_entry(identifier: &str) -> Result<(), String> {
        let exe = exe()?;
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let folder = SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None).map_err(|error| error.to_string())?;
            let programs = folder.to_string().map_err(|error| error.to_string())?;
            windows::Win32::System::Com::CoTaskMemFree(Some(folder.0 as _));
            let path = std::path::Path::new(&programs).join(format!("{}.lnk", super::mates_name()));
            if path.exists() {
                return Ok(());
            }
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|error| error.to_string())?;
            link.SetPath(&HSTRING::from(exe.as_str())).map_err(|error| error.to_string())?;
            link.SetArguments(&HSTRING::from(format!("{} mates", super::APP_ARG))).map_err(|error| error.to_string())?;
            let icon = icon_path(&exe);
            let icon_file = icon.rsplit_once(',').map_or(icon.as_str(), |(file, _)| file);
            link.SetIconLocation(&HSTRING::from(icon_file), 0).map_err(|error| error.to_string())?;
            link.SetDescription(&HSTRING::from("AI teammates on Evo")).map_err(|error| error.to_string())?;
            let store: IPropertyStore = link.cast().map_err(|error| error.to_string())?;
            set_string(&store, &PKEY_AppUserModel_ID, &super::mates_app_id(identifier)).map_err(|error| error.to_string())?;
            store.Commit().map_err(|error| error.to_string())?;
            let file: IPersistFile = link.cast().map_err(|error| error.to_string())?;
            file.Save(PCWSTR(HSTRING::from(path.display().to_string()).as_ptr()), true)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_names_its_front_end() {
        assert_eq!(launch_target(&["evo-harness", "--app", "mates"]), Some(Front::Mates));
        assert_eq!(launch_target(&["evo-harness", "--app=mates"]), Some(Front::Mates));
        assert_eq!(launch_target(&["evo-harness", "--app", "Harness"]), Some(Front::Harness));
        assert_eq!(launch_target(&["evo-harness", "--hidden", "--app", "mates"]), Some(Front::Mates));
    }

    #[test]
    fn a_launch_without_a_known_front_end_names_none() {
        assert_eq!(launch_target::<&str>(&[]), None);
        assert_eq!(launch_target(&["evo-harness"]), None);
        assert_eq!(launch_target(&["evo-harness", "--hidden"]), None);
        assert_eq!(launch_target(&["evo-harness", "--app"]), None);
        assert_eq!(launch_target(&["evo-harness", "--app", "office"]), None);
    }

    #[test]
    fn the_switch_names_parse() {
        assert_eq!(Front::parse("mates"), Some(Front::Mates));
        assert_eq!(Front::parse(" harness "), Some(Front::Harness));
        assert_eq!(Front::parse(""), None);
    }

    #[test]
    fn mates_has_its_own_taskbar_id() {
        let id = mates_app_id("com.evoelsewhere.evo");
        assert!(id.starts_with("com.evoelsewhere.evo.mates"));
        assert_ne!(id, "com.evoelsewhere.evo");
    }
}
