//! Link previews in Evo Mates' bubbles (docs/features/evo-mates-messaging/
//! spec.md, FR-17, NFR-3). Off unless the person turns on
//! `mates.linkPreviews`; the command itself checks it. A preview fetches one
//! public `http(s)` page — the name resolved once and the connection pinned
//! to those public addresses (no DNS rebinding), no proxy, no cookies, no
//! redirects, at most [`MAX_BYTES`] within [`TIMEOUT`] — reads its title,
//! site and picture, fetches the picture the same way as a data URL of at
//! most [`MAX_IMAGE_BYTES`], and keeps the answer for a day.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Manager, Url};

use crate::db::Db;

pub const SETTING: &str = "mates.linkPreviews";
pub const MAX_BYTES: usize = 512 * 1024;
pub const MAX_IMAGE_BYTES: usize = 512 * 1024;
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
    /// The page's picture as a `data:` URL, fetched by the shell (the window's CSP blocks remote images).
    pub image: Option<String>,
}

static CACHE: Mutex<Option<HashMap<String, (Instant, Option<LinkPreview>)>>> = Mutex::new(None);

fn public_v4(v4: Ipv4Addr) -> bool {
    let [a, b, c, _] = v4.octets();
    !(v4.is_private()
        || v4.is_loopback()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_unspecified()
        || v4.is_multicast()
        || a == 0
        // Carrier-grade NAT 100.64.0.0/10.
        || (a == 100 && (b & 0xc0) == 64)
        // Benchmarking 198.18.0.0/15.
        || (a == 198 && (b & 0xfe) == 18)
        // Reserved 240.0.0.0/4.
        || a >= 240
        // IETF protocol assignments 192.0.0.0/24.
        || (a == 192 && b == 0 && c == 0))
}

fn public_v6(v6: Ipv6Addr) -> bool {
    let s = v6.segments();
    if let Some(v4) = v6.to_ipv4_mapped() {
        return public_v4(v4);
    }
    !(v6.is_loopback()
        || v6.is_unspecified()
        || v6.is_multicast()
        // Unique local fc00::/7, link-local fe80::/10, site-local fec0::/10.
        || (s[0] & 0xfe00) == 0xfc00
        || (s[0] & 0xffc0) == 0xfe80
        || (s[0] & 0xffc0) == 0xfec0
        // NAT64 64:ff9b::/96 and 6to4 2002::/16 reach IPv4 hosts by other names.
        || (s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0])
        || s[0] == 0x2002
        // Documentation 2001:db8::/32.
        || (s[0] == 0x2001 && s[1] == 0x0db8)
        // IPv4-compatible ::a.b.c.d and the rest of ::/96.
        || s[0..6] == [0, 0, 0, 0, 0, 0])
}

/// Whether `ip` is on the public internet: never this computer or its network.
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
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

/// The card from a page's HTML: `og:` / `twitter:` tags, else `<title>`;
/// `image` is the picture's absolute `https` URL, fetched later.
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

/// The public addresses `url`'s host names, all of them public, or `None`.
async fn public_addresses(url: &Url) -> Option<(String, Vec<SocketAddr>)> {
    let host = url.host_str()?.trim_matches(|c| c == '[' || c == ']').to_string();
    let port = url.port_or_known_default()?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port)).await.ok()?.collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
        return None;
    }
    Some((host, addresses))
}

/// Up to `limit` bytes of `url`, connecting only to the checked addresses.
async fn fetch_bytes(url: &Url, accept: &str, limit: usize) -> Option<(Option<String>, Vec<u8>)> {
    let (host, addresses) = public_addresses(url).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        // Pinned: reqwest must not resolve the name again (DNS rebinding).
        .resolve_to_addrs(&host, &addresses)
        .timeout(TIMEOUT)
        .user_agent("Mozilla/5.0 (compatible; EvoLinkPreview/1.0)")
        .build()
        .ok()?;
    let mut response = client.get(url.clone()).header("Accept", accept).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|t| t.to_ascii_lowercase());
    let mut body: Vec<u8> = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if body.len() + chunk.len() > limit {
            let room = limit - body.len();
            body.extend_from_slice(&chunk[..room]);
            return Some((content_type, body));
        }
        body.extend_from_slice(&chunk);
    }
    Some((content_type, body))
}

/// The picture at `url` as a `data:` URL, when it is a whole image within the cap.
async fn fetch_image(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok().filter(fetchable)?;
    let (kind, bytes) = fetch_bytes(&parsed, "image/*", MAX_IMAGE_BYTES + 1).await?;
    let kind = kind?;
    if bytes.len() > MAX_IMAGE_BYTES || !matches!(kind.split(';').next()?.trim(), "image/png" | "image/jpeg" | "image/gif" | "image/webp") {
        return None;
    }
    Some(format!("data:{};base64,{}", kind.split(';').next()?.trim(), base64::engine::general_purpose::STANDARD.encode(bytes)))
}

async fn fetch(url: Url) -> Option<LinkPreview> {
    let (kind, body) = fetch_bytes(&url, "text/html", MAX_BYTES).await?;
    if kind.as_deref().is_some_and(|t| !t.contains("html")) {
        return None;
    }
    let mut card = parse_preview(&url, &String::from_utf8_lossy(&body))?;
    card.image = match card.image.take() {
        Some(src) => fetch_image(&src).await,
        None => None,
    };
    Some(card)
}

/// Whether the person turned link previews on.
fn previews_on(app: &AppHandle) -> bool {
    app.try_state::<Db>()
        .and_then(|db| db.with(|conn| evo_store::settings::get(conn, SETTING)).ok().flatten())
        .is_some_and(|v| v.as_bool() == Some(true))
}

/// The card for `url`, or `None` when previews are off, or it is not a public page or has no title.
#[tauri::command]
pub async fn link_preview(app: AppHandle, url: String) -> Option<LinkPreview> {
    if !previews_on(&app) {
        return None;
    }
    let parsed = Url::parse(url.trim()).ok().filter(fetchable)?;
    let key = parsed.to_string();
    if let Some((at, kept)) = CACHE.lock().ok()?.get_or_insert_with(HashMap::new).get(&key) {
        if at.elapsed() < KEEP {
            return kept.clone();
        }
    }
    let found = tokio::time::timeout(TIMEOUT * 2 + Duration::from_secs(1), fetch(parsed)).await.ok().flatten();
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
        for private in [
            "127.0.0.1",
            "192.168.1.4",
            "10.0.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "198.19.255.1",
            "240.0.0.1",
            "255.255.255.255",
            "192.0.0.8",
            "0.0.0.0",
            "::1",
            "::",
            "::ffff:127.0.0.1",
            "::7f00:1",
            "64:ff9b::7f00:1",
            "2002:7f00:1::1",
            "fc00::1",
            "fe80::1",
            "fec0::1",
            "2001:db8::1",
        ] {
            assert!(!public_ip(private.parse().unwrap()), "{private} must not be fetched");
        }
        for public in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111", "::ffff:93.184.216.34"] {
            assert!(public_ip(public.parse().unwrap()), "{public} is public");
        }
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
