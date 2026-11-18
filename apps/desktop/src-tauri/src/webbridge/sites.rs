//! Site permissions for Evo Webbridge (docs/features/browser-bridge/spec.md
//! FR-12, FR-13, FR-22; plan 3.4 `sites.json`).
//!
//! One list for every browser and profile: a site (scheme, host and port) is
//! Allow or Block; every other site is Ask. Kept in
//! `<agent dir>/webbridge/sites.json`, written only by the shell, read once
//! and cached.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Ask,
    Allow,
    Block,
}

impl Permission {
    pub fn parse(text: &str) -> Option<Permission> {
        match text {
            "ask" => Some(Permission::Ask),
            "allow" => Some(Permission::Allow),
            "block" => Some(Permission::Block),
            _ => None,
        }
    }
}

/// The site a permission is kept for: the origin of `url`, or `url` itself
/// when it has none (as the approval gate names sites).
pub fn site_of(url: &str) -> String {
    match crate::agent_tools::gate::origin_of(url) {
        origin if origin.is_empty() || origin.ends_with("://") => url.trim().to_string(),
        origin => origin,
    }
}

static CACHE: Mutex<Option<BTreeMap<String, Permission>>> = Mutex::new(None);

fn file() -> PathBuf {
    super::dir().join("sites.json")
}

fn load_from(path: &Path) -> BTreeMap<String, Permission> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            serde_json::from_value::<BTreeMap<String, Permission>>(v["sites"].clone()).ok()
        })
        .map(|mut sites| {
            sites.retain(|_, p| *p != Permission::Ask);
            sites
        })
        .unwrap_or_default()
}

fn save_to(path: &Path, sites: &BTreeMap<String, Permission>) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("sites.json has no folder")?)
        .map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&json!({ "version": 1, "sites": sites }))
        .map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn with<T>(f: impl FnOnce(&mut BTreeMap<String, Permission>) -> T) -> T {
    let mut cache = CACHE.lock().expect("sites lock");
    let sites = cache.get_or_insert_with(|| load_from(&file()));
    f(sites)
}

/// The permission for the site of `url`.
pub fn permission(url: &str) -> Permission {
    let site = site_of(url);
    with(|sites| sites.get(&site).copied().unwrap_or(Permission::Ask))
}

/// Every site with a permission other than Ask.
pub fn all() -> BTreeMap<String, Permission> {
    with(|sites| sites.clone())
}

/// Set a site's permission (Ask removes it from the list).
pub fn set(site: &str, permission: Permission) -> Result<(), String> {
    let site = site_of(site);
    if site.is_empty() {
        return Err("no site".into());
    }
    with(|sites| {
        if permission == Permission::Ask {
            sites.remove(&site);
        } else {
            sites.insert(site, permission);
        }
        save_to(&file(), sites)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_are_origins() {
        assert_eq!(
            site_of("https://billing.example.com/invoices?x=1"),
            "https://billing.example.com"
        );
        assert_eq!(site_of("http://localhost:3000/a"), "http://localhost:3000");
    }

    #[test]
    fn the_list_round_trips_and_keeps_only_allow_and_block() {
        let dir = std::env::temp_dir().join(format!("evo-sites-test-{}", std::process::id()));
        let path = dir.join("sites.json");
        let mut sites = BTreeMap::new();
        sites.insert("https://a.example".to_string(), Permission::Allow);
        sites.insert("https://b.example".to_string(), Permission::Block);
        save_to(&path, &sites).unwrap();
        assert_eq!(load_from(&path), sites);
        std::fs::write(
            &path,
            r#"{"version":1,"sites":{"https://c.example":"ask","https://d.example":"block"}}"#,
        )
        .unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("https://d.example"), Some(&Permission::Block));
        assert!(load_from(&dir.join("missing.json")).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn permissions_parse_by_name() {
        assert_eq!(Permission::parse("block"), Some(Permission::Block));
        assert_eq!(Permission::parse("deny"), None);
    }
}
