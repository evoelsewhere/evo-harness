//! Which extensions may reach Evo through the host (docs/features/browser-bridge/spec.md FR-3; plan 3.3).
//!
//! The unpacked extension carries a fixed public key in its manifest, so its id
//! is stable; store listings get ids of their own, added here when published.

/// The public key in `extensions/evo-webbridge/manifest.json` (`key`).
pub const EXTENSION_KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA3MwA4ZQc11peL1zWPFAl3Nx1QgXcRrx1DAfbRb1+IVkf4CXW2goCrRnWZDU8CW9UPIZH8PeDWaMD4cEDL2qs2+ThoaOsPaBmXjRCmaQzMDvahos1Uq/Log2acRrOYV53gZ8R8MmbqAg8Wl9LB53C3qxKYFkfWeMR0fCKJd6UAqMGWhyW3lCK3nsRWS4mdLgJyavBOOTO9ozNWqOoaX1O2qLZ4h6kjWPBftEnOUreaEv18qh55nQi89T80mHEEqtvmo+j0yH4e77haav4V055OsXYJpgcFxIXC5v1/n+7RPhoWpEqTIwZFOI3EYTRymcdpgb4FswibJ83AL95xRtNxQIDAQAB";

/// The id that key gives the unpacked extension.
pub const UNPACKED_ID: &str = "jcmbhmbghlhihkdpjhnefhegafibckff";

/// Ids of the store listings (Chrome Web Store, Edge Add-ons), once published.
pub const STORE_IDS: &[&str] = &[];

/// Every extension id that may start the host.
pub fn allowed_ids() -> Vec<&'static str> {
    std::iter::once(UNPACKED_ID)
        .chain(STORE_IDS.iter().copied())
        .collect()
}

/// The origins in the host manifest's `allowed_origins`.
pub fn allowed_origins() -> Vec<String> {
    allowed_ids()
        .into_iter()
        .map(|id| format!("chrome-extension://{id}/"))
        .collect()
}

/// Whether `origin` (as the host reports it) is one of Evo's extensions.
pub fn is_allowed_origin(origin: &str) -> bool {
    allowed_origins().iter().any(|allowed| allowed == origin)
}

/// The id a browser gives an extension with this public key: the first 32 hex
/// digits of its SHA-256, written with the letters a–p.
pub fn id_from_key(key_base64: &str) -> Option<String> {
    use base64::Engine;
    use sha2::Digest;
    let der = base64::engine::general_purpose::STANDARD
        .decode(key_base64)
        .ok()?;
    let digest = sha2::Sha256::digest(der);
    Some(
        digest
            .iter()
            .take(16)
            .flat_map(|b| [b >> 4, b & 0xf])
            .map(|n| (b'a' + n) as char)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_gives_the_unpacked_id() {
        assert_eq!(id_from_key(EXTENSION_KEY).as_deref(), Some(UNPACKED_ID));
    }

    #[test]
    fn the_manifest_carries_the_same_key() {
        let manifest = include_str!("../../../../../extensions/evo-webbridge/manifest.json");
        let manifest: serde_json::Value = serde_json::from_str(manifest).unwrap();
        assert_eq!(manifest["key"].as_str(), Some(EXTENSION_KEY));
    }

    #[test]
    fn only_evo_extensions_are_allowed() {
        assert!(is_allowed_origin(&format!(
            "chrome-extension://{UNPACKED_ID}/"
        )));
        assert!(!is_allowed_origin(
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"
        ));
        assert!(!is_allowed_origin("https://example.com/"));
    }
}
