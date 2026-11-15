//! A plugin's settings: what it asks the person for, described in its
//! manifest (`settings`, and the older `userConfig`) and drawn by evo
//! (docs/features/plugin-settings/spec.md, FR-2 – FR-4).

use serde_json::{Map, Value};

/// What a setting holds, and how evo draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Text,
    Multiline,
    Number,
    Boolean,
    /// One of `options`.
    Select,
    Folder,
    File,
    /// Kept in the vault; never shown back (FR-5).
    Secret,
}

/// Whose value it is: the person's, or one per workspace (FR-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    #[default]
    Personal,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    pub key: String,
    pub kind: Kind,
    pub title: String,
    pub description: Option<String>,
    pub default: Option<Value>,
    pub required: bool,
    pub group: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub pattern: Option<String>,
    pub options: Vec<String>,
    pub scope: Scope,
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn kind_of(raw: &str, sensitive: bool) -> Option<Kind> {
    Some(match raw {
        "text" | "string" if sensitive => Kind::Secret,
        "text" | "string" => Kind::Text,
        "multiline" => Kind::Multiline,
        "number" => Kind::Number,
        "boolean" => Kind::Boolean,
        "select" => Kind::Select,
        "folder" | "directory" => Kind::Folder,
        "file" => Kind::File,
        "secret" => Kind::Secret,
        _ => return None,
    })
}

/// One setting from its manifest object, or why it is left out.
fn parse_one(key: &str, value: &Value) -> Result<(Setting, Vec<String>), String> {
    if !valid_key(key) {
        return Err(format!(
            "settings: the key `{key}` may use only letters, digits, - and _"
        ));
    }
    let object = value
        .as_object()
        .ok_or_else(|| format!("settings.{key}: expected an object"))?;
    let text = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let number = |name: &str| object.get(name).and_then(Value::as_f64);
    let sensitive = object
        .get("sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // `kind` in `settings`, `type` in `userConfig`.
    let raw = text("kind")
        .or_else(|| text("type"))
        .unwrap_or_else(|| "text".into());
    let kind =
        kind_of(&raw, sensitive).ok_or_else(|| format!("settings.{key}: unknown kind `{raw}`"))?;
    let mut minor = Vec::new();
    let options: Vec<String> = object
        .get("options")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if kind == Kind::Select && options.is_empty() {
        return Err(format!("settings.{key}: a select needs `options`"));
    }
    let pattern = text("pattern").filter(|pattern| match regex::Regex::new(pattern) {
        Ok(_) => true,
        Err(_) => {
            minor.push(format!(
                "settings.{key}: the pattern `{pattern}` is not a valid regular expression"
            ));
            false
        }
    });
    let scope = match text("scope").as_deref() {
        None | Some("personal") => Scope::Personal,
        Some("workspace") => Scope::Workspace,
        Some(other) => {
            minor.push(format!(
                "settings.{key}: unknown scope `{other}` (personal or workspace)"
            ));
            Scope::Personal
        }
    };
    let mut setting = Setting {
        key: key.to_string(),
        kind,
        title: text("title").unwrap_or_else(|| key.to_string()),
        description: text("description"),
        default: None,
        required: object
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        group: text("group"),
        min: number("min"),
        max: number("max"),
        pattern,
        options,
        scope,
    };
    if let Some(default) = object.get("default").filter(|v| !v.is_null()) {
        if kind == Kind::Secret {
            minor.push(format!("settings.{key}: a secret cannot have a default"));
        } else {
            match check(&setting, default) {
                Ok(value) => setting.default = Some(value),
                Err(why) => minor.push(format!("settings.{key}: the default {why}")),
            }
        }
    }
    Ok((setting, minor))
}

/// Read `settings` and the older `userConfig` (the newer wins on a key both
/// have), in manifest order.
pub fn parse_settings(
    settings: Option<&Value>,
    legacy: Option<&Value>,
) -> (Vec<Setting>, Vec<String>) {
    let mut out: Vec<Setting> = Vec::new();
    let mut problems = Vec::new();
    for (field, value) in [("settings", settings), ("userConfig", legacy)] {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            continue;
        };
        let Some(map) = value.as_object() else {
            problems.push(format!("{field}: expected an object of settings"));
            continue;
        };
        for (key, item) in map {
            if out.iter().any(|s| &s.key == key) {
                continue;
            }
            match parse_one(key, item) {
                Ok((setting, mut minor)) => {
                    problems.append(&mut minor);
                    out.push(setting);
                }
                Err(problem) => problems.push(problem),
            }
        }
    }
    (out, problems)
}

/// `value` as `setting` keeps it (numbers from text, trimmed text), or why
/// it does not fit (FR-2).
pub fn check(setting: &Setting, value: &Value) -> Result<Value, String> {
    match setting.kind {
        Kind::Boolean => value
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| "must be on or off".to_string()),
        Kind::Number => {
            let number = match value {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => s.trim().parse::<f64>().ok(),
                _ => None,
            }
            .filter(|n| n.is_finite())
            .ok_or_else(|| "must be a number".to_string())?;
            if setting.min.is_some_and(|min| number < min) {
                return Err(format!("must be at least {}", setting.min.unwrap()));
            }
            if setting.max.is_some_and(|max| number > max) {
                return Err(format!("must be at most {}", setting.max.unwrap()));
            }
            Ok(serde_json::Number::from_f64(number)
                .map(Value::Number)
                .unwrap_or(Value::Null))
        }
        Kind::Select => {
            let text = value
                .as_str()
                .ok_or_else(|| "must be one of the choices".to_string())?;
            if setting.options.iter().any(|o| o == text) {
                Ok(Value::String(text.to_string()))
            } else {
                Err(format!("must be one of: {}", setting.options.join(", ")))
            }
        }
        Kind::Text | Kind::Multiline | Kind::Folder | Kind::File | Kind::Secret => {
            let text = value.as_str().ok_or_else(|| "must be text".to_string())?;
            let text = if setting.kind == Kind::Multiline || setting.kind == Kind::Secret {
                text
            } else {
                text.trim()
            };
            let length = text.chars().count() as f64;
            if setting.min.is_some_and(|min| length < min) {
                return Err(format!(
                    "must have at least {} characters",
                    setting.min.unwrap()
                ));
            }
            if setting.max.is_some_and(|max| length > max) {
                return Err(format!(
                    "must have at most {} characters",
                    setting.max.unwrap()
                ));
            }
            if let Some(pattern) = &setting.pattern {
                let whole = format!("^(?:{pattern})$");
                if regex::Regex::new(&whole).is_ok_and(|re| !re.is_match(text)) {
                    return Err("is not in the expected form".to_string());
                }
            }
            Ok(Value::String(text.to_string()))
        }
    }
}

/// Check every value in `values` (secrets are not in it); the problems by key.
pub fn validate(settings: &[Setting], values: &Map<String, Value>) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    for (key, value) in values {
        match settings.iter().find(|s| &s.key == key) {
            None => problems.push((key.clone(), "is not a setting of this plugin".to_string())),
            Some(setting) if setting.kind == Kind::Secret => problems.push((
                key.clone(),
                "is a secret: it is kept in the vault, not here".to_string(),
            )),
            Some(setting) if !value.is_null() => {
                if let Err(why) = check(setting, value) {
                    problems.push((key.clone(), why));
                }
            }
            Some(_) => {}
        }
    }
    problems
}

/// The required settings with no value: not in `values`, no default, and
/// (for secrets) not among `secrets_set` (FR-4).
pub fn missing_required(
    settings: &[Setting],
    values: &Map<String, Value>,
    secrets_set: &[String],
) -> Vec<String> {
    settings
        .iter()
        .filter(|s| s.required)
        .filter(|s| match s.kind {
            Kind::Secret => !secrets_set.contains(&s.key),
            _ => {
                s.default.is_none()
                    && values
                        .get(&s.key)
                        .is_none_or(|v| v.is_null() || v.as_str().is_some_and(str::is_empty))
            }
        })
        .map(|s| s.key.clone())
        .collect()
}

/// The values a plugin's processes see: defaults, then the person's, then
/// the workspace's (FR-3). Secrets are not here.
pub fn effective(
    settings: &[Setting],
    personal: &Map<String, Value>,
    workspace: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for setting in settings.iter().filter(|s| s.kind != Kind::Secret) {
        let chosen = workspace
            .filter(|_| setting.scope == Scope::Workspace)
            .and_then(|w| w.get(&setting.key))
            .or_else(|| personal.get(&setting.key))
            .or(setting.default.as_ref());
        if let Some(value) = chosen.filter(|v| !v.is_null()) {
            out.insert(setting.key.clone(), value.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests;
