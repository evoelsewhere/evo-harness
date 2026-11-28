//! Services' logos for the Connected apps page (docs/features/connected-apps/
//! spec.md, FR-3, NFR-3): fetched by the shell from Composio's logo host only
//! (`https://logos.composio.dev/api/<slug>`), kept in the app's cache folder
//! and handed to the page as `data:` URLs, so the page loads no remote image
//! and logos show offline. A logo that cannot be had is remembered for a day.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use base64::Engine as _;

/// Where logos come from.
pub const HOST: &str = "https://logos.composio.dev/api/";
/// A logo larger than this is not used.
const MAX_BYTES: usize = 256 * 1024;
/// How long a missing logo is not asked for again.
const MISSING_FOR: Duration = Duration::from_secs(24 * 60 * 60);
/// Logos fetched at once.
const AT_ONCE: usize = 8;

/// A toolkit slug fit for a URL path and a file name.
pub fn clean(slug: &str) -> Option<String> {
    let slug = slug.trim().to_ascii_lowercase();
    (!slug.is_empty()
        && slug.len() <= 80
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    .then_some(slug)
}

/// The image types a logo may be, by content type: its file extension.
fn extension(mime: &str) -> Option<&'static str> {
    match mime.split(';').next().unwrap_or("").trim() {
        "image/svg+xml" => Some("svg"),
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

fn mime_of(ext: &str) -> &'static str {
    match ext {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/gif",
    }
}

fn data_url(ext: &str, bytes: &[u8]) -> String {
    format!(
        "data:{};base64,{}",
        mime_of(ext),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// The cached logo of `slug`: `Some(Some(url))`, `Some(None)` when it is
/// known to be missing, `None` when it was never fetched (or that is old).
pub fn cached(dir: &Path, slug: &str) -> Option<Option<String>> {
    for ext in ["svg", "png", "jpg", "webp", "gif"] {
        if let Ok(bytes) = std::fs::read(dir.join(format!("{slug}.{ext}"))) {
            return Some(Some(data_url(ext, &bytes)));
        }
    }
    let missing = dir.join(format!("{slug}.none"));
    let fresh = std::fs::metadata(&missing)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|at| SystemTime::now().duration_since(at).ok())
        .is_some_and(|age| age < MISSING_FOR);
    fresh.then_some(None)
}

/// Fetch one logo and cache it (or its absence).
async fn fetch(http: &reqwest::Client, dir: &Path, slug: &str) -> Option<String> {
    let got = async {
        let response = http.get(format!("{HOST}{slug}")).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let ext = extension(response.headers().get("content-type")?.to_str().ok()?)?;
        let bytes = response.bytes().await.ok()?;
        (!bytes.is_empty() && bytes.len() <= MAX_BYTES).then(|| (ext, bytes.to_vec()))
    }
    .await;
    let _ = std::fs::create_dir_all(dir);
    match got {
        Some((ext, bytes)) => {
            let _ = std::fs::write(dir.join(format!("{slug}.{ext}")), &bytes);
            let _ = std::fs::remove_file(dir.join(format!("{slug}.none")));
            Some(data_url(ext, &bytes))
        }
        None => {
            let _ = std::fs::write(dir.join(format!("{slug}.none")), b"");
            None
        }
    }
}

/// `<cache dir>/connected-apps/logos`.
pub fn dir(cache_dir: &Path) -> PathBuf {
    cache_dir.join("connected-apps").join("logos")
}

/// The logos of `slugs` as `data:` URLs (missing ones left out), from the
/// cache, fetching what is not there.
pub async fn logos(dir: &Path, slugs: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut wanted = Vec::new();
    for slug in slugs.iter().filter_map(|s| clean(s)) {
        match cached(dir, &slug) {
            Some(Some(url)) => {
                out.insert(slug, url);
            }
            Some(None) => {}
            None if !wanted.contains(&slug) => wanted.push(slug),
            None => {}
        }
    }
    if wanted.is_empty() {
        return out;
    }
    let Ok(http) = reqwest::Client::builder()
        .user_agent("evo")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return out;
    };
    for batch in wanted.chunks(AT_ONCE) {
        let found = futures_join(batch.iter().map(|slug| {
            let http = http.clone();
            let slug = slug.clone();
            let dir = dir.to_path_buf();
            async move { (slug.clone(), fetch(&http, &dir, &slug).await) }
        }))
        .await;
        for (slug, url) in found {
            if let Some(url) = url {
                out.insert(slug, url);
            }
        }
    }
    out
}

/// Run futures together and collect their outputs in order.
async fn futures_join<F, T>(futures: impl Iterator<Item = F>) -> Vec<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let handles: Vec<_> = futures.map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(value) = handle.await {
            out.push(value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logos_come_from_the_cache_as_data_urls() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(clean("Gmail"), Some("gmail".into()));
        assert_eq!(clean("../etc"), None);
        assert_eq!(clean("a/b"), None);
        assert_eq!(cached(dir.path(), "gmail"), None, "never fetched");
        std::fs::write(dir.path().join("gmail.svg"), "<svg/>").unwrap();
        assert_eq!(
            cached(dir.path(), "gmail"),
            Some(Some("data:image/svg+xml;base64,PHN2Zy8+".into()))
        );
        std::fs::write(dir.path().join("acme.none"), "").unwrap();
        assert_eq!(
            cached(dir.path(), "acme"),
            Some(None),
            "known missing, not asked again today"
        );
        assert_eq!(extension("image/svg+xml; charset=utf-8"), Some("svg"));
        assert_eq!(extension("text/html"), None, "only images");
    }
}
