//! `browser_viewport`: emulate a device for a tab (docs/features/agent-browser
//! FR-11, T21) — size, device pixel ratio, touch, colour scheme, locale, time
//! zone and user agent, through the DevTools `Emulation` domain (Windows).
//!
//! The state each tab was given is kept here (module state, per tab id) so a
//! call can report it, change one part and leave the rest, and reset. The
//! planning (arguments → commands → new state) is a pure function.
//! Class: change (it alters how the person's tab looks), gated once per call
//! that actually changes something.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};

use super::driver::{cut_chars, finish, gate_page, resolve_page, settle, Stop, ToolResult};
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};

// ── Presets and state ───────────────────────────────────────────────────────

pub const MOBILE_UA: &str =
    "Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Mobile Safari/537.36";

/// A device preset: the parameters `Emulation.setDeviceMetricsOverride` and the touch switch get.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: f64,
    pub mobile: bool,
    pub touch: bool,
    pub user_agent: Option<&'static str>,
}

pub const MOBILE: Preset = Preset { width: 390, height: 844, device_scale_factor: 3.0, mobile: true, touch: true, user_agent: Some(MOBILE_UA) };
pub const TABLET: Preset = Preset { width: 820, height: 1180, device_scale_factor: 2.0, mobile: false, touch: true, user_agent: None };

/// What a tab is emulating now.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ViewportState {
    pub preset: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub device_scale_factor: Option<f64>,
    pub mobile: Option<bool>,
    pub touch: Option<bool>,
    pub color_scheme: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub user_agent: Option<String>,
}

/// The arguments of a call, checked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewportArgs {
    pub preset: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub device_scale_factor: Option<f64>,
    pub mobile: Option<bool>,
    pub color_scheme: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub user_agent: Option<String>,
    pub reset: bool,
}

pub type Command = (&'static str, Value);

/// What to run, and what the tab looks like afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub commands: Vec<Command>,
    pub state: ViewportState,
}

/// Per tab id: what `browser_viewport` set.
static STATES: Mutex<BTreeMap<String, ViewportState>> = Mutex::new(BTreeMap::new());

fn state_of(tab: &str) -> ViewportState {
    STATES.lock().expect("viewport states lock").get(tab).cloned().unwrap_or_default()
}

fn remember(tab: &str, state: ViewportState) {
    let mut states = STATES.lock().expect("viewport states lock");
    if state == ViewportState::default() {
        states.remove(tab);
    } else {
        states.insert(tab.to_string(), state);
    }
}

// ── Planning (pure) ─────────────────────────────────────────────────────────

fn optional_number(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(n) if n.is_finite() && n > 0.0 => Ok(Some(n)),
            _ => Err(format!("{key} must be a positive number.")),
        },
        Some(_) => Err(format!("{key} must be a positive number.")),
    }
}

fn optional_text(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(format!("{key} must be text.")),
    }
}

/// Read and check the arguments of a call.
pub fn parse_args(args: &Value) -> Result<ViewportArgs, String> {
    let reset = args.get("reset").and_then(Value::as_bool).unwrap_or(false);
    let preset = optional_text(args, "preset")?.map(|p| p.trim().to_lowercase());
    if let Some(preset) = preset.as_deref() {
        if !["mobile", "tablet", "desktop"].contains(&preset) {
            return Err(format!("Unknown preset \"{preset}\": use mobile, tablet or desktop."));
        }
    }
    let color_scheme = optional_text(args, "colorScheme")?.map(|c| c.trim().to_lowercase());
    if let Some(scheme) = color_scheme.as_deref() {
        if !["light", "dark", "none"].contains(&scheme) {
            return Err(format!("Unknown colorScheme \"{scheme}\": use light, dark or none."));
        }
    }
    let mobile = match args.get("mobile") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(flag)) => Some(*flag),
        Some(_) => return Err("mobile must be true or false.".into()),
    };
    Ok(ViewportArgs {
        preset,
        width: optional_number(args, "width")?,
        height: optional_number(args, "height")?,
        device_scale_factor: optional_number(args, "deviceScaleFactor")?,
        mobile,
        color_scheme,
        locale: optional_text(args, "locale")?,
        timezone: optional_text(args, "timezone")?,
        user_agent: optional_text(args, "userAgent")?,
        reset,
    })
}

/// The commands that clear every override.
pub fn reset_commands() -> Vec<Command> {
    vec![
        ("Emulation.clearDeviceMetricsOverride", json!({})),
        ("Emulation.setTouchEmulationEnabled", json!({ "enabled": false })),
        ("Emulation.setEmulatedMedia", json!({ "features": [{ "name": "prefers-color-scheme", "value": "" }] })),
        ("Emulation.setLocaleOverride", json!({})),
        ("Emulation.setTimezoneOverride", json!({ "timezoneId": "" })),
        ("Emulation.setUserAgentOverride", json!({ "userAgent": "" })),
    ]
}

fn rounded(value: f64) -> u32 {
    value.round().max(1.0) as u32
}

/// What to run for `args` on a tab in state `current`. `page` is the page's own
/// size, used when only one of width and height is given and the tab has no override yet.
pub fn plan(args: &ViewportArgs, current: &ViewportState, page: (u32, u32)) -> Plan {
    if args.reset {
        return Plan { commands: reset_commands(), state: ViewportState::default() };
    }
    let mut state = current.clone();
    let mut commands: Vec<Command> = Vec::new();
    let mut device = false;
    let mut phone_agent = false;

    match args.preset.as_deref() {
        Some("desktop") => {
            state.preset = None;
            state.width = None;
            state.height = None;
            state.device_scale_factor = None;
            state.mobile = None;
            state.touch = None;
            state.user_agent = None;
            commands.push(("Emulation.clearDeviceMetricsOverride", json!({})));
            commands.push(("Emulation.setTouchEmulationEnabled", json!({ "enabled": false })));
            commands.push(("Emulation.setUserAgentOverride", json!({ "userAgent": "" })));
        }
        Some(name @ ("mobile" | "tablet")) => {
            let preset = if name == "mobile" { MOBILE } else { TABLET };
            state.preset = Some(name.to_string());
            state.width = Some(preset.width);
            state.height = Some(preset.height);
            state.device_scale_factor = Some(preset.device_scale_factor);
            state.mobile = Some(preset.mobile);
            state.touch = Some(preset.touch);
            device = true;
            match preset.user_agent {
                Some(agent) => {
                    state.user_agent = Some(agent.to_string());
                    phone_agent = true;
                }
                None if current.user_agent.as_deref() == Some(MOBILE_UA) => {
                    // Leaving the phone preset: the phone's user agent goes with it.
                    state.user_agent = None;
                    commands.push(("Emulation.setUserAgentOverride", json!({ "userAgent": "" })));
                }
                None => {}
            }
        }
        _ => {}
    }

    if args.width.is_some() || args.height.is_some() || args.device_scale_factor.is_some() || args.mobile.is_some() {
        state.preset = None;
        state.width = Some(args.width.map(rounded).or(state.width).unwrap_or(page.0));
        state.height = Some(args.height.map(rounded).or(state.height).unwrap_or(page.1));
        state.device_scale_factor = Some(args.device_scale_factor.or(state.device_scale_factor).unwrap_or(1.0));
        state.mobile = Some(args.mobile.or(state.mobile).unwrap_or(false));
        if let Some(mobile) = args.mobile {
            state.touch = Some(mobile);
        }
        device = true;
    }
    if device {
        let (width, height) = (state.width.unwrap_or(page.0), state.height.unwrap_or(page.1));
        commands.push((
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": state.device_scale_factor.unwrap_or(1.0),
                "mobile": state.mobile.unwrap_or(false),
                "screenWidth": width,
                "screenHeight": height,
            }),
        ));
        let touch = state.touch.unwrap_or(false);
        let mut params = json!({ "enabled": touch });
        if touch {
            params["maxTouchPoints"] = json!(5);
        }
        commands.push(("Emulation.setTouchEmulationEnabled", params));
    }

    if let Some(scheme) = args.color_scheme.as_deref() {
        state.color_scheme = (scheme != "none").then(|| scheme.to_string());
        let value = if scheme == "none" { "" } else { scheme };
        commands.push(("Emulation.setEmulatedMedia", json!({ "features": [{ "name": "prefers-color-scheme", "value": value }] })));
    }
    if let Some(locale) = args.locale.as_deref() {
        let locale = locale.trim();
        state.locale = (!locale.is_empty()).then(|| locale.to_string());
        commands.push(("Emulation.setLocaleOverride", if locale.is_empty() { json!({}) } else { json!({ "locale": locale }) }));
    }
    if let Some(timezone) = args.timezone.as_deref() {
        let timezone = timezone.trim();
        state.timezone = (!timezone.is_empty()).then(|| timezone.to_string());
        commands.push(("Emulation.setTimezoneOverride", json!({ "timezoneId": timezone })));
    }
    if let Some(agent) = args.user_agent.as_deref() {
        let agent = agent.trim();
        state.user_agent = (!agent.is_empty()).then(|| agent.to_string());
        phone_agent = false;
        commands.push(("Emulation.setUserAgentOverride", json!({ "userAgent": agent })));
    }
    if phone_agent {
        if let Some(agent) = &state.user_agent {
            let mut params = json!({ "userAgent": agent });
            if state.mobile == Some(true) {
                params["platform"] = json!("Linux armv81");
            }
            commands.push(("Emulation.setUserAgentOverride", params));
        }
    }
    Plan { commands, state }
}

/// The state as a short sentence; "none" when nothing is overridden.
pub fn describe(state: &ViewportState) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let (Some(width), Some(height)) = (state.width, state.height) {
        parts.push(format!(
            "{}{width}×{height} at {}x{}{}",
            state.preset.as_ref().map(|p| format!("{p} preset, ")).unwrap_or_default(),
            format_number(state.device_scale_factor.unwrap_or(1.0)),
            if state.mobile == Some(true) { ", mobile" } else { "" },
            if state.touch == Some(true) { ", touch" } else { "" },
        ));
    }
    if let Some(scheme) = &state.color_scheme {
        parts.push(format!("{scheme} colour scheme"));
    }
    if let Some(locale) = &state.locale {
        parts.push(format!("locale {locale}"));
    }
    if let Some(timezone) = &state.timezone {
        parts.push(format!("time zone {timezone}"));
    }
    if let Some(agent) = &state.user_agent {
        let shown = if agent.chars().count() > 70 { format!("{}…", cut_chars(agent, 70)) } else { agent.clone() };
        parts.push(format!("user agent {shown}"));
    }
    if parts.is_empty() {
        "none (the tab's own size and settings)".to_string()
    } else {
        parts.join("; ")
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

// ── The tool ────────────────────────────────────────────────────────────────

const VIEWPORT: &str = "Emulate a device in a browser tab: preset \"mobile\" (390×844, 3x, touch, phone user agent), \"tablet\" (820×1180, 2x, touch) or \"desktop\" (clears the size), or width, height, deviceScaleFactor and mobile; colorScheme \"light\", \"dark\" or \"none\"; locale (en-US); timezone (Europe/Paris); userAgent. Settings stay until changed or reset: reset true clears all of them. Called with nothing it only reports what the tab emulates. Returns what the page now sees (size, media queries).";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "preset": { "type": "string", "enum": ["mobile", "tablet", "desktop"], "description": "A device preset; desktop clears the size, touch and user agent." },
            "width": { "type": "number", "description": "Viewport width in CSS pixels." },
            "height": { "type": "number", "description": "Viewport height in CSS pixels." },
            "deviceScaleFactor": { "type": "number", "description": "Device pixel ratio (1, 2, 3…)." },
            "mobile": { "type": "boolean", "description": "Mobile mode: touch events, overlay scrollbars, meta viewport." },
            "colorScheme": { "type": "string", "enum": ["light", "dark", "none"], "description": "Emulated prefers-color-scheme; none clears it." },
            "locale": { "type": "string", "description": "Locale such as en-US; empty clears it." },
            "timezone": { "type": "string", "description": "IANA time zone such as America/New_York; empty clears it." },
            "userAgent": { "type": "string", "description": "User agent string; empty clears it." },
            "reset": { "type": "boolean", "description": "Clear every override of this tab." },
            "tabId": { "type": "string", "description": "A tab id from browser_tabs; default: the active tab." },
        },
        "additionalProperties": false,
    })
}

fn run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(viewport(&ctx, &args).await) })
}

pub fn tools() -> Vec<Tool> {
    vec![Tool { name: "browser_viewport", description: VIEWPORT, schema, class: Class::Change, run }]
}

/// What the page reports about itself after the change.
const PAGE_VIEW: &str = r##"({
  w: innerWidth, h: innerHeight, dpr: devicePixelRatio,
  narrow: matchMedia("(max-width: 480px)").matches,
  dark: matchMedia("(prefers-color-scheme: dark)").matches,
  coarse: matchMedia("(pointer: coarse)").matches,
  lang: navigator.language,
  tz: (() => { try { return Intl.DateTimeFormat().resolvedOptions().timeZone; } catch { return ""; } })(),
})"##;

/// One line from what the page reported.
fn page_view_line(view: &Value) -> Option<String> {
    let width = view.get("w")?.as_f64()?;
    let height = view.get("h")?.as_f64()?;
    let dpr = view.get("dpr").and_then(Value::as_f64).unwrap_or(1.0);
    let flag = |key: &str| view.get(key).and_then(Value::as_bool).unwrap_or(false);
    let mut line = format!(
        "The page now sees {}×{} at {}x; (max-width: 480px) {}; {} scheme; {} pointer",
        width as i64,
        height as i64,
        format_number(dpr),
        if flag("narrow") { "matches" } else { "does not match" },
        if flag("dark") { "dark" } else { "light" },
        if flag("coarse") { "coarse" } else { "fine" },
    );
    if let Some(lang) = view.get("lang").and_then(Value::as_str).filter(|l| !l.is_empty()) {
        line.push_str(&format!("; language {lang}"));
    }
    if let Some(zone) = view.get("tz").and_then(Value::as_str).filter(|z| !z.is_empty()) {
        line.push_str(&format!("; time zone {zone}"));
    }
    line.push('.');
    Some(line)
}

async fn viewport(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let parsed = parse_args(args).map_err(|error| Stop::from(error))?;
    if !cfg!(windows) {
        return Err("browser_viewport is not available on this platform: it needs the DevTools protocol (Windows). Resize the Browser panel to change the page's size.".into());
    }
    let current = state_of(&page.tab);
    let size = page
        .eval("[Math.round(innerWidth), Math.round(innerHeight)]", false)
        .await
        .ok()
        .and_then(|v| Some((v.get(0)?.as_u64()? as u32, v.get(1)?.as_u64()? as u32)))
        .unwrap_or((1280, 800));
    let plan = plan(&parsed, &current, size);
    if plan.commands.is_empty() {
        let mut out = format!("Viewport: {}.", describe(&current));
        if let Some(line) = page.eval(PAGE_VIEW, false).await.ok().as_ref().and_then(page_view_line) {
            out.push_str(&format!("\n{line}"));
        }
        return Ok(ToolOutput::text(out));
    }
    let what = if parsed.reset { "Reset the viewport and device emulation".to_string() } else { format!("Emulate: {}", describe(&plan.state)) };
    gate_page(ctx, &page, &what).await?;
    for (method, params) in &plan.commands {
        if let Err(error) = page.cdp(method, params.clone()).await {
            // Some of it may have been applied: remember the plan so a reset starts from the truth.
            remember(&page.tab, plan.state.clone());
            return Err(format!("{method} failed: {error}. Part of the change may be applied; use reset true to start clean.").into());
        }
    }
    remember(&page.tab, plan.state.clone());
    settle(150).await;
    let mut out = if parsed.reset { "Viewport reset: no emulation.".to_string() } else { format!("Viewport: {}.", describe(&plan.state)) };
    if let Some(line) = page.eval(PAGE_VIEW, false).await.ok().as_ref().and_then(page_view_line) {
        out.push_str(&format!("\n{line}"));
    }
    out.push_str("\nThe settings stay on this tab until browser_viewport changes them or is called with reset true.");
    Ok(ToolOutput::text(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(value: Value) -> ViewportArgs {
        parse_args(&value).expect("valid arguments")
    }

    fn methods(plan: &Plan) -> Vec<&'static str> {
        plan.commands.iter().map(|(m, _)| *m).collect()
    }

    #[test]
    fn the_mobile_preset_is_a_phone() {
        let plan = plan(&args(json!({ "preset": "mobile" })), &ViewportState::default(), (1280, 800));
        let (method, params) = &plan.commands[0];
        assert_eq!(*method, "Emulation.setDeviceMetricsOverride");
        assert_eq!(params["width"], 390);
        assert_eq!(params["height"], 844);
        assert_eq!(params["deviceScaleFactor"], 3.0);
        assert_eq!(params["mobile"], true);
        assert_eq!(params["screenWidth"], 390);
        assert_eq!(plan.commands[1].1, json!({ "enabled": true, "maxTouchPoints": 5 }));
        let (method, params) = plan.commands.last().unwrap();
        assert_eq!(*method, "Emulation.setUserAgentOverride");
        assert_eq!(params["userAgent"], MOBILE_UA);
        assert_eq!(params["platform"], "Linux armv81");
        assert_eq!(plan.state.preset.as_deref(), Some("mobile"));
        assert_eq!(plan.state.touch, Some(true));
    }

    #[test]
    fn the_tablet_preset_has_touch_but_is_not_mobile_and_keeps_the_user_agent() {
        let plan = plan(&args(json!({ "preset": "tablet" })), &ViewportState::default(), (1280, 800));
        assert_eq!(methods(&plan), ["Emulation.setDeviceMetricsOverride", "Emulation.setTouchEmulationEnabled"]);
        let params = &plan.commands[0].1;
        assert_eq!((params["width"].as_u64(), params["height"].as_u64()), (Some(820), Some(1180)));
        assert_eq!(params["deviceScaleFactor"], 2.0);
        assert_eq!(params["mobile"], false);
        assert_eq!(plan.state.user_agent, None);
    }

    #[test]
    fn leaving_the_phone_preset_takes_its_user_agent_along() {
        let phone = plan(&args(json!({ "preset": "mobile" })), &ViewportState::default(), (1, 1)).state;
        let tablet = plan(&args(json!({ "preset": "tablet" })), &phone, (1, 1));
        assert_eq!(tablet.commands[0].0, "Emulation.setUserAgentOverride");
        assert_eq!(tablet.commands[0].1, json!({ "userAgent": "" }));
        assert_eq!(tablet.state.user_agent, None);
        // A user agent the agent chose itself stays.
        let own = ViewportState { user_agent: Some("custom".into()), ..Default::default() };
        let kept = plan(&args(json!({ "preset": "tablet" })), &own, (1, 1));
        assert_eq!(kept.state.user_agent.as_deref(), Some("custom"));
        assert!(!methods(&kept).contains(&"Emulation.setUserAgentOverride"));
    }

    #[test]
    fn the_desktop_preset_clears_size_touch_and_agent_but_not_the_colour_scheme() {
        let phone = plan(&args(json!({ "preset": "mobile", "colorScheme": "dark" })), &ViewportState::default(), (1, 1)).state;
        let desktop = plan(&args(json!({ "preset": "desktop" })), &phone, (1, 1));
        assert_eq!(
            methods(&desktop),
            ["Emulation.clearDeviceMetricsOverride", "Emulation.setTouchEmulationEnabled", "Emulation.setUserAgentOverride"]
        );
        assert_eq!(desktop.state, ViewportState { color_scheme: Some("dark".into()), ..Default::default() });
    }

    #[test]
    fn reset_clears_everything_and_the_state() {
        let plan = plan(&args(json!({ "reset": true, "preset": "mobile" })), &ViewportState { width: Some(1), ..Default::default() }, (1, 1));
        assert_eq!(plan.commands.len(), 6);
        assert_eq!(plan.commands[2].1["features"][0]["value"], "");
        assert_eq!(plan.state, ViewportState::default());
    }

    #[test]
    fn a_custom_size_fills_the_missing_side_from_the_tab_then_the_page() {
        let from_page = plan(&args(json!({ "width": 500 })), &ViewportState::default(), (1280, 800));
        let params = &from_page.commands[0].1;
        assert_eq!((params["width"].as_u64(), params["height"].as_u64()), (Some(500), Some(800)));
        assert_eq!(params["deviceScaleFactor"], 1.0);
        assert_eq!(from_page.commands[1].1, json!({ "enabled": false }));
        assert_eq!(from_page.state.preset, None);
        let current = ViewportState { width: Some(390), height: Some(844), device_scale_factor: Some(3.0), preset: Some("mobile".into()), ..Default::default() };
        let from_tab = plan(&args(json!({ "height": 700.4 })), &current, (1280, 800));
        let params = &from_tab.commands[0].1;
        assert_eq!((params["width"].as_u64(), params["height"].as_u64(), params["deviceScaleFactor"].as_f64()), (Some(390), Some(700), Some(3.0)));
        assert_eq!(from_tab.state.preset, None, "a custom size is no longer the preset");
    }

    #[test]
    fn mobile_true_turns_touch_on_with_the_size() {
        let plan = plan(&args(json!({ "mobile": true })), &ViewportState::default(), (1000, 600));
        assert_eq!(plan.commands[0].1["mobile"], true);
        assert_eq!(plan.commands[1].1, json!({ "enabled": true, "maxTouchPoints": 5 }));
    }

    #[test]
    fn colour_scheme_locale_time_zone_and_agent_have_their_commands() {
        let set = plan(
            &args(json!({ "colorScheme": "Dark", "locale": " fr-FR ", "timezone": "Europe/Paris", "userAgent": "Agent/1" })),
            &ViewportState::default(),
            (1, 1),
        );
        assert_eq!(
            methods(&set),
            ["Emulation.setEmulatedMedia", "Emulation.setLocaleOverride", "Emulation.setTimezoneOverride", "Emulation.setUserAgentOverride"]
        );
        assert_eq!(set.commands[0].1["features"][0], json!({ "name": "prefers-color-scheme", "value": "dark" }));
        assert_eq!(set.commands[1].1, json!({ "locale": "fr-FR" }));
        assert_eq!(set.commands[2].1, json!({ "timezoneId": "Europe/Paris" }));
        assert_eq!(set.state.locale.as_deref(), Some("fr-FR"));
        assert_eq!(set.state.user_agent.as_deref(), Some("Agent/1"));
        let cleared = plan(&args(json!({ "colorScheme": "none", "locale": "", "timezone": "", "userAgent": "" })), &set.state, (1, 1));
        assert_eq!(cleared.state, ViewportState::default());
        assert_eq!(cleared.commands[1].1, json!({}));
        assert_eq!(cleared.commands[0].1["features"][0]["value"], "");
    }

    #[test]
    fn a_user_agent_given_with_the_phone_preset_wins_over_the_preset_agent() {
        let plan = plan(&args(json!({ "preset": "mobile", "userAgent": "Mine/2" })), &ViewportState::default(), (1, 1));
        assert_eq!(plan.state.user_agent.as_deref(), Some("Mine/2"));
        let agents: Vec<_> = plan.commands.iter().filter(|(m, _)| *m == "Emulation.setUserAgentOverride").collect();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].1["userAgent"], "Mine/2");
    }

    #[test]
    fn nothing_to_change_plans_nothing() {
        let current = ViewportState { locale: Some("en-US".into()), ..Default::default() };
        let plan = plan(&args(json!({})), &current, (1, 1));
        assert!(plan.commands.is_empty());
        assert_eq!(plan.state, current);
    }

    #[test]
    fn bad_arguments_are_refused_with_the_allowed_values() {
        assert_eq!(parse_args(&json!({ "preset": "watch" })).unwrap_err(), "Unknown preset \"watch\": use mobile, tablet or desktop.");
        assert_eq!(parse_args(&json!({ "colorScheme": "blue" })).unwrap_err(), "Unknown colorScheme \"blue\": use light, dark or none.");
        assert_eq!(parse_args(&json!({ "width": -4 })).unwrap_err(), "width must be a positive number.");
        assert_eq!(parse_args(&json!({ "deviceScaleFactor": "big" })).unwrap_err(), "deviceScaleFactor must be a positive number.");
        assert_eq!(parse_args(&json!({ "mobile": "yes" })).unwrap_err(), "mobile must be true or false.");
        assert_eq!(parse_args(&json!({ "locale": 5 })).unwrap_err(), "locale must be text.");
        assert_eq!(parse_args(&json!({ "preset": " MOBILE " })).unwrap().preset.as_deref(), Some("mobile"));
        assert_eq!(parse_args(&json!({ "width": null })).unwrap().width, None);
    }

    #[test]
    fn the_state_reads_as_a_sentence() {
        assert_eq!(describe(&ViewportState::default()), "none (the tab's own size and settings)");
        let phone = plan(&args(json!({ "preset": "mobile", "colorScheme": "dark", "locale": "en-GB", "timezone": "Asia/Tokyo" })), &ViewportState::default(), (1, 1)).state;
        let text = describe(&phone);
        assert!(text.starts_with("mobile preset, 390×844 at 3x, mobile, touch; dark colour scheme; locale en-GB; time zone Asia/Tokyo; user agent Mozilla/5.0"), "{text}");
        assert!(text.ends_with('…'));
        let custom = ViewportState { width: Some(500), height: Some(300), device_scale_factor: Some(1.5), ..Default::default() };
        assert_eq!(describe(&custom), "500×300 at 1.5x");
    }

    #[test]
    fn what_the_page_sees_is_one_line() {
        let view = json!({ "w": 390, "h": 844, "dpr": 3, "narrow": true, "dark": false, "coarse": true, "lang": "en-US", "tz": "UTC" });
        assert_eq!(
            page_view_line(&view).unwrap(),
            "The page now sees 390×844 at 3x; (max-width: 480px) matches; light scheme; coarse pointer; language en-US; time zone UTC."
        );
        assert!(page_view_line(&json!({ "dpr": 1 })).is_none());
    }

    #[test]
    fn the_schema_is_closed_and_takes_a_tab() {
        let schema = schema();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["tabId"]["type"], "string");
        assert_eq!(schema["properties"]["preset"]["enum"], json!(["mobile", "tablet", "desktop"]));
        assert!(schema.get("required").is_none());
        assert_eq!(tools()[0].class, Class::Change);
    }

    #[test]
    fn the_state_of_a_tab_is_remembered_and_forgotten() {
        remember("viewport-test-tab", ViewportState { locale: Some("de".into()), ..Default::default() });
        assert_eq!(state_of("viewport-test-tab").locale.as_deref(), Some("de"));
        remember("viewport-test-tab", ViewportState::default());
        assert_eq!(state_of("viewport-test-tab"), ViewportState::default());
    }
}
