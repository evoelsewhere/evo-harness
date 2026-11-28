//! Link previews in Evo Mates' bubbles (docs/features/evo-mates-messaging/
//! spec.md, FR-17, NFR-3). Off by default (`mates.linkPreviews`); the window
//! asks only when it is on. A preview fetches one public `http(s)` page with
//! no cookies, no redirects, at most [`MAX_BYTES`] within [`TIMEOUT`], reads
//! its title, site and picture, and keeps the answer for a day.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::Url;

pub const MAX_BYTES: usize = 512 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(3);
const KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// What a card shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    pub url: String,
    pub title: String,
    pub site: String,
    pub description: Option<String>,
    pub image: Option<String>,
}

static CACHE: Mutex<Option<HashMap<String, (Instant, Option<LinkPreview>)>>> = Mutex::new(None);

/// Whether `ip` is on the public internet: never this computer or its network.
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.octets()[0] == 0
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64))
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || v6.to_ipv4_mapped().is_some_and(|v4| !public_ip(IpAddr::V4(v4))))
        }
    }
}

/// A URL a preview may fetch: `http(s)`, a host, no user name or password.
pub fn fetchable(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some() && url.username().is_empty() && url.password().is_none()
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))?;
    let rest = &tag[at + name.len() + 1..];
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = rest[1..].find(quote)?;
        Some(rest[1..1 + end].to_string())
    } else {
        Some(rest.split(|c: char| c.is_whitespace() || c == '>').next()?.to_string())
    }
}

fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The card from a page's HTML: `og:` / `twitter:` tags, else `<title>`.
pub fn parse_preview(url: &Url, html: &str) -> Option<LinkPreview> {
    let mut meta: HashMap<String, String> = HashMap::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(start) = lower[from..].find("<meta") {
        let start = from + start;
        let Some(end) = lower[start..].find('>') else { break };
        let tag = &html[start..start + end + 1];
        let key = attr(tag, "property").or_else(|| attr(tag, "name")).map(|k| k.to_ascii_lowercase());
        if let (Some(key), Some(content)) = (key, attr(tag, "content")) {
            meta.entry(key).or_insert_with(|| unescape(&content));
        }
        from = start + end + 1;
    }
    let title_tag = lower.find("<title").and_then(|start| {
        let open = start + lower[start..].find('>')? + 1;
        let close = open + lower[open..].find("</title>")?;
        Some(unescape(&html[open..close]))
    });
    let title = meta.get("og:title").or_else(|| meta.get("twitter:title")).cloned().or(title_tag)?;
    if title.is_empty() {
        return None;
    }
    let site = meta
        .get("og:site_name")
        .cloned()
        .unwrap_or_else(|| url.host_str().unwrap_or_default().trim_start_matches("www.").to_string());
    let image = meta
        .get("og:image")
        .or_else(|| meta.get("twitter:image"))
        .and_then(|src| url.join(src).ok())
        .filter(|u| u.scheme() == "https")
        .map(|u| u.to_string());
    let description = meta.get("og:description").or_else(|| meta.get("description")).cloned().filter(|d| !d.is_empty());
    let cut = |text: String, max: usize| if text.chars().count() > max { format!("{}…", text.chars().take(max - 1).collect::<String>()) } else { text };
    Some(LinkPreview {
        url: url.to_string(),
        title: cut(title, 140),
        site: cut(site, 60),
        description: description.map(|d| cut(d, 200)),
        image,
    })
}

async fn fetch(url: Url) -> Option<LinkPreview> {
    let host = url.host_str()?.to_string();
    let port = url.port_or_known_default()?;
    // Every address the name points to must be public.
    let addresses: Vec<_> = tokio::net::lookup_host((host.as_str(), port)).await.ok()?.collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
        return None;
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent("Mozilla/5.0 (compatible; EvoLinkPreview/1.0)")
        .build()
        .ok()?;
    let mut response = client.get(url.clone()).header("Accept", "text/html").send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let html_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|t| t.contains("html"));
    if !html_type {
        return None;
    }
    let mut body: Vec<u8> = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        let room = MAX_BYTES.saturating_sub(body.len());
        body.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if body.len() >= MAX_BYTES {
            break;
        }
    }
    parse_preview(&url, &String::from_utf8_lossy(&body))
}

/// The card for `url`, or `None` when it is not a public page or has no title.
#[tauri::command]
pub async fn link_preview(url: String) -> Option<LinkPreview> {
    let parsed = Url::parse(url.trim()).ok().filter(fetchable)?;
    let key = parsed.to_string();
    if let Some((at, kept)) = CACHE.lock().ok()?.get_or_insert_with(HashMap::new).get(&key) {
        if at.elapsed() < KEEP {
            return kept.clone();
        }
    }
    let found = tokio::time::timeout(TIMEOUT + Duration::from_secs(1), fetch(parsed)).await.ok().flatten();
    if let Ok(mut cache) = CACHE.lock() {
        let map = cache.get_or_insert_with(HashMap::new);
        if map.len() > 500 {
            map.clear();
        }
        map.insert(key, (Instant::now(), found.clone()));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_web_pages() {
        assert!(fetchable(&Url::parse("https://example.com/a").unwrap()));
        assert!(!fetchable(&Url::parse("file:///C:/x").unwrap()));
        assert!(!fetchable(&Url::parse("https://user:pw@example.com").unwrap()));
        assert!(!public_ip("127.0.0.1".parse().unwrap()));
        assert!(!public_ip("192.168.1.4".parse().unwrap()));
        assert!(!public_ip("10.0.0.1".parse().unwrap()));
        assert!(!public_ip("169.254.1.1".parse().unwrap()));
        assert!(!public_ip("::1".parse().unwrap()));
        assert!(!public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(public_ip("93.184.216.34".parse().unwrap()));
    }

    #[test]
    fn a_card_from_open_graph_or_the_title() {
        let url = Url::parse("https://www.example.com/post/1").unwrap();
        let html = r#"<html><head><title>Fallback</title>
            <meta property="og:title" content="Evo 4.15 &amp; more">
            <meta property="og:site_name" content="Example">
            <meta name="description" content="What changed">
            <meta property="og:image" content="/img/cover.png"></head></html>"#;
        let card = parse_preview(&url, html).unwrap();
        assert_eq!(card.title, "Evo 4.15 & more");
        assert_eq!(card.site, "Example");
        assert_eq!(card.description.as_deref(), Some("What changed"));
        assert_eq!(card.image.as_deref(), Some("https://www.example.com/img/cover.png"));

        let plain = parse_preview(&url, "<title> Just a title </title>").unwrap();
        assert_eq!((plain.title.as_str(), plain.site.as_str(), plain.image), ("Just a title", "example.com", None));
        assert!(parse_preview(&url, "<p>no title</p>").is_none());
    }
}
