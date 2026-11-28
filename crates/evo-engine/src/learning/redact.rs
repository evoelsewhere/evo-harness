//! Secrets never reach memory: text shaped like a key, token, password or
//! private key is replaced by a marker before anything is written
//! (docs/features/learning-layers/spec.md, FR-9; plan 3.2).
//!
//! The shapes are deliberately narrow — known key prefixes, JWTs, PEM blocks,
//! and a value after `password`, `token`, `api_key` or `secret` — so that
//! ordinary prose and code about passwords and tokens stay as written.

use std::sync::OnceLock;

use regex::{Captures, Regex};

/// The marker a secret is replaced with.
pub fn marker(chars: usize) -> String {
    format!("«redacted {chars} chars»")
}

struct Shapes {
    /// The whole match is the secret.
    whole: Vec<Regex>,
    /// `label` then the secret in group `value`.
    labelled: Vec<Regex>,
}

fn shapes() -> &'static Shapes {
    static SHAPES: OnceLock<Shapes> = OnceLock::new();
    SHAPES.get_or_init(|| {
        let whole = [
            // PEM private keys, to their end line (or the end of the text).
            r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z0-9 ]*PRIVATE KEY-----|\z)",
            // Provider keys: sk-…, sk-ant-…, sk_live_…, rk_live_…
            r"\b(?:sk|rk|pk)[-_](?:live_|test_|proj-|ant-)?[A-Za-z0-9_\-]{16,}",
            // GitHub tokens.
            r"\bgh[pousr]_[A-Za-z0-9]{20,}",
            r"\bgithub_pat_[A-Za-z0-9_]{20,}",
            // Slack tokens.
            r"\bxox[abposr]-[A-Za-z0-9\-]{10,}",
            // AWS access key ids.
            r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
            // Google API keys.
            r"\bAIza[0-9A-Za-z_\-]{30,}",
            // JSON web tokens.
            r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}",
        ];
        let labelled = [
            // password: hunter22, pwd=…, passcode "…"
            r#"(?i)\b(?:password|passwd|pwd|passcode|passphrase)\b\s*(?:is\s+|[:=]\s*)["'`]?(?P<value>[^\s"'`<>${}*][^\s"'`]{3,})"#,
            // token=…, api_key: …, client_secret = …, access key: … (a value with a digit)
            r#"(?i)\b(?:token|api[_\- ]?key|secret(?:[_\- ]?key)?|access[_\- ]?key|client[_\- ]?secret|auth[_\- ]?token|bearer)\b\s*(?:is\s+|[:=]\s*|\s)["'`]?(?P<value>[A-Za-z0-9_\-./+=]*[0-9][A-Za-z0-9_\-./+=]*)"#,
            // A long hex or base64 run after `key` or `secret`.
            r#"(?i)\b(?:key|secret)\b[^\n]{0,24}?(?P<value>\b[A-Fa-f0-9]{32,}\b|[A-Za-z0-9+/]{32,}={0,2})"#,
        ];
        Shapes {
            whole: whole.iter().map(|s| Regex::new(s).expect("shape")).collect(),
            labelled: labelled
                .iter()
                .map(|s| Regex::new(s).expect("shape"))
                .collect(),
        }
    })
}

/// A labelled value is a secret when it is long enough and not a word or a
/// code expression (`os.environ`, `${TOKEN}`, `process.env.KEY`).
fn looks_secret(value: &str) -> bool {
    let chars = value.chars().count();
    if chars < 6 {
        return chars >= 4 && value.chars().any(|c| c.is_ascii_digit());
    }
    !(value.contains("environ") || value.starts_with("process.") || value.starts_with("env."))
}

/// `text` with every secret replaced by [`marker`].
pub fn redact(text: &str) -> String {
    let shapes = shapes();
    let mut out = text.to_string();
    for shape in &shapes.whole {
        out = shape
            .replace_all(&out, |found: &Captures<'_>| {
                marker(found[0].chars().count())
            })
            .into_owned();
    }
    for shape in &shapes.labelled {
        out = shape
            .replace_all(&out, |found: &Captures<'_>| {
                let all = found.get(0).expect("match");
                let value = found.name("value").expect("value");
                if value.as_str().starts_with('«') || !looks_secret(value.as_str()) {
                    return all.as_str().to_string();
                }
                let head = &all.as_str()[..value.start() - all.start()];
                let tail = &all.as_str()[value.end() - all.start()..];
                format!("{head}{}{tail}", marker(value.as_str().chars().count()))
            })
            .into_owned();
    }
    out
}

/// Whether `text` holds anything [`redact`] would replace.
pub fn has_secret(text: &str) -> bool {
    redact(text) != text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gone(text: &str, secret: &str) {
        let out = redact(text);
        assert!(!out.contains(secret), "{secret} stayed in {out}");
        assert!(out.contains("«redacted "), "{out}");
    }

    // FR-9: each secret shape is replaced.
    #[test]
    fn redact_replaces_each_secret_shape() {
        gone(
            "my key is sk-abcdef0123456789abcdef",
            "sk-abcdef0123456789abcdef",
        );
        gone(
            "anthropic sk-ant-api03-AAAAbbbbCCCCdddd1234",
            "AAAAbbbbCCCCdddd1234",
        );
        gone("stripe sk_live_51HxYzAbCdEfGh0123", "51HxYzAbCdEfGh0123");
        gone(
            "ghp_0123456789abcdefghij0123456789abcd",
            "ghp_0123456789abcdefghij",
        );
        gone(
            "github_pat_11ABCDEFG0123456789_abcdefghijk",
            "github_pat_11ABCDEFG",
        );
        gone("slack xoxb-1234567890-abcdefghij", "xoxb-1234567890");
        gone("aws AKIAIOSFODNN7EXAMPLE here", "AKIAIOSFODNN7EXAMPLE");
        gone(
            "AIzaSyA-1234567890abcdefghijklmnopqrstu",
            "AIzaSyA-1234567890",
        );
        gone(
            "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTYifQ.SflKxwRJSMeKKF2QT4fwpMeJf36",
            "eyJhbGciOiJIUzI1NiJ9",
        );
        gone(
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----\nafter",
            "MIIEpAIBAAKCAQEA",
        );
        gone("password: hunter22", "hunter22");
        gone("The wifi password is Tr0ub4dor&3", "Tr0ub4dor&3");
        gone("token=abc123def456ghi", "abc123def456ghi");
        gone("api_key: 9f8e7d6c5b4a3210", "9f8e7d6c5b4a3210");
        gone(
            "the signing secret 4f2a9c0d1e8b7a6f5c4d3e2f1a0b9c8d stays private",
            "4f2a9c0d1e8b7a6f5c4d3e2f1a0b9c8d",
        );
        // The label stays, so the line still reads.
        assert_eq!(redact("password: hunter22"), "password: «redacted 8 chars»");
        assert!(redact("after\n-----BEGIN PRIVATE KEY-----\nabc").starts_with("after\n«redacted"));
    }

    // FR-9: ordinary text and code are not touched.
    #[test]
    fn redact_leaves_ordinary_text_and_code() {
        for text in [
            "The person resets their password every quarter.",
            "Use token-based auth for the API.",
            "api_key = os.environ['OPENAI_API_KEY']",
            "const token = process.env.GITHUB_TOKEN;",
            "The key is in the drawer.",
            "Uses scikit-learn and sk-learn style pipelines.",
            "Freeze starts 2026-12-01; ticket ABC-1234.",
            "Password reset flow: send email, then link.",
            "secret: ${{ secrets.DEPLOY }}",
            "The person's commit hash habit: short hashes like 3f2a9c1.",
            "«redacted 12 chars» stays a marker",
        ] {
            assert_eq!(redact(text), text, "changed: {text}");
            assert!(!has_secret(text));
        }
        assert!(has_secret("password: hunter22"));
    }
}
