//! Notes added to a connected app's refusals so the agent knows what the
//! person must change (docs/features/connected-apps/spec.md, FR-15, FR-16).
//!
//! A service that refuses for missing permissions (Google's
//! `ACCESS_TOKEN_SCOPE_INSUFFICIENT`, "insufficient scope", a 403 naming a
//! scope) will refuse again after a reconnect: the sign-in Composio asks
//! for lacks that permission, so the person's own auth config in Composio
//! must ask for it.

/// Words that mark a missing-permission refusal.
const SCOPE_MARKS: [&str; 5] = [
    "access_token_scope_insufficient",
    "insufficient authentication scopes",
    "insufficient scope",
    "insufficient_scope",
    "insufficientpermissions",
];

/// Whether `text` (a tool result) is a refusal for missing permissions.
pub fn is_scope_refusal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    SCOPE_MARKS.iter().any(|m| lower.contains(m))
        || (lower.contains("403") && lower.contains("scope"))
}

/// The note for a missing-permission refusal of `service`.
pub fn scope_note(service: &str) -> String {
    format!(
        "[Evo] {service} refused because the connection lacks a permission (scope) this tool needs. Reconnecting alone will not help: the person must add that scope to their own {service} auth config in Composio (their project → Auth Configs → {service} → Scopes, using their own OAuth app), turn it on for Tool Router, then Reconnect {service} in Connected apps. Tell the person this; do not retry."
    )
}

/// `text` with the scope note when it is a scope refusal.
pub fn annotate(text: &str, service: &str) -> Option<String> {
    is_scope_refusal(text).then(|| scope_note(service))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_refusal_names_the_step() {
        let fixture = include_str!("testdata/scope_refusal.json");
        assert!(is_scope_refusal(fixture));
        let note = annotate(fixture, "Gmail").unwrap();
        assert!(note.contains("Reconnecting alone will not help"), "{note}");
        assert!(note.contains("auth config in Composio"), "{note}");
        assert!(annotate("{\"successful\": true, \"data\": {}}", "Gmail").is_none());
        assert!(annotate("403 Forbidden: rate limited", "Gmail").is_none());
    }
}
