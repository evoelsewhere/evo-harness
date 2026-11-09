//! `zoom` (docs/features/computer-app-control, FR-20): an enlarged crop of a
//! region of the latest screenshot, for small or dense targets. The agent can
//! then aim inside the crop: a pointer action with `"in_zoom": true` gives its
//! coordinates in the crop's pixels, and they are mapped back to the
//! screenshot's here before the backend sees them.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

/// The crop is at most this large (screenshot pixels), so the enlarged
/// picture stays within the model's image size.
pub(crate) const MAX_REGION: (f64, f64) = (640.0, 400.0);
pub(crate) const FACTOR: f64 = 2.0;

/// A crop of the screenshot: its top-left and size in screenshot pixels, and
/// how much it is enlarged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub factor: f64,
}

/// The region asked for (`[x, y, width, height]`), kept inside a screenshot
/// of `size` and at most [`MAX_REGION`] large.
pub(crate) fn region(asked: &Value, size: (f64, f64)) -> Result<Region, String> {
    let numbers: Vec<f64> = asked
        .as_array()
        .ok_or("zoom needs region: [x, y, width, height] in screenshot pixels.")?
        .iter()
        .map(|v| v.as_f64().ok_or("region holds numbers only."))
        .collect::<Result<_, _>>()?;
    let [x, y, width, height] = numbers[..] else {
        return Err("region is [x, y, width, height].".into());
    };
    if width < 4.0 || height < 4.0 {
        return Err("The region is too small: at least 4×4 pixels.".into());
    }
    let width = width.min(MAX_REGION.0).min(size.0);
    let height = height.min(MAX_REGION.1).min(size.1);
    let x = x.clamp(0.0, (size.0 - width).max(0.0));
    let y = y.clamp(0.0, (size.1 - height).max(0.0));
    Ok(Region { x, y, width, height, factor: FACTOR })
}

impl Region {
    /// A point in the enlarged crop, as screenshot pixels.
    pub(crate) fn map_back(&self, x: f64, y: f64) -> (f64, f64) {
        (self.x + x / self.factor, self.y + y / self.factor)
    }
}

/// The latest zoom of each chat, until its next screenshot or zoom.
static LAST: LazyLock<Mutex<HashMap<String, Region>>> = LazyLock::new(Default::default);

pub(crate) fn remember(chat: &str, region: Region) {
    LAST.lock().expect("zoom lock").insert(chat.to_string(), region);
}

pub(crate) fn forget(chat: &str) {
    LAST.lock().expect("zoom lock").remove(chat);
}

/// `params` with crop coordinates (`"in_zoom": true`) mapped back to the
/// screenshot's; unchanged without `in_zoom`.
pub(crate) fn map_params(chat: &str, mut params: Value) -> Result<Value, String> {
    if params.get("in_zoom").and_then(Value::as_bool) != Some(true) {
        return Ok(params);
    }
    let region = LAST.lock().expect("zoom lock").get(chat).copied().ok_or("in_zoom needs a zoom first: call zoom on the region, then aim inside it.")?;
    let object = params.as_object_mut().ok_or("bad params")?;
    object.remove("in_zoom");
    for (kx, ky) in [("x", "y"), ("to_x", "to_y")] {
        if let (Some(x), Some(y)) = (object.get(kx).and_then(Value::as_f64), object.get(ky).and_then(Value::as_f64)) {
            let (sx, sy) = region.map_back(x, y);
            object.insert(kx.into(), serde_json::json!(sx.round()));
            object.insert(ky.into(), serde_json::json!(sy.round()));
        }
    }
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_the_region_inside_the_screenshot_and_small_enough() {
        let r = region(&json!([1200, 780, 300, 100]), (1280.0, 800.0)).unwrap();
        assert_eq!((r.x, r.y, r.width, r.height), (980.0, 700.0, 300.0, 100.0));
        let r = region(&json!([0, 0, 2000, 2000]), (1280.0, 800.0)).unwrap();
        assert_eq!((r.width, r.height), MAX_REGION);
        assert!(region(&json!([0, 0, 2, 2]), (1280.0, 800.0)).is_err());
        assert!(region(&json!([0, 0]), (1280.0, 800.0)).is_err());
    }

    #[test]
    fn a_point_in_the_crop_maps_back_to_the_screenshot() {
        let chat = "zoom-test";
        remember(chat, Region { x: 100.0, y: 50.0, width: 200.0, height: 100.0, factor: 2.0 });
        let mapped = map_params(chat, json!({ "x": 40, "y": 20, "in_zoom": true })).unwrap();
        assert_eq!(mapped, json!({ "x": 120.0, "y": 60.0 }));
        let drag = map_params(chat, json!({ "x": 0, "y": 0, "to_x": 400, "to_y": 200, "in_zoom": true })).unwrap();
        assert_eq!(drag, json!({ "x": 100.0, "y": 50.0, "to_x": 300.0, "to_y": 150.0 }));
        // Without in_zoom nothing changes; without a zoom it is refused.
        assert_eq!(map_params(chat, json!({ "x": 1, "y": 2 })).unwrap(), json!({ "x": 1, "y": 2 }));
        forget(chat);
        assert!(map_params(chat, json!({ "x": 1, "y": 2, "in_zoom": true })).is_err());
    }
}
