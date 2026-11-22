//! Keeps the images in a provider request within a byte and count budget.
//!
//! A long browsing or computer-use run leaves dozens of screenshots in the
//! history. Each is a base64 block, so the request body can pass a provider's
//! size limit (HTTP 413) long before the token estimate says the context is
//! full. This trims the *oldest* images from the request only: the stored
//! session is untouched. A trimmed image becomes a short text placeholder so
//! the transcript the model reads stays coherent.
//!
//! The budget starts generous and is lowered one level each time a provider
//! answers "request too large" (see `agent_harness` retry loop).

use std::sync::atomic::{AtomicU32, Ordering};

use evo_ai::types::{Content, Context, Message, UserContent};

/// Request image budget at level 0.
const BASE_BYTES: usize = 8 * 1024 * 1024;
const BASE_COUNT: usize = 24;
/// At this level every image is dropped.
const MAX_LEVEL: u32 = 5;

const PLACEHOLDER: &str = "[An older image was removed to keep the request small.]";

/// Shared, per run: how strictly images are trimmed.
#[derive(Debug, Default)]
pub struct ImageBudget {
    level: AtomicU32,
}

impl ImageBudget {
    pub fn level(&self) -> u32 {
        self.level.load(Ordering::Relaxed)
    }

    /// Lower the budget after a "request too large" answer. Returns the new level.
    pub fn tighten(&self) -> u32 {
        let next = (self.level() + 1).min(MAX_LEVEL);
        self.level.store(next, Ordering::Relaxed);
        next
    }

    fn limits(&self) -> (usize, usize) {
        let level = self.level();
        if level >= MAX_LEVEL {
            (0, 0)
        } else {
            (BASE_COUNT >> level, BASE_BYTES >> level)
        }
    }

    /// The context with its oldest images replaced, or `None` when it already fits.
    pub fn trim(&self, ctx: &Context) -> Option<Context> {
        let (max_count, max_bytes) = self.limits();
        trim_images(ctx, max_count, max_bytes)
    }
}

/// Keep the newest images while both limits hold; replace the rest.
pub fn trim_images(ctx: &Context, max_count: usize, max_bytes: usize) -> Option<Context> {
    let mut count = 0usize;
    let mut bytes = 0usize;
    for message in &ctx.messages {
        for block in blocks(message) {
            if let Content::Image(image) = block {
                count += 1;
                bytes += image.data.len();
            }
        }
    }
    if count <= max_count && bytes <= max_bytes {
        return None;
    }

    let mut out = ctx.clone();
    let (mut kept, mut kept_bytes) = (0usize, 0usize);
    for message in out.messages.iter_mut().rev() {
        let Some(blocks) = blocks_mut(message) else {
            continue;
        };
        for block in blocks.iter_mut().rev() {
            let Content::Image(image) = block else {
                continue;
            };
            if kept < max_count && kept_bytes + image.data.len() <= max_bytes {
                kept += 1;
                kept_bytes += image.data.len();
            } else {
                *block = Content::text(PLACEHOLDER);
            }
        }
    }
    Some(out)
}

fn blocks(message: &Message) -> &[Content] {
    match message {
        Message::User(u) => match &u.content {
            UserContent::Blocks(b) => b,
            UserContent::Text(_) => &[],
        },
        Message::ToolResult(t) => &t.content,
        Message::Assistant(_) => &[],
    }
}

fn blocks_mut(message: &mut Message) -> Option<&mut Vec<Content>> {
    match message {
        Message::User(u) => match &mut u.content {
            UserContent::Blocks(b) => Some(b),
            UserContent::Text(_) => None,
        },
        Message::ToolResult(t) => Some(&mut t.content),
        Message::Assistant(_) => None,
    }
}

/// True for a provider answer meaning the request body was too big.
pub fn is_request_too_large(error: &str) -> bool {
    let lower = error.to_lowercase();
    [
        "413",
        "request entity too large",
        "payload too large",
        "request too large",
        "request_too_large",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_ai::types::{ImageContent, ImageContentType, ToolResultMessage, ToolResultRole};

    fn image(size: usize) -> Content {
        Content::Image(ImageContent {
            kind: ImageContentType,
            data: "A".repeat(size).into(),
            mime_type: "image/jpeg".into(),
        })
    }

    fn tool_result(content: Vec<Content>) -> Message {
        Message::ToolResult(Box::new(ToolResultMessage {
            role: ToolResultRole,
            tool_call_id: "c".into(),
            tool_name: "browser_screenshot".into(),
            content,
            details: None,
            usage: None,
            added_tool_names: Vec::new(),
            is_error: false,
            timestamp: 0,
        }))
    }

    fn images_left(ctx: &Context) -> usize {
        ctx.messages
            .iter()
            .flat_map(blocks)
            .filter(|b| matches!(b, Content::Image(_)))
            .count()
    }

    #[test]
    fn within_budget_is_borrowed_unchanged() {
        let ctx = Context::new(vec![tool_result(vec![image(10)])]);
        assert!(trim_images(&ctx, 4, 1000).is_none());
    }

    #[test]
    fn oldest_images_go_first_by_count() {
        let ctx = Context::new((0..5).map(|_| tool_result(vec![image(10)])).collect());
        let out = trim_images(&ctx, 2, usize::MAX).unwrap();
        assert_eq!(images_left(&out), 2);
        // The two newest survive; the older three became text.
        assert!(matches!(blocks(&out.messages[4])[0], Content::Image(_)));
        assert!(matches!(blocks(&out.messages[3])[0], Content::Image(_)));
        assert!(matches!(blocks(&out.messages[2])[0], Content::Text(_)));
        // Input is not modified.
        assert_eq!(images_left(&ctx), 5);
    }

    #[test]
    fn bytes_limit_applies_too() {
        let ctx = Context::new((0..4).map(|_| tool_result(vec![image(100)])).collect());
        let out = trim_images(&ctx, 100, 250).unwrap();
        assert_eq!(images_left(&out), 2);
    }

    #[test]
    fn tightening_ends_with_no_images_and_stays_there() {
        let budget = ImageBudget::default();
        let ctx = Context::new(vec![tool_result(vec![image(10)])]);
        assert!(budget.trim(&ctx).is_none());
        for _ in 0..10 {
            budget.tighten();
        }
        assert_eq!(images_left(&budget.trim(&ctx).unwrap()), 0);
    }

    #[test]
    fn recognises_request_too_large() {
        assert!(is_request_too_large(
            "http error 413: Request Entity Too Large"
        ));
        assert!(is_request_too_large("Payload Too Large"));
        assert!(!is_request_too_large("http error 429: slow down"));
    }
}
