//! The unread count on Evo Mates' taskbar or dock icon
//! (docs/features/evo-mates-messaging/spec.md, FR-16): macOS and Linux take
//! a number; Windows takes an overlay picture, which the window draws (a
//! circle with the number) and hands over as RGBA pixels.

use tauri::{image::Image, AppHandle, Manager};

/// The side of the overlay picture the window draws, in pixels.
pub const OVERLAY_SIDE: u32 = 32;

/// Whether `rgba` holds a square overlay picture of [`OVERLAY_SIDE`].
pub fn overlay_fits(rgba: &[u8]) -> bool {
    rgba.len() == (OVERLAY_SIDE * OVERLAY_SIDE * 4) as usize
}

/// Show `count` on the window `label`'s icon (none at 0); `rgba` is the
/// overlay for Windows.
#[tauri::command]
pub fn mates_set_badge(app: AppHandle, window: String, count: u32, rgba: Option<Vec<u8>>) -> Result<(), String> {
    let Some(target) = app.get_webview_window(&window) else {
        return Ok(());
    };
    if cfg!(windows) {
        let icon = match rgba {
            Some(pixels) if count > 0 && overlay_fits(&pixels) => Some(Image::new_owned(pixels, OVERLAY_SIDE, OVERLAY_SIDE)),
            _ => None,
        };
        target.set_overlay_icon(icon).map_err(|error| error.to_string())
    } else {
        target
            .set_badge_count(if count == 0 { None } else { Some(i64::from(count)) })
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_square_overlay_of_the_agreed_side_fits() {
        assert!(overlay_fits(&vec![0; 32 * 32 * 4]));
        assert!(!overlay_fits(&vec![0; 16 * 16 * 4]));
        assert!(!overlay_fits(&[]));
    }
}
