//! The catalogue of services (docs/features/connected-apps/spec.md, FR-3,
//! NFR-3; spec A-2): Composio's toolkits, kept a day in the app's cache
//! folder so the page opens at once and offline, searched by name and
//! category here.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::composio::{ApiError, Composio};

/// How long the cached catalogue is used before it is fetched again.
pub const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// One service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolkit {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub logo: String,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Composio provides the sign-in (else the person's own auth config
    /// is needed: FR-16).
    #[serde(default)]
    pub managed_auth: bool,
    /// Needs no sign-in at all.
    #[serde(default)]
    pub no_auth: bool,
    #[serde(default)]
    pub description: String,
    /// How many tools Composio has for it.
    #[serde(default)]
    pub tools_count: u64,
    /// The service's own MCP server (an official id), when it has one.
    #[serde(default)]
    pub official: Option<String>,
    /// Composio offers it (false for a service only its official server offers).
    #[serde(default = "yes")]
    pub composio: bool,
}

fn yes() -> bool {
    true
}

impl Toolkit {
    /// One item of `GET /toolkits`.
    pub fn from_composio(item: &Value) -> Option<Self> {
        let text = |p: &str| item.pointer(p).and_then(Value::as_str).map(str::to_string);
        let slug = text("/slug")?.to_ascii_lowercase();
        let categories = item
            .pointer("/meta/categories")
            .or_else(|| item.get("categories"))
            .and_then(Value::as_array)
            .map(|c| {
                c.iter()
                    .filter_map(|c| c.get("name").or(Some(c)).and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let managed = item
            .get("composio_managed_auth_schemes")
            .and_then(Value::as_array)
            .is_some_and(|s| !s.is_empty());
        Some(Self {
            name: text("/name").unwrap_or_else(|| slug.clone()),
            logo: text("/meta/logo")
                .or_else(|| text("/logo"))
                .unwrap_or_default(),
            description: text("/meta/description")
                .or_else(|| text("/description"))
                .unwrap_or_default(),
            categories,
            managed_auth: managed,
            no_auth: item
                .get("no_auth")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            tools_count: item
                .pointer("/meta/tools_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            official: None,
            composio: true,
            slug,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Cached {
    /// Seconds since 1970.
    at: u64,
    toolkits: Vec<Toolkit>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `<cache dir>/connected-apps/catalog.json`.
pub fn cache_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("connected-apps").join("catalog.json")
}

/// The cached catalogue and whether it is younger than [`MAX_AGE`].
pub fn read_cache(file: &Path) -> Option<(Vec<Toolkit>, bool)> {
    let cached: Cached = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let fresh = now().saturating_sub(cached.at) < MAX_AGE.as_secs();
    Some((cached.toolkits, fresh))
}

pub fn write_cache(file: &Path, toolkits: &[Toolkit]) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(file, json!({"at": now(), "toolkits": toolkits}).to_string());
}

/// Every toolkit Composio lists (all pages).
pub async fn fetch(client: &Composio) -> Result<Vec<Toolkit>, ApiError> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..50 {
        let page = client.toolkits(cursor.as_deref()).await?;
        out.extend(
            page.get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Toolkit::from_composio),
        );
        cursor = page
            .get("next_cursor")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    // Composio lists the most used first; its own toolkit is not a service.
    let mut seen = std::collections::HashSet::new();
    out.retain(|t| t.slug != "composio" && seen.insert(t.slug.clone()));
    Ok(out)
}

/// The catalogue: the cache while it is fresh; else fetched (and cached),
/// or the old cache when Composio cannot be reached.
pub async fn load(client: &Composio, file: &Path) -> Result<Vec<Toolkit>, String> {
    let cached = read_cache(file);
    if let Some((toolkits, true)) = &cached {
        return Ok(toolkits.clone());
    }
    match fetch(client).await {
        Ok(toolkits) if !toolkits.is_empty() => {
            write_cache(file, &toolkits);
            Ok(toolkits)
        }
        Ok(_) | Err(_) if cached.is_some() => Ok(cached.map(|c| c.0).unwrap_or_default()),
        Ok(toolkits) => Ok(toolkits),
        Err(error) => Err(error.to_string()),
    }
}

/// Browse's list (FR-20): every official server first, in the list's
/// order — on the card of its Composio toolkit when the service has exactly
/// one, so the service shows once with both ways — then Composio's toolkits.
pub fn merge(toolkits: Vec<Toolkit>) -> Vec<Toolkit> {
    let mut rest = toolkits;
    let mut first = Vec::new();
    for o in super::official::all() {
        let matching: Vec<usize> = rest
            .iter()
            .enumerate()
            .filter(|(_, t)| o.composio.iter().any(|c| c.eq_ignore_ascii_case(&t.slug)))
            .map(|(i, _)| i)
            .collect();
        let card = match matching.as_slice() {
            [one] => {
                let mut t = rest.remove(*one);
                t.official = Some(o.id.clone());
                t
            }
            _ => Toolkit {
                slug: o.id.clone(),
                name: o.name.clone(),
                logo: String::new(),
                categories: vec![o.category.clone()],
                managed_auth: false,
                no_auth: false,
                description: String::new(),
                tools_count: 0,
                official: Some(o.id.clone()),
                composio: false,
            },
        };
        first.push(card);
    }
    first.extend(rest);
    first
}

/// The toolkits matching `query` (name or slug) and `category`.
pub fn search(toolkits: &[Toolkit], query: Option<&str>, category: Option<&str>) -> Vec<Toolkit> {
    let query = query
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let category = category
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty());
    toolkits
        .iter()
        .filter(|t| {
            query
                .as_ref()
                .is_none_or(|q| t.name.to_lowercase().contains(q) || t.slug.contains(q.as_str()))
        })
        .filter(|t| {
            category
                .as_ref()
                .is_none_or(|c| t.categories.iter().any(|k| k.to_lowercase() == *c))
        })
        .cloned()
        .collect()
}

/// Every category, sorted, once.
pub fn categories(toolkits: &[Toolkit]) -> Vec<String> {
    let mut all: Vec<String> = toolkits
        .iter()
        .flat_map(|t| t.categories.iter().cloned())
        .collect();
    all.sort_by_key(|c| c.to_lowercase());
    all.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit(slug: &str, name: &str, categories: &[&str]) -> Toolkit {
        Toolkit {
            slug: slug.into(),
            name: name.into(),
            logo: String::new(),
            categories: categories.iter().map(|c| c.to_string()).collect(),
            managed_auth: true,
            no_auth: false,
            description: String::new(),
            tools_count: 0,
            official: None,
            composio: true,
        }
    }

    #[test]
    fn notion_shows_once_official_first() {
        let all = vec![
            kit("gmail", "Gmail", &["email"]),
            kit("notion", "Notion", &["notes"]),
            kit("jira", "Jira", &[]),
            kit("confluence", "Confluence", &[]),
        ];
        let merged = merge(all);
        let slugs: Vec<&str> = merged.iter().map(|t| t.slug.as_str()).collect();
        assert_eq!(
            slugs.iter().filter(|s| **s == "notion").count(),
            1,
            "{slugs:?}"
        );
        let notion = merged.iter().find(|t| t.slug == "notion").unwrap();
        assert_eq!(
            (notion.official.as_deref(), notion.composio),
            (Some("notion"), true),
            "both ways on one card"
        );
        let official_count = merged.iter().take_while(|t| t.official.is_some()).count();
        assert_eq!(
            official_count,
            crate::connected_apps::official::all().len(),
            "official ones first: {slugs:?}"
        );
        // GitHub has no Composio card here: an official-only card.
        let github = merged.iter().find(|t| t.slug == "github").unwrap();
        assert!(!github.composio);
        // Atlassian covers two toolkits: its own card; Jira stays Composio's.
        assert!(merged.iter().any(|t| t.slug == "atlassian" && !t.composio));
        assert!(merged
            .iter()
            .any(|t| t.slug == "jira" && t.official.is_none()));
        assert_eq!(merged.last().unwrap().slug, "confluence");
    }

    #[test]
    fn the_catalogue_is_searched_by_name_and_category() {
        let all = [
            kit("gmail", "Gmail", &["Email"]),
            kit("slack", "Slack", &["Communication"]),
            kit("outlook", "Outlook", &["Email"]),
        ];
        let names = |found: Vec<Toolkit>| found.into_iter().map(|t| t.slug).collect::<Vec<_>>();
        assert_eq!(names(search(&all, Some("GMA"), None)), ["gmail"]);
        assert_eq!(
            names(search(&all, None, Some("email"))),
            ["gmail", "outlook"]
        );
        assert_eq!(names(search(&all, Some("out"), Some("Email"))), ["outlook"]);
        assert_eq!(search(&all, Some(" "), None).len(), 3);
        assert_eq!(categories(&all), ["Communication", "Email"]);
    }

    #[test]
    fn toolkits_read_from_composio_items() {
        let item = json!({"slug": "GMAIL", "name": "Gmail", "meta": {"logo": "https://logos/gmail.svg", "categories": [{"id": "email", "name": "Email"}], "description": "Mail"}, "composio_managed_auth_schemes": ["OAUTH2"], "no_auth": false});
        let t = Toolkit::from_composio(&item).unwrap();
        assert_eq!(
            (t.slug.as_str(), t.name.as_str(), t.logo.as_str()),
            ("gmail", "Gmail", "https://logos/gmail.svg")
        );
        assert_eq!(t.categories, ["Email"]);
        assert!(t.managed_auth);
        let own = Toolkit::from_composio(
            &json!({"slug": "acme", "name": "Acme", "composio_managed_auth_schemes": []}),
        )
        .unwrap();
        assert!(!own.managed_auth);
    }

    #[test]
    fn the_cache_is_fresh_for_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let file = cache_file(dir.path());
        assert!(read_cache(&file).is_none());
        write_cache(&file, &[kit("gmail", "Gmail", &[])]);
        let (list, fresh) = read_cache(&file).unwrap();
        assert!(fresh);
        assert_eq!(list[0].slug, "gmail");
        std::fs::write(
            &file,
            json!({"at": now() - MAX_AGE.as_secs() - 1, "toolkits": list}).to_string(),
        )
        .unwrap();
        assert!(!read_cache(&file).unwrap().1);
    }
}
