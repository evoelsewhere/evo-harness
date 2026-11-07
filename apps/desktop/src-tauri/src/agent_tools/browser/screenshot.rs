//! `browser_screenshot` (docs/features/agent-browser FR-3, NFR-2, T15): the
//! visible part of the page, the whole page (`fullPage`), one element (`ref`),
//! or a region with a zoom (`region`, `scale`), as JPEG or PNG.
//!
//! The picture is scaled so its long side is at most 1568 px. The geometry
//! (what to clip, at which scale) is plain arithmetic in [`plan_capture`]; the actual
//! size of the image the browser returned is read from its header and, if the
//! browser disagrees with the arithmetic, the shot is retaken once at the
//! corrected scale. DevTools only (Windows); elsewhere the tool says so.

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};

use super::driver::{finish, js, resolve_page, settle, show, show_browser, Page, ToolResult};
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};

// ── Schema ──────────────────────────────────────────────────────────────────

const SCREENSHOT: &str = "A screenshot of the page in the built-in browser: the visible part (default), the whole page (fullPage), one element (ref), or a region (region {x, y, width, height} in CSS pixels of the visible part). scale zooms the picture (a region at scale 2 shows more detail); every picture is at most 1568 px on its long side, so for detail on a long page use ref or region. format jpeg (default) or png.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "fullPage": { "type": "boolean", "description": "Capture the whole page, not only the visible part." },
            "ref": { "type": "string", "description": "Capture this element (a ref from browser_read_page / browser_find)." },
            "region": {
                "type": "object",
                "description": "Capture this rectangle, in CSS pixels of the visible part (as for browser_click).",
                "properties": { "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" } },
                "required": ["x", "y", "width", "height"],
                "additionalProperties": false,
            },
            "scale": { "type": "number", "description": "Zoom, 0.1 to 4 (default 1): 2 gives twice the pixels per CSS pixel. Capped so the long side stays at most 1568 px." },
            "format": { "type": "string", "enum": ["jpeg", "png"], "description": "Default jpeg." },
            "quality": { "type": "number", "description": "JPEG quality 1 to 100 (default 80)." },
            "tabId": { "type": "string", "description": "A tab id from browser_tabs; default: the active tab." },
        },
        "additionalProperties": false,
    })
}

fn run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(screenshot(&ctx, &args).await) })
}

/// The `browser_screenshot` that replaces core's.
pub fn tools() -> Vec<Tool> {
    vec![Tool { name: "browser_screenshot", description: SCREENSHOT, schema, class: Class::Read, run }]
}

// ── Arguments ───────────────────────────────────────────────────────────────

/// The longest side of a picture, in pixels.
pub const MAX_SIDE: f64 = 1568.0;
pub const DEFAULT_QUALITY: u32 = 80;
const MIN_SCALE: f64 = 0.1;
const MAX_SCALE: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Subject {
    Viewport,
    FullPage,
    Element(String),
    Region(Rect),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Format::Jpeg => "jpeg",
            Format::Png => "png",
        }
    }

    fn mime(self) -> &'static str {
        match self {
            Format::Jpeg => "image/jpeg",
            Format::Png => "image/png",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub subject: Subject,
    /// The zoom the caller asked for.
    pub scale: Option<f64>,
    pub format: Format,
    pub quality: u32,
}

fn number(value: Option<&Value>, what: &str) -> Result<Option<f64>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_f64() {
            Some(n) if n.is_finite() => Ok(Some(n)),
            _ => Err(format!("{what} must be a number.")),
        },
    }
}

fn parse_region(value: &Value) -> Result<Rect, String> {
    let Some(map) = value.as_object() else {
        return Err("region must be {x, y, width, height}.".into());
    };
    let get = |key: &str| number(map.get(key), &format!("region.{key}"))?.ok_or_else(|| format!("region needs {key}."));
    let rect = Rect { x: get("x")?, y: get("y")?, width: get("width")?, height: get("height")? };
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Err("region width and height must be more than 0.".into());
    }
    Ok(rect)
}

pub fn parse_args(args: &Value) -> Result<Options, String> {
    let full = match args.get("fullPage") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => return Err("fullPage must be true or false.".into()),
    };
    let reference = match args.get("ref") {
        None | Some(Value::Null) => None,
        Some(Value::String(r)) if r.trim().is_empty() => None,
        Some(Value::String(r)) => Some(r.trim().to_string()),
        Some(_) => return Err("ref must be a string.".into()),
    };
    let region = match args.get("region") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_region(value)?),
    };
    if [full, reference.is_some(), region.is_some()].iter().filter(|given| **given).count() > 1 {
        return Err("Use only one of fullPage, ref and region.".into());
    }
    let subject = if full {
        Subject::FullPage
    } else if let Some(reference) = reference {
        Subject::Element(reference)
    } else if let Some(region) = region {
        Subject::Region(region)
    } else {
        Subject::Viewport
    };
    let scale = number(args.get("scale"), "scale")?;
    if let Some(scale) = scale {
        if scale <= 0.0 {
            return Err("scale must be more than 0.".into());
        }
    }
    let format = match args.get("format") {
        None | Some(Value::Null) => Format::Jpeg,
        Some(Value::String(f)) if f == "jpeg" || f == "jpg" => Format::Jpeg,
        Some(Value::String(f)) if f == "png" => Format::Png,
        Some(_) => return Err("format must be jpeg or png.".into()),
    };
    let quality = match number(args.get("quality"), "quality")? {
        None => DEFAULT_QUALITY,
        Some(q) => q.round().clamp(1.0, 100.0) as u32,
    };
    Ok(Options { subject, scale: scale.map(|s| s.clamp(MIN_SCALE, MAX_SCALE)), format, quality })
}

// ── Geometry ────────────────────────────────────────────────────────────────

/// What the page looks like now, in CSS pixels (the page scroll is where the visible part starts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub dpr: f64,
    pub page_x: f64,
    pub page_y: f64,
    pub view_w: f64,
    pub view_h: f64,
    pub doc_w: f64,
    pub doc_h: f64,
}

impl Metrics {
    /// From the answer of `Page.getLayoutMetrics` and the device pixel ratio.
    pub fn from_layout(layout: &Value, dpr: f64) -> Result<Self, String> {
        let viewport = layout.get("cssVisualViewport").or_else(|| layout.get("visualViewport"));
        let content = layout.get("cssContentSize").or_else(|| layout.get("contentSize"));
        let get = |value: Option<&Value>, key: &str| value.and_then(|v| v.get(key)).and_then(Value::as_f64).filter(|n| n.is_finite());
        let (Some(view_w), Some(view_h)) = (get(viewport, "clientWidth"), get(viewport, "clientHeight")) else {
            return Err("The browser did not report the page's size.".into());
        };
        let (doc_w, doc_h) = (get(content, "width").unwrap_or(view_w), get(content, "height").unwrap_or(view_h));
        Ok(Self {
            dpr: if dpr.is_finite() && dpr > 0.0 { dpr } else { 1.0 },
            page_x: get(viewport, "pageX").unwrap_or(0.0),
            page_y: get(viewport, "pageY").unwrap_or(0.0),
            view_w,
            view_h,
            doc_w: doc_w.max(view_w),
            doc_h: doc_h.max(view_h),
        })
    }
}

/// A `Page.captureScreenshot` clip, in CSS pixels of the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// How to take one picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub clip: Clip,
    /// The clip is needed (the plain visible part is taken without one).
    pub clipped: bool,
    pub beyond_viewport: bool,
    /// The part of the page in the picture, CSS pixels.
    pub css_w: f64,
    pub css_h: f64,
    /// The picture's expected size in pixels.
    pub out_w: u32,
    pub out_h: u32,
    pub dpr: f64,
}

fn pixels(css: f64, scale: f64, dpr: f64) -> u32 {
    (css * scale * dpr).round().max(1.0) as u32
}

/// Decide what to clip and at which scale. `element` is the box of an element or the region, in CSS
/// pixels of the visible part; `scale` is the zoom the caller asked for.
pub fn plan_capture(subject: &Subject, area: Option<Rect>, m: &Metrics, scale: Option<f64>) -> Result<Plan, String> {
    let (x, y, w, h, beyond) = match subject {
        Subject::Viewport => (m.page_x, m.page_y, m.view_w, m.view_h, false),
        Subject::FullPage => (0.0, 0.0, m.doc_w, m.doc_h, true),
        Subject::Element(_) | Subject::Region(_) => {
            let r = area.ok_or_else(|| "no area to capture".to_string())?;
            let (x0, y0) = ((m.page_x + r.x).max(0.0), (m.page_y + r.y).max(0.0));
            let (x1, y1) = ((m.page_x + r.x + r.width).min(m.doc_w), (m.page_y + r.y + r.height).min(m.doc_h));
            if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                return Err("the area is empty or outside the page".into());
            }
            let outside = r.x < 0.0 || r.y < 0.0 || r.x + r.width > m.view_w || r.y + r.height > m.view_h;
            (x0, y0, x1 - x0, y1 - y0, outside)
        }
    };
    let user = scale.unwrap_or(1.0);
    let fit = MAX_SIDE / (w.max(h) * m.dpr);
    let applied = user.min(fit);
    let clipped = !(matches!(subject, Subject::Viewport) && scale.is_none() && fit >= 1.0);
    Ok(Plan {
        clip: Clip { x, y, width: w, height: h, scale: applied },
        clipped,
        beyond_viewport: beyond,
        css_w: w,
        css_h: h,
        out_w: pixels(w, applied, m.dpr),
        out_h: pixels(h, applied, m.dpr),
        dpr: m.dpr,
    })
}

impl Plan {
    /// The plan again at `scale`, always clipped.
    pub fn at_scale(self, scale: f64) -> Plan {
        Plan {
            clip: Clip { scale, ..self.clip },
            clipped: true,
            out_w: pixels(self.css_w, scale, self.dpr),
            out_h: pixels(self.css_h, scale, self.dpr),
            ..self
        }
    }

    /// The parameters of `Page.captureScreenshot`.
    pub fn params(&self, format: Format, quality: u32) -> Value {
        let mut params = json!({ "format": format.name() });
        if format == Format::Jpeg {
            params["quality"] = json!(quality);
        }
        if self.clipped {
            let c = self.clip;
            params["clip"] = json!({ "x": c.x, "y": c.y, "width": c.width, "height": c.height, "scale": c.scale });
        }
        if self.beyond_viewport {
            params["captureBeyondViewport"] = json!(true);
        }
        params
    }
}

/// The scale that brings a picture of `actual` pixels (taken at `scale`) down to the size limit; `None` when it fits.
pub fn corrected_scale(scale: f64, actual: (u32, u32)) -> Option<f64> {
    let long = f64::from(actual.0.max(actual.1));
    if long > MAX_SIDE * 1.02 {
        Some(scale * MAX_SIDE / long)
    } else {
        None
    }
}

// ── The picture's own size ──────────────────────────────────────────────────

/// The width and height of a PNG or JPEG given as base64, from its header.
pub fn image_size(data: &str) -> Option<(u32, u32)> {
    // The header is in the first few kilobytes: decode a multiple of four characters of it.
    let take = data.len().min(87_380);
    let head = data.get(..take - take % 4)?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(head).ok()?;
    let be16 = |i: usize| bytes.get(i..i + 2).map(|b| u32::from(u16::from_be_bytes([b[0], b[1]])));
    let be32 = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some((be32(16)?, be32(20)?));
    }
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        match marker {
            0xFF => i += 1,
            0x01 | 0xD0..=0xD8 => i += 2,
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => return Some((be16(i + 7)?, be16(i + 5)?)),
            _ => i += 2 + be16(i + 2)? as usize,
        }
    }
    None
}

// ── The tool ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Located {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

async fn capture(page: &Page, options: &Options, plan: &Plan) -> Result<String, String> {
    let shot = page.cdp("Page.captureScreenshot", plan.params(options.format, options.quality)).await?;
    let data = show(shot.get("data"));
    if data.is_empty() || data == "undefined" {
        return Err("The browser returned no image.".into());
    }
    Ok(data)
}

fn what_is_shown(subject: &Subject) -> String {
    match subject {
        Subject::Viewport => "the visible part".into(),
        Subject::FullPage => "the whole page".into(),
        Subject::Element(reference) => format!("element {reference}"),
        Subject::Region(_) => "a region".into(),
    }
}

/// The line under the picture: what it shows, its size, and a hint when it was scaled down a lot.
pub fn caption(subject: &Subject, plan: &Plan, actual: (u32, u32), scale: f64) -> String {
    let mut line = format!("Screenshot of {} ({}x{} CSS px, picture {}x{} px", what_is_shown(subject), plan.css_w.round(), plan.css_h.round(), actual.0, actual.1);
    if scale < 0.95 {
        line.push_str(&format!(", scaled to {:.2}x", scale));
    }
    line.push(')');
    if scale < 0.5 {
        line.push_str("; small text may not be readable: use ref or region (with scale) for detail");
    }
    line.push('.');
    line
}

async fn screenshot(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let options = parse_args(args)?;
    let page = resolve_page(ctx, args)?;
    show_browser(ctx, Some(&page.tab)).await;
    if !cfg!(windows) {
        return Err("Screenshots are not available on this platform yet; use browser_read_page.".into());
    }
    // An element is scrolled into view first; its box is where it sits then.
    let mut area = match &options.subject {
        Subject::Element(reference) => {
            let at = page.helper::<Located>(&format!("point({})", js(reference))).await?;
            if at.width < 1.0 || at.height < 1.0 {
                return Err(format!("{reference} has no visible box to capture.").into());
            }
            Some(Rect { x: at.x - at.width / 2.0, y: at.y - at.height / 2.0, width: at.width, height: at.height })
        }
        Subject::Region(rect) => Some(*rect),
        _ => None,
    };
    settle(250).await;
    let dpr = page.eval("window.devicePixelRatio || 1", false).await?.as_f64().unwrap_or(1.0);
    let layout = page.cdp("Page.getLayoutMetrics", json!({})).await?;
    let metrics = Metrics::from_layout(&layout, dpr)?;
    if matches!(options.subject, Subject::Element(_)) {
        // The page may have scrolled again while it settled: aim again.
        if let Subject::Element(reference) = &options.subject {
            if let Ok(at) = page.helper::<Located>(&format!("point({})", js(reference))).await {
                area = Some(Rect { x: at.x - at.width / 2.0, y: at.y - at.height / 2.0, width: at.width, height: at.height });
            }
        }
    }
    let mut plan = plan_capture(&options.subject, area, &metrics, options.scale).map_err(|why| match &options.subject {
        Subject::Element(reference) => format!("{reference}: {why}."),
        _ => format!("The region: {why}."),
    })?;
    let mut data = capture(&page, &options, &plan).await?;
    let mut actual = image_size(&data).unwrap_or((plan.out_w, plan.out_h));
    // The browser decides the pixel size: retake once, smaller, if it came out too big.
    if let Some(scale) = corrected_scale(plan.clip.scale, actual) {
        plan = plan.at_scale(scale);
        data = capture(&page, &options, &plan).await?;
        actual = image_size(&data).unwrap_or((plan.out_w, plan.out_h));
    }
    let note = caption(&options.subject, &plan, actual, plan.clip.scale);
    Ok(ToolOutput::image(data, options.format.mime(), format!("{note}\n{}", page.summary().await)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> Metrics {
        Metrics { dpr: 1.0, page_x: 0.0, page_y: 300.0, view_w: 1000.0, view_h: 700.0, doc_w: 1000.0, doc_h: 5000.0 }
    }

    #[test]
    fn no_arguments_is_the_visible_part_as_jpeg() {
        let o = parse_args(&json!({})).unwrap();
        assert_eq!(o, Options { subject: Subject::Viewport, scale: None, format: Format::Jpeg, quality: 80 });
    }

    #[test]
    fn the_subjects_are_exclusive_and_checked() {
        assert_eq!(parse_args(&json!({ "fullPage": true })).unwrap().subject, Subject::FullPage);
        assert_eq!(parse_args(&json!({ "ref": " ref_2 " })).unwrap().subject, Subject::Element("ref_2".into()));
        let region = parse_args(&json!({ "region": { "x": 1, "y": 2, "width": 30, "height": 40 }, "scale": 2 })).unwrap();
        assert_eq!(region.subject, Subject::Region(Rect { x: 1.0, y: 2.0, width: 30.0, height: 40.0 }));
        assert_eq!(region.scale, Some(2.0));
        assert!(parse_args(&json!({ "fullPage": true, "ref": "ref_1" })).unwrap_err().contains("only one"));
        assert!(parse_args(&json!({ "ref": "ref_1", "region": { "x": 0, "y": 0, "width": 1, "height": 1 } })).is_err());
        assert!(parse_args(&json!({ "fullPage": false, "ref": "ref_1" })).is_ok());
        assert!(parse_args(&json!({ "fullPage": "yes" })).is_err());
        assert!(parse_args(&json!({ "region": { "x": 0, "y": 0, "width": 0, "height": 10 } })).unwrap_err().contains("more than 0"));
        assert!(parse_args(&json!({ "region": { "x": 0, "y": 0, "width": 5 } })).unwrap_err().contains("needs height"));
        assert!(parse_args(&json!({ "region": [1, 2] })).is_err());
        assert!(parse_args(&json!({ "scale": 0 })).is_err());
        assert!(parse_args(&json!({ "scale": "big" })).is_err());
        assert_eq!(parse_args(&json!({ "scale": 40 })).unwrap().scale, Some(4.0), "a huge zoom is clamped");
        assert!(parse_args(&json!({ "format": "gif" })).is_err());
        let png = parse_args(&json!({ "format": "png", "quality": 500 })).unwrap();
        assert_eq!((png.format, png.quality), (Format::Png, 100));
    }

    #[test]
    fn the_visible_part_is_taken_without_a_clip_when_it_fits() {
        let p = plan_capture(&Subject::Viewport, None, &metrics(), None).unwrap();
        assert!(!p.clipped);
        assert_eq!((p.out_w, p.out_h), (1000, 700));
        assert_eq!(p.params(Format::Jpeg, 80), json!({ "format": "jpeg", "quality": 80 }));
        assert_eq!(p.params(Format::Png, 80), json!({ "format": "png" }));
    }

    #[test]
    fn a_big_view_is_scaled_down_to_the_limit() {
        let m = Metrics { dpr: 2.0, view_w: 1920.0, view_h: 1080.0, page_y: 0.0, doc_h: 1080.0, doc_w: 1920.0, ..metrics() };
        let p = plan_capture(&Subject::Viewport, None, &m, None).unwrap();
        assert!(p.clipped, "scaling needs a clip");
        assert!((p.clip.scale - 1568.0 / 3840.0).abs() < 1e-9);
        assert_eq!(p.out_w, 1568);
        assert!(p.out_h <= 1568);
        let params = p.params(Format::Jpeg, 80);
        assert_eq!(params["clip"]["width"], 1920.0);
        assert!(params.get("captureBeyondViewport").is_none());
    }

    #[test]
    fn a_full_page_is_the_content_size_beyond_the_viewport() {
        let p = plan_capture(&Subject::FullPage, None, &metrics(), None).unwrap();
        assert!(p.beyond_viewport && p.clipped);
        assert_eq!((p.clip.x, p.clip.y, p.clip.width, p.clip.height), (0.0, 0.0, 1000.0, 5000.0));
        // 5000 px tall: the long side is brought down to 1568.
        assert!((p.clip.scale - 1568.0 / 5000.0).abs() < 1e-9);
        assert_eq!(p.out_h, 1568);
        assert_eq!(p.out_w, 314);
        assert_eq!(p.params(Format::Png, 80)["captureBeyondViewport"], true);
        // A page not taller than the limit keeps its size, and still is "taller than the viewport".
        let short = Metrics { doc_h: 1200.0, ..metrics() };
        let p = plan_capture(&Subject::FullPage, None, &short, None).unwrap();
        assert_eq!((p.out_w, p.out_h), (1000, 1200));
        assert!(p.out_h > 700);
    }

    #[test]
    fn a_region_is_in_view_coordinates_and_zooms() {
        let region = Rect { x: 100.0, y: 50.0, width: 200.0, height: 100.0 };
        let p = plan_capture(&Subject::Region(region), Some(region), &metrics(), Some(3.0)).unwrap();
        // The view starts at page y 300: the region sits at 350 in the page.
        assert_eq!((p.clip.x, p.clip.y, p.clip.width, p.clip.height), (100.0, 350.0, 200.0, 100.0));
        assert_eq!((p.out_w, p.out_h), (600, 300));
        assert!(!p.beyond_viewport);
    }

    #[test]
    fn a_zoom_never_passes_the_size_limit() {
        let region = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
        let p = plan_capture(&Subject::Region(region), Some(region), &metrics(), Some(4.0)).unwrap();
        assert_eq!(p.out_w, 1568);
        assert_eq!(p.out_h, 1176);
    }

    #[test]
    fn a_region_is_cut_to_the_page_and_may_not_be_empty() {
        let hanging = Rect { x: 900.0, y: 600.0, width: 300.0, height: 300.0 };
        let p = plan_capture(&Subject::Region(hanging), Some(hanging), &metrics(), None).unwrap();
        assert_eq!((p.clip.width, p.clip.height), (100.0, 300.0), "cut at the right edge of the page");
        assert!(p.beyond_viewport, "it reaches below the visible part");
        let outside = Rect { x: 2000.0, y: 0.0, width: 50.0, height: 50.0 };
        assert!(plan_capture(&Subject::Region(outside), Some(outside), &metrics(), None).is_err());
        let above = Rect { x: 0.0, y: -900.0, width: 50.0, height: 50.0 };
        assert!(plan_capture(&Subject::Region(above), Some(above), &metrics(), None).is_err(), "above the top of the page");
    }

    #[test]
    fn metrics_are_read_from_the_layout() {
        let layout = json!({
            "cssVisualViewport": { "pageX": 0, "pageY": 120.5, "clientWidth": 800, "clientHeight": 600 },
            "cssContentSize": { "width": 790, "height": 3000 },
        });
        let m = Metrics::from_layout(&layout, 1.25).unwrap();
        assert_eq!((m.page_y, m.view_w, m.doc_h, m.dpr), (120.5, 800.0, 3000.0, 1.25));
        assert_eq!(m.doc_w, 800.0, "the page is at least as wide as the view");
        assert!(Metrics::from_layout(&json!({}), 1.0).is_err());
        assert_eq!(Metrics::from_layout(&layout, f64::NAN).unwrap().dpr, 1.0);
    }

    #[test]
    fn an_oversize_picture_is_retaken_smaller() {
        assert_eq!(corrected_scale(1.0, (1500, 900)), None);
        assert_eq!(corrected_scale(1.0, (1590, 900)), None, "a hair over is tolerated");
        let s = corrected_scale(1.0, (3136, 2000)).unwrap();
        assert!((s - 0.5).abs() < 1e-9);
        assert!((corrected_scale(0.5, (3136, 2000)).unwrap() - 0.25).abs() < 1e-9);
        let p = plan_capture(&Subject::FullPage, None, &metrics(), None).unwrap().at_scale(0.25);
        assert_eq!((p.out_w, p.out_h), (250, 1250));
        assert!(p.clipped);
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn the_size_of_a_png_is_in_its_header() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert_eq!(image_size(&b64(&png)), Some((640, 480)));
    }

    #[test]
    fn the_size_of_a_jpeg_is_in_its_frame_header() {
        // SOI, an APP0 segment (length 16), then SOF0: precision, height 300, width 500.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        jpeg.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8]);
        jpeg.extend_from_slice(&300u16.to_be_bytes());
        jpeg.extend_from_slice(&500u16.to_be_bytes());
        jpeg.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        assert_eq!(image_size(&b64(&jpeg)), Some((500, 300)));
        assert_eq!(image_size(&b64(b"GIF89a....")), None);
        assert_eq!(image_size("not base64!!"), None);
        assert_eq!(image_size(""), None);
    }

    #[test]
    fn the_caption_says_what_and_how_big() {
        let p = plan_capture(&Subject::FullPage, None, &metrics(), None).unwrap();
        let line = caption(&Subject::FullPage, &p, (314, 1568), p.clip.scale);
        assert!(line.starts_with("Screenshot of the whole page (1000x5000 CSS px, picture 314x1568 px, scaled to 0.31x)"));
        assert!(line.contains("use ref or region"));
        let v = plan_capture(&Subject::Viewport, None, &metrics(), None).unwrap();
        assert_eq!(caption(&Subject::Viewport, &v, (1000, 700), 1.0), "Screenshot of the visible part (1000x700 CSS px, picture 1000x700 px).");
    }

    #[test]
    fn the_tool_replaces_core_with_an_observe_tool() {
        let tool = &tools()[0];
        assert_eq!((tool.name, tool.class), ("browser_screenshot", Class::Read));
        let schema = (tool.schema)();
        assert_eq!(schema["additionalProperties"], false);
        for key in ["fullPage", "ref", "region", "scale", "format", "tabId"] {
            assert!(schema["properties"][key].is_object(), "{key}");
        }
    }
}
