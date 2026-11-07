//! Pointer tools (docs/features/agent-browser FR-1, T13): `browser_hover`,
//! `browser_drag`, and `browser_click` with `button` and `clickCount`.
//!
//! With DevTools (Windows) they send real mouse events; elsewhere hover and
//! drag fall back to the page helper's script events (`__evo.hover`,
//! `__evo.dragTo`), which do not trigger CSS `:hover`. A ref is aimed at its
//! centre; when something covers it or it is off screen the script path is used
//! (hover) or the result says so (drag), as `browser_click` does.
//! An element the page made `draggable` uses HTML5 drag and drop, which real
//! mouse events would start as an operating-system drag: that case goes through
//! the script path.

use serde::Deserialize;
use serde_json::{json, Value};

use super::core::{self, ClickInput};
use super::driver::{finish, gate_page, is_dialog_error, js, resolve_page, settle, show_browser, Page, Stop, ToolResult};
use super::wait::with_element;
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};

// ── Schemas ─────────────────────────────────────────────────────────────────

const HOVER: &str = "Move the pointer onto an element (ref from browser_read_page / browser_find) or a point (x, y in CSS pixels of the screenshot), so hover menus and tooltips open. Returns the element's state and the page. Then use browser_read_page or browser_screenshot to see what appeared.";
const DRAG: &str = "Drag from one element or point to another with the left button held: sliders, sortable lists, drag and drop. from and to are each a ref or a point {x, y}. Returns where it ended and the target's state.";
const CLICK: &str = "Click an element by ref (from browser_read_page / browser_find), or a point (x, y in CSS pixels of the screenshot). button picks left (default), right or middle; clickCount 2 or 3 double- or triple-clicks. Returns the element's state afterwards (checked, focused, value) and the page. A click that opens a page dialog returns while it is open: answer it with browser_dialog.";

fn schema(properties: Value, required: &[&str]) -> Value {
    let mut props = properties.as_object().cloned().unwrap_or_default();
    props.insert("tabId".into(), json!({ "type": "string", "description": "A tab id from browser_tabs; default: the active tab." }));
    let mut object = json!({ "type": "object", "properties": props, "additionalProperties": false });
    if !required.is_empty() {
        object["required"] = json!(required);
    }
    object
}

fn endpoint_schema(what: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{what}: {{ref}} from browser_read_page / browser_find, or a point {{x, y}} in CSS pixels of the screenshot."),
        "properties": { "ref": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" } },
        "additionalProperties": false,
    })
}

fn hover_schema() -> Value {
    schema(json!({ "ref": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" } }), &[])
}

fn drag_schema() -> Value {
    schema(json!({ "from": endpoint_schema("Where to press"), "to": endpoint_schema("Where to release") }), &["from", "to"])
}

fn click_schema() -> Value {
    schema(
        json!({
            "ref": { "type": "string" },
            "x": { "type": "number" },
            "y": { "type": "number" },
            "button": { "type": "string", "enum": ["left", "right", "middle"], "description": "Default left. Right and middle need the DevTools protocol (Windows)." },
            "clickCount": { "type": "number", "description": "1 (default), 2 or 3." },
            "doubleClick": { "type": "boolean", "description": "Same as clickCount 2." },
        }),
        &[],
    )
}

macro_rules! runner {
    ($name:ident, $body:path) => {
        fn $name(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
            Box::pin(async move { finish($body(&ctx, &args).await) })
        }
    };
}

runner!(hover_run, hover);
runner!(drag_run, drag);
runner!(click_run, click);

/// `browser_hover`, `browser_drag` and the `browser_click` that replaces core's.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "browser_hover", description: HOVER, schema: hover_schema, class: Class::Act, run: hover_run },
        Tool { name: "browser_drag", description: DRAG, schema: drag_schema, class: Class::Act, run: drag_run },
        Tool { name: "browser_click", description: CLICK, schema: click_schema, class: Class::Act, run: click_run },
    ]
}

// ── Arguments ───────────────────────────────────────────────────────────────

/// Where a pointer action starts or ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Endpoint {
    Ref(String),
    Point { x: f64, y: f64 },
}

impl Endpoint {
    /// `ref_4` or `(120, 80)`.
    pub fn label(&self) -> String {
        match self {
            Endpoint::Ref(reference) => reference.clone(),
            Endpoint::Point { x, y } => format!("({}, {})", x.round() as i64, y.round() as i64),
        }
    }
}

fn finite(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|n| n.is_finite())
}

/// A ref (a string, or `{ref}`) or a point `{x, y}`.
fn endpoint_of(map: &serde_json::Map<String, Value>) -> Option<Endpoint> {
    if let Some(reference) = map.get("ref").and_then(Value::as_str).map(str::trim).filter(|r| !r.is_empty()) {
        return Some(Endpoint::Ref(reference.to_string()));
    }
    Some(Endpoint::Point { x: finite(map.get("x"))?, y: finite(map.get("y"))? })
}

/// `from` / `to` of a drag: a ref string, or an object with `ref`, or `x` and `y`.
pub fn parse_endpoint(value: Option<&Value>, name: &str) -> Result<Endpoint, String> {
    match value {
        Some(Value::String(reference)) if !reference.trim().is_empty() => Ok(Endpoint::Ref(reference.trim().to_string())),
        Some(Value::Object(map)) => endpoint_of(map).ok_or_else(|| format!("{name} needs a ref, or a point with numbers x and y.")),
        _ => Err(format!("{name} needs a ref or a point {{x, y}}.")),
    }
}

/// The `ref`, or `x` and `y`, of `browser_hover`.
pub fn hover_target(args: &Value) -> Result<Endpoint, String> {
    args.as_object().and_then(endpoint_of).ok_or_else(|| "browser_hover needs a ref or x and y.".to_string())
}

/// Validate `button` and `clickCount` (and `doubleClick`) of a click.
pub fn parse_click(args: &Value) -> Result<ClickInput, String> {
    let button = match args.get("button") {
        None | Some(Value::Null) => "left",
        Some(Value::String(button)) => match button.as_str() {
            "left" => "left",
            "right" => "right",
            "middle" => "middle",
            other => return Err(format!("button must be left, right or middle, not \"{other}\".")),
        },
        Some(_) => return Err("button must be left, right or middle.".into()),
    };
    let count = match args.get("clickCount") {
        None | Some(Value::Null) => {
            if args.get("doubleClick").and_then(Value::as_bool).unwrap_or(false) {
                2
            } else {
                1
            }
        }
        Some(value) => match value.as_f64() {
            Some(n) if n.fract() == 0.0 && (1.0..=3.0).contains(&n) => n as u32,
            _ => return Err("clickCount must be 1, 2 or 3.".into()),
        },
    };
    Ok(ClickInput { button, count, verb: click_verb(button, count) })
}

/// How a click result starts: `Clicked`, `Double-clicked`, `Right-clicked`…
pub fn click_verb(button: &str, count: u32) -> &'static str {
    match (button, count) {
        ("right", 2) => "Right double-clicked",
        ("right", 3) => "Right triple-clicked",
        ("right", _) => "Right-clicked",
        ("middle", 2) => "Middle double-clicked",
        ("middle", 3) => "Middle triple-clicked",
        ("middle", _) => "Middle-clicked",
        (_, 2) => "Double-clicked",
        (_, 3) => "Triple-clicked",
        _ => "Clicked",
    }
}

/// The line the gate shows: `Right-click ref_3`, `Double-click at (10, 20)`.
pub fn click_summary(args: &Value, input: &ClickInput) -> String {
    let verb = match (input.button, input.count) {
        ("left", 1) => "Click",
        ("left", 2) => "Double-click",
        ("left", _) => "Triple-click",
        ("right", _) => "Right-click",
        _ => "Middle-click",
    };
    let reference = args.get("ref").and_then(Value::as_str).unwrap_or("");
    if !reference.is_empty() {
        format!("{verb} {reference}")
    } else {
        let at = |key: &str| finite(args.get(key)).map_or(0, |n| n.round() as i64);
        format!("{verb} at ({}, {})", at("x"), at("y"))
    }
}

// ── Mouse events and geometry ───────────────────────────────────────────────

/// An `Input.dispatchMouseEvent` call. `button` is `none` for a plain move.
pub fn mouse_event(kind: &str, x: f64, y: f64, button: &str, buttons: u32, click_count: u32) -> Value {
    json!({ "type": kind, "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": click_count })
}

pub const DRAG_STEPS: usize = 8;

/// The points between `from` and `to` a drag moves through, `to` last.
pub fn drag_path(from: (f64, f64), to: (f64, f64), steps: usize) -> Vec<(f64, f64)> {
    let steps = steps.max(1);
    (1..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
        })
        .collect()
}

/// `point` after the page scrolled from `before` to `after` (scroll x, y): where it is in the view now.
pub fn shift_by(point: (f64, f64), before: (f64, f64), after: (f64, f64)) -> (f64, f64) {
    (point.0 - (after.0 - before.0), point.1 - (after.1 - before.1))
}

/// Whether the point is inside a view of `width` x `height`.
pub fn on_screen(point: (f64, f64), width: f64, height: f64) -> bool {
    point.0 >= 0.0 && point.1 >= 0.0 && point.0 <= width && point.1 <= height
}

#[derive(Debug, Deserialize)]
struct Located {
    x: f64,
    y: f64,
    hits: bool,
    covered: Option<String>,
}

/// A note for a ref whose centre is not what the pointer reaches.
fn cover_note(reference: &str, at: &Located) -> Option<String> {
    if at.hits {
        return None;
    }
    Some(match at.covered.as_deref().filter(|c| !c.is_empty()) {
        Some(covered) => format!("{reference} is covered by {covered}; the pointer went to its centre anyway"),
        None => format!("{reference} is not reachable at its centre; the pointer went there anyway"),
    })
}

/// A dialog opening under a pointer event is a result, not a failure.
fn tolerate_dialog(result: Result<Value, String>) -> Result<(), String> {
    match result {
        Ok(_) => Ok(()),
        Err(error) if is_dialog_error(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn mouse(page: &Page, kind: &str, x: f64, y: f64, button: &str, buttons: u32, count: u32) -> Result<(), String> {
    page.cdp("Input.dispatchMouseEvent", mouse_event(kind, x, y, button, buttons, count)).await.map(|_| ())
}

fn not_on_this_platform(what: &str, instead: &str) -> Stop {
    Stop::from(format!("{what} is not available on this platform (it needs the DevTools protocol, Windows): {instead}."))
}

/// A ref's centre in the top page's view, scrolling it into view.
async fn locate(page: &Page, endpoint: &Endpoint) -> Result<(f64, f64, Option<String>), String> {
    match endpoint {
        Endpoint::Point { x, y } => Ok((*x, *y, None)),
        Endpoint::Ref(reference) => {
            let at = page.helper::<Located>(&format!("point({})", js(reference))).await?;
            let note = cover_note(reference, &at);
            Ok((at.x, at.y, note))
        }
    }
}

/// What the endpoint looks like now (refs only).
async fn state_of(page: &Page, endpoint: &Endpoint) -> Option<String> {
    match endpoint {
        Endpoint::Ref(reference) => page.state_of(reference).await,
        Endpoint::Point { .. } => None,
    }
}

async fn tail(page: &Page, lines: &mut Vec<String>, state: Option<String>) {
    if let Some(state) = state {
        lines.push(format!("Now: {state}"));
    }
    lines.push(format!("Page: {}", page.summary().await));
    lines.extend(page.dialog_note());
}

fn with_notes(head: &str, notes: &[String]) -> String {
    if notes.is_empty() {
        format!("{head}.")
    } else {
        format!("{head} ({}).", notes.join("; "))
    }
}

// ── browser_hover ───────────────────────────────────────────────────────────

async fn hover(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let target = hover_target(args)?;
    let page = resolve_page(ctx, args)?;
    gate_page(ctx, &page, &format!("Hover over {}", target.label())).await?;
    show_browser(ctx, Some(&page.tab)).await;
    let mut notes: Vec<String> = Vec::new();
    match (&target, cfg!(windows)) {
        (Endpoint::Ref(reference), true) => {
            let at = page.helper::<Located>(&format!("point({})", js(reference))).await?;
            if at.hits {
                settle(60).await;
                tolerate_dialog(page.cdp("Input.dispatchMouseEvent", mouse_event("mouseMoved", at.x, at.y, "none", 0, 0)).await)?;
            } else {
                notes.push(match at.covered.as_deref().filter(|c| !c.is_empty()) {
                    Some(covered) => format!("it was covered by {covered}, so hover events were sent by script"),
                    None => "it was off screen, so hover events were sent by script".to_string(),
                });
                page.helper::<Value>(&format!("hover({})", js(reference))).await?;
            }
        }
        (Endpoint::Ref(reference), false) => {
            page.helper::<Value>(&format!("hover({})", js(reference))).await?;
            notes.push("hover events were sent by script; CSS :hover does not apply".into());
        }
        (Endpoint::Point { x, y }, true) => {
            tolerate_dialog(page.cdp("Input.dispatchMouseEvent", mouse_event("mouseMoved", *x, *y, "none", 0, 0)).await)?;
        }
        (Endpoint::Point { .. }, false) => return Err(not_on_this_platform("Hovering a point", "hover by ref instead")),
    }
    // Menus open on a delay or a transition.
    settle(300).await;
    let mut lines = vec![with_notes(&format!("Hovered {}", target.label()), &notes)];
    let state = state_of(&page, &target).await;
    tail(&page, &mut lines, state).await;
    Ok(ToolOutput::text(lines.join("\n")))
}

// ── browser_drag ────────────────────────────────────────────────────────────

/// Page script: whether the press point of a drag is on an element the page made `draggable`.
fn draggable_script(from: &Endpoint) -> String {
    match from {
        Endpoint::Ref(reference) => with_element(reference, "return !!el && !!el.closest('[draggable=\"true\"]');"),
        Endpoint::Point { x, y } => format!("!!document.elementFromPoint({x}, {y})?.closest('[draggable=\"true\"]')"),
    }
}

/// The `dragTo(from, to)` call of the page helper (`from` must be a ref).
fn drag_to_call(from: &str, to: &Endpoint) -> String {
    let to = match to {
        Endpoint::Ref(reference) => js(reference),
        Endpoint::Point { x, y } => js(&json!({ "x": x, "y": y })),
    };
    format!("dragTo({}, {to})", js(from))
}

async fn drag(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let from = parse_endpoint(args.get("from"), "from")?;
    let to = parse_endpoint(args.get("to"), "to")?;
    if from == to {
        return Err("The drag starts and ends at the same place.".into());
    }
    let page = resolve_page(ctx, args)?;
    gate_page(ctx, &page, &format!("Drag {} to {}", from.label(), to.label())).await?;
    show_browser(ctx, Some(&page.tab)).await;

    let mut notes: Vec<String> = Vec::new();
    let mut by_script = !cfg!(windows);
    if cfg!(windows) {
        let draggable = page.eval(&draggable_script(&from), false).await.map(|v| v.as_bool().unwrap_or(false))?;
        if draggable {
            if matches!(from, Endpoint::Point { .. }) {
                return Err("The start point is on an element the page made draggable (HTML5 drag and drop); drag it by ref instead.".into());
            }
            by_script = true;
            notes.push("the page uses HTML5 drag and drop, so the drag events were sent by script".into());
        }
    }

    if by_script {
        let Endpoint::Ref(source) = &from else {
            return Err(not_on_this_platform("Dragging from a point", "drag from a ref instead"));
        };
        page.helper::<Value>(&drag_to_call(source, &to)).await?;
        if !cfg!(windows) {
            notes.push("drag events were sent by script".into());
        }
    } else {
        // Resolve both ends. Scrolling to the second can move the first: shift it back by the scroll.
        let (sx, sy, note_from) = locate(&page, &from).await?;
        let view_start = view_of(&page).await?;
        let (ex, ey, note_to) = locate(&page, &to).await?;
        let view_end = view_of(&page).await?;
        let start = if matches!(from, Endpoint::Ref(_)) { shift_by((sx, sy), (view_start[0], view_start[1]), (view_end[0], view_end[1])) } else { (sx, sy) };
        if !on_screen(start, view_end[2], view_end[3]) {
            return Err(format!(
                "The start ({}) and the end ({}) do not fit in one view. Scroll so both show, then drag by points.",
                from.label(),
                to.label()
            )
            .into());
        }
        notes.extend(note_from);
        notes.extend(note_to);
        tolerate_dialog(page.cdp("Input.dispatchMouseEvent", mouse_event("mouseMoved", start.0, start.1, "none", 0, 0)).await)?;
        mouse(&page, "mousePressed", start.0, start.1, "left", 1, 1).await?;
        for (x, y) in drag_path(start, (ex, ey), DRAG_STEPS) {
            mouse(&page, "mouseMoved", x, y, "left", 1, 0).await?;
            // Pages that throttle to animation frames still see each step.
            settle(16).await;
        }
        tolerate_dialog(page.cdp("Input.dispatchMouseEvent", mouse_event("mouseReleased", ex, ey, "left", 0, 1)).await)?;
    }
    settle(300).await;
    let mut lines = vec![with_notes(&format!("Dragged {} to {}", from.label(), to.label()), &notes)];
    let state = match state_of(&page, &to).await {
        Some(state) => Some(state),
        None => state_of(&page, &from).await,
    };
    tail(&page, &mut lines, state).await;
    Ok(ToolOutput::text(lines.join("\n")))
}

/// `[scrollX, scrollY, innerWidth, innerHeight]` of the page.
async fn view_of(page: &Page) -> Result<[f64; 4], String> {
    let value = page.eval("[scrollX, scrollY, innerWidth, innerHeight]", false).await?;
    let numbers: Vec<f64> = value.as_array().map(|items| items.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    <[f64; 4]>::try_from(numbers).map_err(|_| "the page did not report its view".to_string())
}

// ── browser_click ───────────────────────────────────────────────────────────

async fn click(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let input = parse_click(args)?;
    let page = resolve_page(ctx, args)?;
    if !cfg!(windows) && (input.button != "left" || input.count > 1) {
        return Err(not_on_this_platform("Right, middle and repeated clicks", "a plain click by ref works"));
    }
    gate_page(ctx, &page, &click_summary(args, &input)).await?;
    core::click(ctx, &page, args, input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_defaults_to_one_left_click() {
        let input = parse_click(&json!({ "ref": "ref_1" })).unwrap();
        assert_eq!(input, ClickInput { button: "left", count: 1, verb: "Clicked" });
        assert_eq!(parse_click(&json!({ "doubleClick": true })).unwrap().count, 2);
        assert_eq!(parse_click(&json!({ "doubleClick": true })).unwrap().verb, "Double-clicked");
    }

    #[test]
    fn button_and_click_count_are_validated() {
        let right = parse_click(&json!({ "button": "right" })).unwrap();
        assert_eq!((right.button, right.count, right.verb), ("right", 1, "Right-clicked"));
        let triple = parse_click(&json!({ "clickCount": 3 })).unwrap();
        assert_eq!((triple.count, triple.verb), (3, "Triple-clicked"));
        assert_eq!(parse_click(&json!({ "button": "middle", "clickCount": 2 })).unwrap().verb, "Middle double-clicked");
        assert!(parse_click(&json!({ "button": "back" })).unwrap_err().contains("left, right or middle"));
        assert!(parse_click(&json!({ "button": 1 })).is_err());
        assert!(parse_click(&json!({ "clickCount": 0 })).is_err());
        assert!(parse_click(&json!({ "clickCount": 4 })).is_err());
        assert!(parse_click(&json!({ "clickCount": 1.5 })).is_err());
        assert!(parse_click(&json!({ "clickCount": "2" })).is_err());
        // clickCount wins over doubleClick.
        assert_eq!(parse_click(&json!({ "clickCount": 3, "doubleClick": true })).unwrap().count, 3);
    }

    #[test]
    fn the_gate_line_names_the_click() {
        let args = json!({ "ref": "ref_7", "button": "right" });
        assert_eq!(click_summary(&args, &parse_click(&args).unwrap()), "Right-click ref_7");
        let args = json!({ "x": 10.4, "y": 20.6, "clickCount": 2 });
        assert_eq!(click_summary(&args, &parse_click(&args).unwrap()), "Double-click at (10, 21)");
    }

    #[test]
    fn hover_needs_a_ref_or_a_point() {
        assert_eq!(hover_target(&json!({ "ref": "ref_2" })), Ok(Endpoint::Ref("ref_2".into())));
        assert_eq!(hover_target(&json!({ "x": 3, "y": 4.5 })), Ok(Endpoint::Point { x: 3.0, y: 4.5 }));
        assert_eq!(hover_target(&json!({ "ref": "ref_2", "x": 1, "y": 1 })), Ok(Endpoint::Ref("ref_2".into())), "a ref wins");
        assert!(hover_target(&json!({})).is_err());
        assert!(hover_target(&json!({ "x": 3 })).is_err());
        assert!(hover_target(&json!({ "ref": "  " })).is_err());
    }

    #[test]
    fn drag_endpoints_are_refs_or_points() {
        assert_eq!(parse_endpoint(Some(&json!("ref_1")), "from"), Ok(Endpoint::Ref("ref_1".into())));
        assert_eq!(parse_endpoint(Some(&json!({ "ref": "ref_9" })), "to"), Ok(Endpoint::Ref("ref_9".into())));
        assert_eq!(parse_endpoint(Some(&json!({ "x": 5, "y": 6 })), "to"), Ok(Endpoint::Point { x: 5.0, y: 6.0 }));
        assert!(parse_endpoint(Some(&json!({ "x": 5 })), "to").unwrap_err().starts_with("to needs"));
        assert!(parse_endpoint(None, "from").unwrap_err().starts_with("from needs"));
        assert!(parse_endpoint(Some(&json!(12)), "from").is_err());
        assert_eq!(Endpoint::Point { x: 119.6, y: 80.2 }.label(), "(120, 80)");
    }

    #[test]
    fn mouse_events_carry_button_and_state() {
        let moved = mouse_event("mouseMoved", 1.0, 2.0, "none", 0, 0);
        assert_eq!((moved["type"].as_str(), moved["button"].as_str(), moved["buttons"].as_u64()), (Some("mouseMoved"), Some("none"), Some(0)));
        let pressed = mouse_event("mousePressed", 1.0, 2.0, "left", 1, 1);
        assert_eq!((pressed["buttons"].as_u64(), pressed["clickCount"].as_u64()), (Some(1), Some(1)));
    }

    #[test]
    fn a_drag_moves_in_even_steps_and_ends_on_the_target() {
        let path = drag_path((0.0, 0.0), (80.0, 40.0), 4);
        assert_eq!(path, vec![(20.0, 10.0), (40.0, 20.0), (60.0, 30.0), (80.0, 40.0)]);
        assert_eq!(drag_path((5.0, 5.0), (5.0, 5.0), 0), vec![(5.0, 5.0)], "at least one move");
        assert_eq!(drag_path((0.0, 0.0), (1.0, 1.0), DRAG_STEPS).len(), DRAG_STEPS);
    }

    #[test]
    fn scrolling_moves_the_start_back_into_place() {
        // The page scrolled down 100 px while the end was brought into view: the start moved up 100 px.
        assert_eq!(shift_by((50.0, 300.0), (0.0, 0.0), (0.0, 100.0)), (50.0, 200.0));
        assert_eq!(shift_by((50.0, 300.0), (0.0, 100.0), (0.0, 100.0)), (50.0, 300.0));
        assert!(on_screen((50.0, 200.0), 800.0, 600.0));
        assert!(!on_screen((50.0, -10.0), 800.0, 600.0));
        assert!(!on_screen((900.0, 10.0), 800.0, 600.0));
    }

    #[test]
    fn a_covered_ref_is_explained() {
        let hit = Located { x: 1.0, y: 1.0, hits: true, covered: None };
        assert_eq!(cover_note("ref_1", &hit), None);
        let covered = Located { x: 1.0, y: 1.0, hits: false, covered: Some("dialog \"Cookies\"".into()) };
        assert!(cover_note("ref_1", &covered).unwrap().contains("covered by dialog \"Cookies\""));
        let off = Located { x: 1.0, y: 1.0, hits: false, covered: None };
        assert!(cover_note("ref_1", &off).unwrap().contains("not reachable"));
    }

    #[test]
    fn script_calls_quote_their_refs() {
        assert_eq!(drag_to_call("ref_1", &Endpoint::Ref("f1:ref_2".into())), "dragTo(\"ref_1\", \"f1:ref_2\")");
        assert_eq!(drag_to_call("ref_1", &Endpoint::Point { x: 10.0, y: 20.5 }), "dragTo(\"ref_1\", {\"x\":10.0,\"y\":20.5})");
        assert!(draggable_script(&Endpoint::Ref("ref_3".into())).contains("evoElement(\"ref_3\")"));
        assert!(draggable_script(&Endpoint::Point { x: 4.0, y: 5.0 }).contains("elementFromPoint(4, 5)"));
    }

    #[test]
    fn results_name_their_notes() {
        assert_eq!(with_notes("Hovered ref_1", &[]), "Hovered ref_1.");
        assert_eq!(with_notes("Hovered ref_1", &["a".into(), "b".into()]), "Hovered ref_1 (a; b).");
    }

    #[test]
    fn the_click_tool_replaces_core_with_the_same_name_and_class() {
        let names: Vec<_> = tools().iter().map(|t| t.name).collect();
        assert_eq!(names, ["browser_hover", "browser_drag", "browser_click"]);
        let click = &tools()[2];
        let schema = (click.schema)();
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["properties"]["button"]["enum"].as_array().unwrap().len() == 3);
        assert!(schema["properties"]["tabId"].is_object());
        assert_eq!(click.class, Class::Act);
    }
}
