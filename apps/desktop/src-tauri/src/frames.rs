//! Pictures for the cards' canvases: the frames of staged pages (`browser://frame`) and of the
//! app a chat's agent controls (`computer://frame`). Docs: floating-cards (FR-9),
//! computer-app-control (FR-10).
//!
//! A frame is a JPEG of about 130 KB. Sent as an event, it reaches the webview as a script that
//! holds the picture as base64 text, and the renderer keeps each script and its copies of the
//! text until the next garbage collection: with a few cards at ten frames a second the window
//! grew by hundreds of MB between collections. So each window that paints frames hands the shell
//! a [`Channel`] (`stage_frames`), and a frame goes down it as raw bytes, which the webview
//! fetches as an `ArrayBuffer` and decodes straight into a bitmap. A window that has not handed
//! one over yet still gets the event.
//!
//! The bytes of a frame: a little-endian `u32` with the length of a JSON header, the header
//! (`kind`, then the fields of the event without `data`), and the JPEG.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::Engine as _;
use serde_json::{Map, Value};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, Webview};

/// Each window's channel for frames, by window label.
#[derive(Default)]
pub struct FrameSinks(Mutex<HashMap<String, Channel<InvokeResponseBody>>>);

impl FrameSinks {
    /// The window `label` is gone: so is its channel.
    pub fn window_closed(&self, label: &str) {
        self.0.lock().expect("frame sinks lock").remove(label);
    }
}

/// This window paints frames: send them down `on_frame` from now on (a window that reloads
/// hands over a new one).
#[tauri::command]
pub fn stage_frames(webview: Webview, state: tauri::State<'_, FrameSinks>, on_frame: Channel<InvokeResponseBody>) {
    state.0.lock().expect("frame sinks lock").insert(webview.window().label().to_string(), on_frame);
}

/// Where a frame goes: one window, or every window.
pub enum To<'a> {
    Window(&'a str),
    All,
}

/// Send `payload` (an event payload whose `data` is a base64 JPEG) as `event`'s frame: as bytes
/// to the windows that handed over a channel, as the event to the others.
pub fn send(app: &AppHandle, to: To<'_>, event: &str, payload: Value) {
    let Some((header, image)) = split(event, &payload) else {
        emit(app, &to, event, payload);
        return;
    };
    let bytes = packet(&header, &image);
    let sinks = app.state::<FrameSinks>();
    let mut sinks = sinks.0.lock().expect("frame sinks lock");
    match to {
        To::Window(label) => match sinks.get(label) {
            Some(channel) => {
                if channel.send(InvokeResponseBody::Raw(bytes)).is_err() {
                    sinks.remove(label);
                }
            }
            None => {
                drop(sinks);
                let _ = app.emit_to(label, event, payload);
            }
        },
        To::All => {
            sinks.retain(|_, channel| channel.send(InvokeResponseBody::Raw(bytes.clone())).is_ok());
            let covered: Vec<String> = sinks.keys().cloned().collect();
            drop(sinks);
            // Windows that paint no frames yet (or never will) get the event; it costs only
            // the windows that listen for it.
            for label in app.windows().into_keys() {
                if !covered.contains(&label) {
                    let _ = app.emit_to(label.as_str(), event, payload.clone());
                }
            }
        }
    }
}

fn emit(app: &AppHandle, to: &To<'_>, event: &str, payload: Value) {
    let _ = match to {
        To::Window(label) => app.emit_to(*label, event, payload),
        To::All => app.emit(event, payload),
    };
}

/// The header (`kind` and the payload without `data`) and the JPEG of a frame payload; `None`
/// when it holds no picture.
fn split(event: &str, payload: &Value) -> Option<(Value, Vec<u8>)> {
    let fields = payload.as_object()?;
    let data = fields.get("data")?.as_str()?;
    let image = base64::engine::general_purpose::STANDARD.decode(data).ok()?;
    let mut header: Map<String, Value> = fields.iter().filter(|(key, _)| key.as_str() != "data").map(|(k, v)| (k.clone(), v.clone())).collect();
    header.insert("kind".into(), Value::String(kind(event).into()));
    Some((Value::Object(header), image))
}

/// `browser` for a page's frame, `app` for an app's.
fn kind(event: &str) -> &'static str {
    if event.starts_with("computer://") {
        "app"
    } else {
        "browser"
    }
}

/// The bytes sent for a frame: header length (`u32`, little-endian), header JSON, JPEG.
pub fn packet(header: &Value, image: &[u8]) -> Vec<u8> {
    let header = serde_json::to_vec(header).unwrap_or_else(|_| b"{}".to_vec());
    let mut bytes = Vec::with_capacity(4 + header.len() + image.len());
    bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(image);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn packet_puts_the_header_length_first() {
        let bytes = packet(&json!({ "tab": "browser-1", "seq": 2 }), &[0xff, 0xd8, 0xff]);
        let length = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let header: Value = serde_json::from_slice(&bytes[4..4 + length]).unwrap();
        assert_eq!(header, json!({ "tab": "browser-1", "seq": 2 }));
        assert_eq!(&bytes[4 + length..], &[0xff, 0xd8, 0xff]);
    }

    #[test]
    fn split_takes_the_picture_out_of_the_payload() {
        let payload = json!({ "tab": "browser-1", "seq": 3, "data": "/9j/", "width": 1280.0, "height": 800.0 });
        let (header, image) = split("browser://frame", &payload).unwrap();
        assert_eq!(header, json!({ "tab": "browser-1", "seq": 3, "width": 1280.0, "height": 800.0, "kind": "browser" }));
        assert_eq!(image, vec![0xff, 0xd8, 0xff]);

        let app = json!({ "surface": "app:c1", "chat": "c1", "seq": 1, "data": "/9j/", "title": "Notepad" });
        assert_eq!(split("computer://frame", &app).unwrap().0["kind"], "app");
    }

    #[test]
    fn split_refuses_a_payload_without_a_picture() {
        assert!(split("browser://frame", &json!({ "tab": "t", "seq": 1 })).is_none());
        assert!(split("browser://frame", &json!({ "tab": "t", "data": "not base64!" })).is_none());
        assert!(split("browser://frame", &json!("frame")).is_none());
    }
}
