//! What is installed, in `<agent dir>/plugins`:
//!
//! ```text
//! marketplaces.json          the marketplaces added (source, where, when)
//! marketplaces/<name>/       their clones (folders are read in place)
//! cache/<marketplace>/<plugin>/<version>/   installed copies
//! data/<plugin id>/          each plugin's data, kept across updates
//! installed.json             {id: {name, marketplace, version, path, enabled, config, …}}
//! ```
//!
//! `installed.json` never holds a secret: secret settings, tokens and
//! cookies live in the plugin's vault (`evo-vault`), and only the names of
//! the secrets that are set are kept here (docs/features/plugin-settings/spec.md,
//! FR-5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::manifest::{self, Plugin};
use crate::marketplace::{self, Marketplace, MarketplaceSource, PluginSource};
use crate::{copy_dir, valid_name};

/// The marketplace name for plugins added from a folder.
pub const LOCAL: &str = "local";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    /// `name@marketplace`.
    pub id: String,
    pub name: String,
    pub marketplace: String,
    pub version: String,
    /// The installed copy (or, for a folder, the folder itself).
    pub path: PathBuf,
    pub enabled: bool,
    #[serde(default)]
    pub installed_at: u64,
    /// Values for the plugin's `userConfig`.
    #[serde(default)]
    pub config: Map<String, Value>,
    /// The permissions the person approved for its views
    /// (docs/features/plugin-ui/spec.md, FR-3, FR-16).
    #[serde(default)]
    pub approved_permissions: Vec<String>,
    /// Reload its views when its files change (FR-14).
    #[serde(default)]
    pub developer_mode: bool,
    /// An update that asks for more permissions, kept until the person
    /// approves them; the installed version runs meanwhile (FR-3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
    /// Workspace values of per-workspace settings, by workspace id
    /// (plugin-settings FR-3). No secrets.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspace_config: BTreeMap<String, Map<String, Value>>,
    /// The secret settings that have a value in the vault: `key`, or
    /// `key@<workspace id>` for a workspace's own (FR-4, FR-5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets_set: Vec<String>,
    /// When its backend runs, as the person chose; the manifest's when unset
    /// (FR-11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_mode: Option<crate::runtime::StartMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_stop_minutes: Option<u32>,
    /// Answers to optional permissions it asked for later: allowed or not (FR-9).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub optional_answers: BTreeMap<String, bool>,
    /// Signed-in accounts, without their tokens or cookies (FR-20, FR-29).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<AccountRecord>,
    /// The person's rules for its AI (docs/features/plugin-ai/spec.md, FR-14).
    #[serde(default, skip_serializing_if = "AiSettings::is_empty")]
    pub ai: AiSettings,
}

/// A model by provider and id, as the person picked it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModel {
    pub provider: String,
    pub model: String,
}

/// What a plugin's AI may use and spend, as the person set it
/// (docs/features/plugin-ai/spec.md, FR-14). Unset: every connected
/// provider, evo's choice of model, no cap.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<AiModel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<AiModel>,
    /// Pinned models per tier: `{fast?, balanced?, best?}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tiers: BTreeMap<String, AiModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_day_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_month_usd: Option<f64>,
}

impl AiSettings {
    pub fn is_empty(&self) -> bool {
        *self == AiSettings::default()
    }
}

/// One signed-in account of a plugin. Its secrets are in the vault under
/// `account:<record id>`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRecord {
    /// The manifest's account id (`github`).
    pub account: String,
    /// This sign-in's own id.
    pub id: String,
    /// The name the service gave (`ana@example.com`).
    pub label: String,
    /// `signedIn`, `needsSignIn` or `offline`.
    pub state: String,
    /// The account used where no workspace chose another.
    #[serde(default)]
    pub default: bool,
    /// Workspaces that use this account instead of the default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspaces: Vec<String>,
    /// The access granted (OAuth scopes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
}

/// A downloaded update waiting for consent to new permissions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub version: String,
    pub path: PathBuf,
    pub permissions: Vec<String>,
}

/// The plugin in `folder`, read for its permissions only.
fn peek(folder: &Path) -> Plugin {
    manifest::load(folder, "", "", folder, &Map::new())
}

/// The permissions the plugin in `folder` cannot run without: declared and
/// not optional, and implied by its parts (plugin-settings FR-8).
pub fn requested_permissions(folder: &Path) -> Vec<String> {
    peek(folder).required_permissions()
}

/// Every permission the plugin in `folder` may be given, optional ones too.
pub fn all_permissions(folder: &Path) -> Vec<String> {
    peek(folder).permissions
}

/// What a reinstall keeps of the person's choices.
fn carry(previous: Option<&Installed>, fresh: Installed) -> Installed {
    let Some(previous) = previous else {
        return fresh;
    };
    Installed {
        config: previous.config.clone(),
        developer_mode: previous.developer_mode,
        workspace_config: previous.workspace_config.clone(),
        secrets_set: previous.secrets_set.clone(),
        start_mode: previous.start_mode,
        idle_stop_minutes: previous.idle_stop_minutes,
        optional_answers: previous.optional_answers.clone(),
        accounts: previous.accounts.clone(),
        ai: previous.ai.clone(),
        ..fresh
    }
}

/// Hold back the parts of `plugin` that may not run yet (FR-4, FR-8,
/// FR-17): hooks without `hooks`; its backend and connector servers without
/// `runtime`, or without `secrets` when they use secrets; everything that
/// runs while a required setting is missing.
pub fn gate(plugin: &mut Plugin, installed: &Installed) {
    let approved = |p: &str| installed.approved_permissions.iter().any(|a| a == p);
    let personal_secrets: Vec<String> = installed
        .secrets_set
        .iter()
        .filter(|s| !s.contains('@'))
        .cloned()
        .collect();
    let missing =
        crate::settings::missing_required(&plugin.settings, &installed.config, &personal_secrets);
    let has_code = !plugin.mcp_servers.is_empty() || plugin.runtime.is_some();
    let mut wait = |part: &str, needs: Vec<String>| {
        plugin.waiting.push(manifest::Waiting {
            part: part.into(),
            needs,
        })
    };
    if !missing.is_empty() && (has_code || !plugin.hooks.is_empty()) {
        wait("setup", missing);
    }
    if !plugin.hooks.is_empty() && !approved("hooks") {
        wait("hooks", vec!["hooks".into()]);
    }
    if has_code && !approved("runtime") {
        wait("runtime", vec!["runtime".into()]);
    }
    let uses_secrets = serde_json::to_string(&plugin.mcp_servers)
        .is_ok_and(|s| s.contains("${secret."))
        || plugin
            .runtime
            .as_ref()
            .is_some_and(|r| !crate::runtime::secret_keys(&r.env).is_empty())
        || !plugin.accounts.is_empty();
    if has_code && uses_secrets && !approved("secrets") {
        wait("runtime", vec!["secrets".into()]);
    }
    let held = |part: &str| {
        plugin
            .waiting
            .iter()
            .any(|w| w.part == part || w.part == "setup")
    };
    if held("hooks") {
        plugin.hooks.clear();
    }
    if held("runtime") {
        plugin.mcp_servers.clear();
        plugin.runtime = None;
    }
}

impl Installed {
    /// Permissions asked for and not approved: those of the installed
    /// version, then those of a pending update.
    pub fn unapproved(&self) -> Vec<String> {
        let mut asked = requested_permissions(&self.path);
        if let Some(pending) = &self.pending {
            asked.extend(pending.permissions.iter().cloned());
        }
        let mut out: Vec<String> = Vec::new();
        for permission in asked {
            if !self.approved_permissions.contains(&permission) && !out.contains(&permission) {
                out.push(permission);
            }
        }
        out
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text + "\n").map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn remove_dir(path: &Path) {
    if path.exists() {
        // Git marks pack files read-only; clear that so Windows can delete them.
        fn writable(dir: &Path) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        writable(&p);
                    } else if let Ok(meta) = std::fs::metadata(&p) {
                        let mut perms = meta.permissions();
                        #[allow(clippy::permissions_set_readonly_false)]
                        perms.set_readonly(false);
                        let _ = std::fs::set_permissions(&p, perms);
                    }
                }
            }
        }
        writable(path);
        let _ = std::fs::remove_dir_all(path);
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `<agent dir>/plugins`.
    pub fn in_agent_dir(agent_dir: &Path) -> Self {
        Self::new(agent_dir.join("plugins"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn marketplaces_file(&self) -> PathBuf {
        self.root.join("marketplaces.json")
    }

    fn installed_file(&self) -> PathBuf {
        self.root.join("installed.json")
    }

    pub fn data_dir(&self, id: &str) -> PathBuf {
        let safe: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        self.root.join("data").join(safe)
    }

    // ── Marketplaces ────────────────────────────────────────────────────────

    pub fn marketplaces(&self) -> Vec<Marketplace> {
        let map: BTreeMap<String, Marketplace> = read(&self.marketplaces_file());
        map.into_values().collect()
    }

    pub fn marketplace(&self, name: &str) -> Result<Marketplace, String> {
        self.marketplaces()
            .into_iter()
            .find(|m| m.name == name)
            .ok_or_else(|| format!("no marketplace `{name}`"))
    }

    fn save_marketplace(&self, marketplace: Marketplace) -> Result<(), String> {
        let mut map: BTreeMap<String, Marketplace> = read(&self.marketplaces_file());
        map.insert(marketplace.name.clone(), marketplace);
        write(&self.marketplaces_file(), &map)
    }

    /// Add a marketplace from `owner/repo[#ref]`, a git URL `[#ref]`, or a folder.
    pub fn add_marketplace(&self, input: &str) -> Result<Marketplace, String> {
        let source = MarketplaceSource::parse(input)?;
        let (location, temporary) = match &source {
            MarketplaceSource::Directory { path } => (path.clone(), false),
            MarketplaceSource::Git { url, reference } => {
                let tmp = self
                    .root
                    .join("marketplaces")
                    .join(format!(".adding-{}", now_ms()));
                marketplace::clone(url, reference.as_deref(), &tmp)?;
                (tmp, true)
            }
        };
        let fail = |error: String| {
            if temporary {
                remove_dir(&location);
            }
            Err(error)
        };
        let catalog = match marketplace::catalog_path(&location).map(|p| manifest::read_json(&p)) {
            Some(Ok(catalog)) => catalog,
            Some(Err(e)) => return fail(e),
            None => {
                return fail(
                    "no .evo-plugin/marketplace.json, .claude-plugin/marketplace.json or marketplace.json there"
                        .into(),
                )
            }
        };
        let name = catalog
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if !valid_name(&name) || name == LOCAL {
            return fail(format!("the marketplace's name `{name}` cannot be used"));
        }
        if self.marketplaces().iter().any(|m| m.name == name) {
            return fail(format!("a marketplace named `{name}` is already added"));
        }
        let location = if temporary {
            let dest = self.root.join("marketplaces").join(&name);
            remove_dir(&dest);
            std::fs::rename(&location, &dest).map_err(|e| e.to_string())?;
            dest
        } else {
            location
        };
        let marketplace = Marketplace {
            name,
            source,
            location,
            description: catalog
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            owner: catalog
                .pointer("/owner/name")
                .and_then(Value::as_str)
                .map(str::to_string),
            updated_at: now_ms(),
        };
        self.save_marketplace(marketplace.clone())?;
        Ok(marketplace)
    }

    /// Fetch the marketplace again (a folder is re-read as it is).
    pub fn update_marketplace(&self, name: &str) -> Result<Marketplace, String> {
        let mut marketplace = self.marketplace(name)?;
        if let MarketplaceSource::Git { url, reference } = &marketplace.source {
            let tmp = self
                .root
                .join("marketplaces")
                .join(format!(".updating-{}", now_ms()));
            marketplace::clone(url, reference.as_deref(), &tmp)?;
            remove_dir(&marketplace.location);
            std::fs::rename(&tmp, &marketplace.location).map_err(|e| e.to_string())?;
        }
        if let Ok(catalog) = marketplace.catalog() {
            marketplace.description = catalog
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);
            marketplace.owner = catalog
                .pointer("/owner/name")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        marketplace.updated_at = now_ms();
        self.save_marketplace(marketplace.clone())?;
        Ok(marketplace)
    }

    /// Forget a marketplace (its installed plugins stay).
    pub fn remove_marketplace(&self, name: &str) -> Result<(), String> {
        let mut map: BTreeMap<String, Marketplace> = read(&self.marketplaces_file());
        let marketplace = map
            .remove(name)
            .ok_or_else(|| format!("no marketplace `{name}`"))?;
        if matches!(marketplace.source, MarketplaceSource::Git { .. }) {
            remove_dir(&marketplace.location);
        }
        write(&self.marketplaces_file(), &map)
    }

    /// Every plugin every marketplace offers, with whether it is installed.
    pub fn catalog(&self) -> Vec<Value> {
        let installed = self.installed();
        let mut out = Vec::new();
        for marketplace in self.marketplaces() {
            match marketplace.entries() {
                Ok(entries) => {
                    for mut entry in entries {
                        let name = entry
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let id = format!("{name}@{}", marketplace.name);
                        let mine = installed.iter().find(|i| i.id == id);
                        if let Some(object) = entry.as_object_mut() {
                            object.insert("marketplace".into(), json!(marketplace.name));
                            object.insert("id".into(), json!(id));
                            object.insert("installed".into(), json!(mine.map(|i| &i.version)));
                        }
                        out.push(entry);
                    }
                }
                Err(error) => out.push(json!({ "marketplace": marketplace.name, "error": error })),
            }
        }
        out
    }

    // ── Plugins ─────────────────────────────────────────────────────────────

    pub fn installed(&self) -> Vec<Installed> {
        let map: BTreeMap<String, Installed> = read(&self.installed_file());
        map.into_values().collect()
    }

    fn save_installed(
        &self,
        change: impl FnOnce(&mut BTreeMap<String, Installed>) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut map: BTreeMap<String, Installed> = read(&self.installed_file());
        change(&mut map)?;
        write(&self.installed_file(), &map)
    }

    /// Install (or reinstall) `plugin` from `marketplace`; keeps its on/off
    /// state and settings when it was installed before.
    pub fn install(&self, marketplace_name: &str, plugin: &str) -> Result<Installed, String> {
        let marketplace = self.marketplace(marketplace_name)?;
        let entry = marketplace.entry(plugin)?;
        if !valid_name(plugin) {
            return Err(format!("the plugin name `{plugin}` cannot be used"));
        }
        let source = marketplace::plugin_source(&marketplace, &entry)?;
        let staging = self
            .root
            .join("cache")
            .join(format!(".installing-{}", now_ms()));
        let (from, commit) = match &source {
            PluginSource::Local(path) => {
                if !path.is_dir() {
                    return Err(format!("{} does not exist", path.display()));
                }
                (path.clone(), None)
            }
            PluginSource::Git {
                url,
                reference,
                subdir,
            } => {
                marketplace::clone(url, reference.as_deref(), &staging)?;
                let commit = marketplace::head(&staging);
                let from = match subdir {
                    Some(sub) => staging.join(sub.trim_start_matches("./")),
                    None => staging.clone(),
                };
                if !from.is_dir() {
                    remove_dir(&staging);
                    return Err(format!(
                        "{} has no folder `{}`",
                        url,
                        subdir.as_deref().unwrap_or("")
                    ));
                }
                (from, commit)
            }
        };
        let manifest = manifest::read_manifest(&from).unwrap_or_default();
        let version = manifest
            .get("version")
            .and_then(Value::as_str)
            .or_else(|| entry.get("version").and_then(Value::as_str))
            .map(str::to_string)
            .or(commit)
            .unwrap_or_else(|| "0".into());
        let safe_version: String = version
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let dest = self
            .root
            .join("cache")
            .join(&marketplace.name)
            .join(plugin)
            .join(&safe_version);
        remove_dir(&dest);
        let copied = copy_dir(&from, &dest);
        remove_dir(&staging);
        copied?;
        let id = format!("{plugin}@{}", marketplace.name);
        let asked = requested_permissions(&dest);
        let mut result = None;
        self.save_installed(|map| {
            let previous = map.get(&id).cloned();
            let installed = match previous {
                // An update that asks for permissions not yet approved waits
                // as pending; the installed version runs on (FR-3).
                Some(previous)
                    if previous.path != dest
                        && asked
                            .iter()
                            .any(|p| !previous.approved_permissions.contains(p)) =>
                {
                    if let Some(old) = &previous.pending {
                        if old.path != dest {
                            remove_dir(&old.path);
                        }
                    }
                    Installed {
                        pending: Some(Pending {
                            version: version.clone(),
                            path: dest.clone(),
                            permissions: asked.clone(),
                        }),
                        ..previous
                    }
                }
                previous => {
                    let approved = previous
                        .as_ref()
                        .map(|p| p.approved_permissions.clone())
                        .unwrap_or_default();
                    let consented = asked.iter().all(|p| approved.contains(p));
                    // An older version's copy is no longer needed.
                    if let Some(previous) = &previous {
                        if previous.path != dest && previous.marketplace != LOCAL {
                            remove_dir(&previous.path);
                        }
                    }
                    carry(
                        previous.as_ref(),
                        Installed {
                            id: id.clone(),
                            name: plugin.to_string(),
                            marketplace: marketplace.name.clone(),
                            version: version.clone(),
                            path: dest.clone(),
                            // A plugin that asks for permissions starts off
                            // until the person approves them.
                            enabled: consented && previous.as_ref().is_none_or(|p| p.enabled),
                            installed_at: now_ms(),
                            approved_permissions: approved,
                            ..Default::default()
                        },
                    )
                }
            };
            map.insert(id.clone(), installed.clone());
            result = Some(installed);
            Ok(())
        })?;
        std::fs::create_dir_all(self.data_dir(&id)).ok();
        Ok(result.unwrap())
    }

    /// Use a plugin folder on this computer where it is (`name@local`).
    pub fn install_folder(&self, folder: &Path) -> Result<Installed, String> {
        if !folder.is_dir() {
            return Err(format!("{} is not a folder", folder.display()));
        }
        let manifest = manifest::read_manifest(folder)?;
        let name = manifest
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| folder.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        if !valid_name(&name) {
            return Err(format!("the plugin name `{name}` cannot be used"));
        }
        let id = format!("{name}@{LOCAL}");
        let installed = Installed {
            id: id.clone(),
            name,
            marketplace: LOCAL.into(),
            version: manifest
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("local")
                .to_string(),
            path: folder.to_path_buf(),
            enabled: true,
            installed_at: now_ms(),
            ..Default::default()
        };
        let asked = requested_permissions(folder);
        let mut result = installed.clone();
        self.save_installed(|map| {
            let previous = map.get(&id);
            let approved = previous
                .map(|p| p.approved_permissions.clone())
                .unwrap_or_default();
            let consented = asked.iter().all(|p| approved.contains(p));
            result = carry(
                previous,
                Installed {
                    enabled: consented && previous.is_none_or(|p| p.enabled),
                    approved_permissions: approved,
                    ..installed
                },
            );
            map.insert(id.clone(), result.clone());
            Ok(())
        })?;
        std::fs::create_dir_all(self.data_dir(&id)).ok();
        Ok(result)
    }

    /// Install again from its marketplace, fetched fresh.
    pub fn update(&self, id: &str) -> Result<Installed, String> {
        let current = self
            .installed()
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| format!("`{id}` is not installed"))?;
        if current.marketplace == LOCAL {
            return Ok(current);
        }
        self.update_marketplace(&current.marketplace)?;
        self.install(&current.marketplace, &current.name)
    }

    /// Remove a plugin (its data folder is kept).
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        self.save_installed(|map| {
            let gone = map
                .remove(id)
                .ok_or_else(|| format!("`{id}` is not installed"))?;
            if gone.marketplace != LOCAL {
                if let Some(plugin_dir) = gone.path.parent() {
                    remove_dir(plugin_dir);
                }
            }
            Ok(())
        })
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.save_installed(|map| {
            map.get_mut(id)
                .ok_or_else(|| format!("`{id}` is not installed"))?
                .enabled = enabled;
            Ok(())
        })
    }

    /// Approve `permissions` for `id` (those it does not ask for are
    /// ignored). Once everything asked for is approved the plugin is on, and
    /// a pending update takes the installed version's place (FR-3).
    pub fn approve(&self, id: &str, permissions: &[String]) -> Result<Installed, String> {
        let mut result = None;
        self.save_installed(|map| {
            let entry = map
                .get_mut(id)
                .ok_or_else(|| format!("`{id}` is not installed"))?;
            let mut asked = requested_permissions(&entry.path);
            if let Some(pending) = &entry.pending {
                asked.extend(pending.permissions.iter().cloned());
            }
            // Optional ones may be given too (FR-9).
            let mut allowed = all_permissions(&entry.path);
            if let Some(pending) = &entry.pending {
                allowed.extend(all_permissions(&pending.path));
            }
            for permission in permissions {
                if allowed.contains(permission) && !entry.approved_permissions.contains(permission)
                {
                    entry.approved_permissions.push(permission.clone());
                }
            }
            if asked.iter().all(|p| entry.approved_permissions.contains(p)) {
                if let Some(pending) = entry.pending.take() {
                    if entry.path != pending.path && entry.marketplace != LOCAL {
                        remove_dir(&entry.path);
                    }
                    entry.path = pending.path;
                    entry.version = pending.version;
                }
                entry.enabled = true;
            }
            result = Some(entry.clone());
            Ok(())
        })?;
        Ok(result.expect("set above"))
    }

    /// Take back one approved permission: calls that need it fail (FR-16).
    pub fn revoke(&self, id: &str, permission: &str) -> Result<(), String> {
        self.save_installed(|map| {
            map.get_mut(id)
                .ok_or_else(|| format!("`{id}` is not installed"))?
                .approved_permissions
                .retain(|p| p != permission);
            Ok(())
        })
    }

    /// Watch its files and reload its views on change (FR-14).
    pub fn set_developer_mode(&self, id: &str, on: bool) -> Result<(), String> {
        self.save_installed(|map| {
            map.get_mut(id)
                .ok_or_else(|| format!("`{id}` is not installed"))?
                .developer_mode = on;
            Ok(())
        })
    }

    /// Change one installed plugin's record; the changed record.
    pub fn edit(&self, id: &str, change: impl FnOnce(&mut Installed)) -> Result<Installed, String> {
        let mut result = None;
        self.save_installed(|map| {
            let entry = map
                .get_mut(id)
                .ok_or_else(|| format!("`{id}` is not installed"))?;
            change(entry);
            result = Some(entry.clone());
            Ok(())
        })?;
        Ok(result.expect("set above"))
    }

    /// Where plugins' sealed vaults live (`evo-vault`; plugin-settings FR-5).
    pub fn vault_dir(&self) -> PathBuf {
        self.root.join("vault")
    }

    pub fn set_config(&self, id: &str, config: Map<String, Value>) -> Result<(), String> {
        self.save_installed(|map| {
            map.get_mut(id)
                .ok_or_else(|| format!("`{id}` is not installed"))?
                .config = config;
            Ok(())
        })
    }

    /// One installed plugin, loaded.
    pub fn load(&self, installed: &Installed) -> Plugin {
        manifest::load(
            &installed.path,
            &installed.id,
            &installed.name,
            &self.data_dir(&installed.id),
            &installed.config,
        )
    }

    /// The plugins that are on, loaded, with the parts that may not run yet
    /// held back ([`gate`]; `waiting` says which and why).
    pub fn enabled(&self) -> Vec<Plugin> {
        self.installed()
            .iter()
            .filter(|i| i.enabled)
            .map(|i| {
                let mut plugin = self.load(i);
                gate(&mut plugin, i);
                plugin
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A marketplace with one plugin in it and one in a separate git repo.
    fn fixture(base: &Path) -> (PathBuf, PathBuf) {
        let market = base.join("market");
        write_file(
            &market.join(".evo-plugin/marketplace.json"),
            r#"{"name": "team", "owner": {"name": "Team"}, "plugins": [
                {"name": "notes", "source": "./plugins/notes", "description": "Notes"},
                {"name": "remote", "source": {"source": "url", "url": "REMOTE"}}
            ]}"#,
        );
        write_file(
            &market.join("plugins/notes/.evo-plugin/plugin.json"),
            r#"{"name": "notes", "version": "1.0.0"}"#,
        );
        write_file(
            &market.join("plugins/notes/commands/today.md"),
            "Write today's notes",
        );
        let remote = base.join("remote");
        write_file(
            &remote.join("skills/hello/SKILL.md"),
            "---\nname: hello\ndescription: Say hello\n---\nHi",
        );
        crate::git(&["init", "--quiet"], Some(&remote)).unwrap();
        crate::git(
            &["-c", "user.email=t@t", "-c", "user.name=t", "add", "."],
            Some(&remote),
        )
        .unwrap();
        crate::git(
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "--quiet",
                "-m",
                "init",
            ],
            Some(&remote),
        )
        .unwrap();
        let text = std::fs::read_to_string(market.join(".evo-plugin/marketplace.json")).unwrap();
        let url = format!(
            "file:///{}",
            remote
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
        );
        write_file(
            &market.join(".evo-plugin/marketplace.json"),
            &text.replace("REMOTE", &url),
        );
        (market, remote)
    }

    #[test]
    fn folder_marketplace_install_enable_update_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let (market, _) = fixture(dir.path());
        let store = Store::new(dir.path().join("plugins"));
        let added = store.add_marketplace(&market.to_string_lossy()).unwrap();
        assert_eq!(added.name, "team");
        assert!(
            store.add_marketplace(&market.to_string_lossy()).is_err(),
            "added twice"
        );
        let catalog = store.catalog();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0]["installed"], Value::Null);

        let notes = store.install("team", "notes").unwrap();
        assert_eq!(notes.id, "notes@team");
        assert_eq!(notes.version, "1.0.0");
        assert!(notes.path.join("commands/today.md").is_file());
        assert_eq!(store.enabled().len(), 1);
        assert_eq!(store.enabled()[0].commands.len(), 1);

        store.set_enabled("notes@team", false).unwrap();
        assert!(store.enabled().is_empty());
        let mut config = Map::new();
        config.insert("k".into(), json!("v"));
        store.set_config("notes@team", config).unwrap();
        // Reinstalling keeps on/off and settings.
        let again = store.update("notes@team").unwrap();
        assert!(!again.enabled);
        assert_eq!(again.config["k"], "v");

        store.uninstall("notes@team").unwrap();
        assert!(store.installed().is_empty());
        assert!(!notes.path.exists());
        store.remove_marketplace("team").unwrap();
        assert!(store.marketplaces().is_empty());
        assert!(market.exists(), "a folder marketplace is not deleted");
    }

    // docs/features/plugin-ui/spec.md (FR-3, FR-14, FR-16; T5).
    #[test]
    fn views_ask_for_consent_and_an_update_with_more_permissions_waits() {
        let dir = tempfile::tempdir().unwrap();
        let (market, _) = fixture(dir.path());
        let manifest = market.join("plugins/notes/.evo-plugin/plugin.json");
        write_file(
            &manifest,
            r#"{"name": "notes", "version": "1.0.0", "permissions": ["chat:read"]}"#,
        );
        let store = Store::new(dir.path().join("plugins"));
        store.add_marketplace(&market.to_string_lossy()).unwrap();

        // New: off until approved.
        let notes = store.install("team", "notes").unwrap();
        assert!(!notes.enabled);
        assert_eq!(notes.unapproved(), vec!["chat:read"]);
        let notes = store
            .approve("notes@team", &["chat:read".into(), "storage".into()])
            .unwrap();
        assert!(notes.enabled);
        assert_eq!(
            notes.approved_permissions,
            vec!["chat:read"],
            "only what it asks for"
        );
        assert!(notes.unapproved().is_empty());

        // An update asking for more: the installed version runs on.
        write_file(
            &manifest,
            r#"{"name": "notes", "version": "1.1.0", "permissions": ["chat:read", "chat:send"]}"#,
        );
        let updated = store.update("notes@team").unwrap();
        assert_eq!(updated.version, "1.0.0");
        assert!(updated.enabled);
        assert_eq!(
            updated.pending.as_ref().map(|p| p.version.as_str()),
            Some("1.1.0")
        );
        assert!(updated.path.is_dir(), "the running version is kept");
        assert_eq!(updated.unapproved(), vec!["chat:send"]);

        // Approving swaps it in and drops the old copy.
        let approved = store.approve("notes@team", &["chat:send".into()]).unwrap();
        assert_eq!(approved.version, "1.1.0");
        assert!(approved.pending.is_none());
        assert!(!updated.path.exists());

        store.revoke("notes@team", "chat:send").unwrap();
        store.set_developer_mode("notes@team", true).unwrap();
        let now = store
            .installed()
            .into_iter()
            .find(|i| i.id == "notes@team")
            .unwrap();
        assert_eq!(now.approved_permissions, vec!["chat:read"]);
        assert!(now.developer_mode);
        // Reinstalling keeps Developer mode; the revoked permission is asked again.
        let again = store.update("notes@team").unwrap();
        assert!(again.developer_mode);
        assert_eq!(again.unapproved(), vec!["chat:send"]);
    }

    #[test]
    fn installed_files_from_earlier_builds_still_read() {
        let old: Installed = serde_json::from_value(json!({
            "id": "a@m", "name": "a", "marketplace": "m", "version": "1", "path": "/x", "enabled": true
        }))
        .unwrap();
        assert!(
            old.approved_permissions.is_empty() && !old.developer_mode && old.pending.is_none()
        );
    }

    // docs/features/plugin-settings/spec.md (FR-4, FR-8, FR-9, FR-17; T5, T7).
    fn code_plugin(dir: &Path, manifest: &str) -> PathBuf {
        let root = dir.join("code-plugin");
        write_file(&root.join(".evo-plugin/plugin.json"), manifest);
        write_file(&root.join("server/main.ts"), "x");
        root
    }

    const CODE: &str = r#"{
      "name": "sync",
      "settings": { "space": { "kind": "text", "required": true }, "token": { "kind": "secret", "required": true } },
      "runtime": { "server": { "language": "typescript", "entry": "server/main.ts", "env": { "TOKEN": "${secret.token}" } } },
      "mcpServers": { "old": { "command": "run", "env": { "T": "${user_config.token}" } } },
      "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "x" }] }] },
      "permissions": [{ "id": "startup", "optional": true }]
    }"#;

    #[test]
    fn parts_wait_for_their_permissions_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let root = code_plugin(dir.path(), CODE);
        let store = Store::new(dir.path().join("plugins"));
        let installed = store.install_folder(&root).unwrap();
        assert!(
            !installed.enabled,
            "a plugin whose code needs permissions starts off"
        );
        let mut unapproved = installed.unapproved();
        unapproved.sort();
        assert_eq!(
            unapproved,
            ["hooks", "runtime", "secrets"],
            "optional `startup` is not asked for now"
        );

        // Approved, but settings missing: nothing runs.
        let installed = store
            .approve(
                &installed.id,
                &[
                    "hooks".into(),
                    "runtime".into(),
                    "secrets".into(),
                    "startup".into(),
                ],
            )
            .unwrap();
        assert!(installed.enabled);
        assert!(
            installed
                .approved_permissions
                .contains(&"startup".to_string()),
            "optional ones may be given"
        );
        let plugin = &store.enabled()[0];
        assert!(
            plugin.hooks.is_empty() && plugin.mcp_servers.is_empty() && plugin.runtime.is_none()
        );
        assert_eq!(plugin.waiting[0].part, "setup");
        assert_eq!(plugin.waiting[0].needs, ["space", "token"]);

        // Settings filled: everything runs.
        let mut config = Map::new();
        config.insert("space".into(), json!("team"));
        store.set_config(&installed.id, config).unwrap();
        store
            .save_installed(|map| {
                map.get_mut(&installed.id).unwrap().secrets_set = vec!["token".into()];
                Ok(())
            })
            .unwrap();
        let plugin = &store.enabled()[0];
        assert!(plugin.waiting.is_empty(), "{:?}", plugin.waiting);
        assert!(
            !plugin.hooks.is_empty() && !plugin.mcp_servers.is_empty() && plugin.runtime.is_some()
        );

        // Taking back `hooks` holds the hooks only; `secrets` holds the code.
        store.revoke(&installed.id, "hooks").unwrap();
        let plugin = &store.enabled()[0];
        assert!(plugin.hooks.is_empty() && plugin.runtime.is_some());
        store.revoke(&installed.id, "secrets").unwrap();
        let plugin = &store.enabled()[0];
        assert!(plugin.runtime.is_none() && plugin.mcp_servers.is_empty());
        assert!(plugin
            .waiting
            .iter()
            .any(|w| w.part == "runtime" && w.needs == ["secrets"]));
    }

    #[test]
    fn an_enabled_plugin_from_before_keeps_its_skills_while_its_code_waits() {
        let dir = tempfile::tempdir().unwrap();
        let root = code_plugin(
            dir.path(),
            r#"{ "name": "old", "mcpServers": { "s": { "command": "run" } } }"#,
        );
        write_file(
            &root.join("skills/a/SKILL.md"),
            "---\nname: a\ndescription: d\n---\n",
        );
        let store = Store::new(dir.path().join("plugins"));
        // As an earlier build left it: on, nothing approved.
        store
            .save_installed(|map| {
                map.insert(
                    "old@local".into(),
                    Installed {
                        id: "old@local".into(),
                        name: "old".into(),
                        marketplace: LOCAL.into(),
                        version: "1".into(),
                        path: root.clone(),
                        enabled: true,
                        ..Default::default()
                    },
                );
                Ok(())
            })
            .unwrap();
        let plugin = &store.enabled()[0];
        assert_eq!(plugin.skills.len(), 1, "skills keep working");
        assert!(plugin.mcp_servers.is_empty(), "its server waits for review");
        assert_eq!(store.installed()[0].unapproved(), ["runtime"]);
    }

    #[test]
    fn hooks_run_only_once_approved_and_stop_when_taken_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = code_plugin(
            dir.path(),
            r#"{ "name": "h", "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "echo hi" }] }] } }"#,
        );
        let store = Store::new(dir.path().join("plugins"));
        let approve = |list: &[&str]| {
            store
                .save_installed(|map| {
                    map.insert(
                        "h@local".into(),
                        Installed {
                            id: "h@local".into(),
                            name: "h".into(),
                            marketplace: LOCAL.into(),
                            version: "1".into(),
                            path: root.clone(),
                            enabled: true,
                            approved_permissions: list.iter().map(|s| s.to_string()).collect(),
                            ..Default::default()
                        },
                    );
                    Ok(())
                })
                .unwrap()
        };
        approve(&[]);
        assert!(
            store.enabled()[0].hooks.is_empty(),
            "not approved: no hooks"
        );
        approve(&["hooks"]);
        assert!(store.enabled()[0].hooks.contains_key("Stop"));
        approve(&[]);
        assert!(
            store.enabled()[0].hooks.is_empty(),
            "taken back: gone at the next read"
        );
    }

    #[test]
    fn a_reinstall_keeps_the_persons_choices() {
        let dir = tempfile::tempdir().unwrap();
        let root = code_plugin(dir.path(), r#"{ "name": "keep" }"#);
        let store = Store::new(dir.path().join("plugins"));
        let installed = store.install_folder(&root).unwrap();
        store
            .save_installed(|map| {
                let entry = map.get_mut(&installed.id).unwrap();
                entry.secrets_set = vec!["k".into()];
                entry.start_mode = Some(crate::runtime::StartMode::WithApp);
                entry.workspace_config.insert("ws1".into(), Map::new());
                entry.accounts.push(AccountRecord {
                    account: "gh".into(),
                    id: "a1".into(),
                    label: "ana".into(),
                    state: "signedIn".into(),
                    default: true,
                    ..Default::default()
                });
                Ok(())
            })
            .unwrap();
        let again = store.install_folder(&root).unwrap();
        assert_eq!(again.secrets_set, ["k"]);
        assert_eq!(again.start_mode, Some(crate::runtime::StartMode::WithApp));
        assert!(again.workspace_config.contains_key("ws1"));
        assert_eq!(again.accounts.len(), 1);
        let text = std::fs::read_to_string(store.root().join("installed.json")).unwrap();
        assert!(
            text.contains("\"secretsSet\"") && !text.contains("\"workspaceConfig\": {}\n"),
            "{text}"
        );
    }

    #[test]
    fn git_sources_clone_and_folders_install_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let (market, remote) = fixture(dir.path());
        // The marketplace itself from git.
        crate::git(&["init", "--quiet"], Some(&market)).unwrap();
        crate::git(
            &["-c", "user.email=t@t", "-c", "user.name=t", "add", "."],
            Some(&market),
        )
        .unwrap();
        crate::git(
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "--quiet",
                "-m",
                "m",
            ],
            Some(&market),
        )
        .unwrap();
        let url = format!(
            "file:///{}",
            market
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
        );
        let store = Store::new(dir.path().join("plugins"));
        let added = store.add_marketplace(&url).unwrap();
        assert!(added.location.starts_with(store.root()));
        // A plugin from its own git repository: versioned by commit.
        let remote_plugin = store.install("team", "remote").unwrap();
        assert_eq!(remote_plugin.version.len(), 12);
        assert!(remote_plugin.path.join("skills/hello/SKILL.md").is_file());
        assert_eq!(store.enabled()[0].skills.len(), 1);
        // A folder used in place.
        let local = store.install_folder(&remote).unwrap();
        assert_eq!(local.id, "remote@local");
        assert_eq!(local.path, remote);
        store.uninstall("remote@local").unwrap();
        assert!(remote.exists(), "a folder plugin is not deleted");
        store.remove_marketplace("team").unwrap();
        assert!(!added.location.exists());
    }
}
