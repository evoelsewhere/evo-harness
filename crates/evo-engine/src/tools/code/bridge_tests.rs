//! The program's view of a finished call (docs/features/code-mode/spec.md,
//! FR-5, FR-8). End-to-end bridge behaviour is in `tests.rs`.

use super::*;

#[test]
fn a_finished_call_gives_its_text_details_and_images() {
    let done = json!({
        "requestId": "tp-1", "status": "done", "isError": false,
        "result": {
            "content": [
                { "type": "text", "text": "one" },
                { "type": "image", "data": "aGk=", "mimeType": "image/png" },
                { "type": "text", "text": "two" }
            ],
            "details": { "exitCode": 0 }
        }
    });
    let (reply, images) = shape(&done);
    assert_eq!(
        reply,
        json!({ "ok": true, "text": "one\ntwo", "details": { "exitCode": 0 }, "images": 1 })
    );
    assert_eq!(images.len(), 1);
}

#[test]
fn errors_and_failures_are_not_ok() {
    let error = json!({
        "status": "done", "isError": true,
        "result": { "content": [{ "type": "text", "text": "Blocked by a hook: no" }] }
    });
    let (reply, _) = shape(&error);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["text"], "Blocked by a hook: no");

    let failed = json!({ "status": "failed", "error": "nested invocation cancelled" });
    let (reply, _) = shape(&failed);
    assert_eq!(
        reply,
        json!({ "ok": false, "text": "nested invocation cancelled" })
    );
}
