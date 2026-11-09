//! Set-of-marks (docs/features/computer-app-control, FR-14): a screenshot with
//! the visible controls boxed and numbered by their refs, so a model picks
//! `click e17` rather than guessing pixels. The marks come from the snapshot's
//! own lines (`… [ref=e17] @x,y WxH`, screenshot pixels), so a number on the
//! picture is always a ref the agent can use.

use image::{Rgba, RgbaImage};

/// One numbered box.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mark {
    pub number: u64,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// At most this many marks are drawn; more would hide the picture.
pub(crate) const MAX_MARKS: usize = 150;

/// Roles worth a mark: what a person would click, type into or pick.
const MARKED_ROLES: &[&str] = &[
    "Button", "SplitButton", "CheckBox", "RadioButton", "ComboBox", "Edit", "Document", "Hyperlink", "ListItem",
    "MenuItem", "TabItem", "TreeItem", "DataItem", "Slider", "Spinner", "MenuBar", "Menu",
];

/// The marks of a snapshot's lines: elements of a marked role with a ref and a
/// box at least 6×6 pixels, in order.
pub(crate) fn from_snapshot(lines: &[String]) -> Vec<Mark> {
    lines
        .iter()
        .filter_map(|line| {
            let role = line.trim_start().strip_prefix("- ")?.split([' ', '"']).next()?;
            if !MARKED_ROLES.contains(&role) {
                return None;
            }
            let after = line.rsplit_once("[ref=e")?.1;
            let (number, rest) = after.split_once(']')?;
            let number: u64 = number.parse().ok()?;
            let (place, size) = rest.trim().strip_prefix('@')?.split_once(' ')?;
            let (x, y) = place.split_once(',')?;
            let (width, height) = size.trim().split_once('x')?;
            let mark = Mark {
                number,
                x: x.parse().ok()?,
                y: y.parse().ok()?,
                width: width.parse().ok()?,
                height: height.parse().ok()?,
            };
            (mark.width >= 6 && mark.height >= 6).then_some(mark)
        })
        .take(MAX_MARKS)
        .collect()
}

/// 3×5 digits, one row per `u8`, high bit first of the low 3 bits.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];
/// Each font pixel is drawn this many image pixels wide and high.
const SCALE: i64 = 2;
const BOX: Rgba<u8> = Rgba([255, 0, 170, 255]);
const LABEL: Rgba<u8> = Rgba([255, 0, 170, 255]);
const INK: Rgba<u8> = Rgba([255, 255, 255, 255]);

fn put(image: &mut RgbaImage, x: i64, y: i64, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < image.width() && (y as u32) < image.height() {
        image.put_pixel(x as u32, y as u32, color);
    }
}

fn fill(image: &mut RgbaImage, x: i64, y: i64, width: i64, height: i64, color: Rgba<u8>) {
    for dy in 0..height {
        for dx in 0..width {
            put(image, x + dx, y + dy, color);
        }
    }
}

/// The size of a label for `number`: (width, height) in pixels.
pub(crate) fn label_size(number: u64) -> (i64, i64) {
    let digits = number.to_string().len() as i64;
    (digits * 4 * SCALE + SCALE, 5 * SCALE + 2 * SCALE)
}

/// Draw each mark's box outline and its number in a filled label at the box's
/// top-left corner (inside the picture).
pub(crate) fn draw(image: &mut RgbaImage, marks: &[Mark]) {
    for mark in marks {
        let (x, y, w, h) = (mark.x, mark.y, mark.width, mark.height);
        for dx in 0..w {
            put(image, x + dx, y, BOX);
            put(image, x + dx, y + h - 1, BOX);
        }
        for dy in 0..h {
            put(image, x, y + dy, BOX);
            put(image, x + w - 1, y + dy, BOX);
        }
        let (label_w, label_h) = label_size(mark.number);
        let lx = x.clamp(0, (image.width() as i64 - label_w).max(0));
        let ly = y.clamp(0, (image.height() as i64 - label_h).max(0));
        fill(image, lx, ly, label_w, label_h, LABEL);
        for (index, digit) in mark.number.to_string().bytes().enumerate() {
            let glyph = DIGITS[(digit - b'0') as usize];
            let gx = lx + SCALE + index as i64 * 4 * SCALE;
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..3 {
                    if bits & (0b100 >> col) != 0 {
                        fill(image, gx + col * SCALE, ly + SCALE + row as i64 * SCALE, SCALE, SCALE, INK);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_marks_from_snapshot_lines() {
        let lines: Vec<String> = [
            "UI of notepad.exe — \"a\"",
            "- Window \"a\" [ref=e1] @0,0 1280x800",
            "  - Button \"Save\" [ref=e17] @96,40 24x24",
            "  - Edit \"Name\" value=\"x\" [ref=e18] @10,60 200x20",
            "  - Button \"tiny\" [ref=e19] @1,1 4x4",
            "  - Text \"label\" [ref=e20] @1,1 40x10",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let marks = from_snapshot(&lines);
        assert_eq!(
            marks,
            vec![
                Mark { number: 17, x: 96, y: 40, width: 24, height: 24 },
                Mark { number: 18, x: 10, y: 60, width: 200, height: 20 },
            ],
            "windows, text and tiny boxes get no mark"
        );
    }

    #[test]
    fn draws_a_box_and_a_numbered_label() {
        let mut image = RgbaImage::from_pixel(100, 60, Rgba([0, 0, 0, 255]));
        draw(&mut image, &[Mark { number: 7, x: 20, y: 20, width: 40, height: 30 }]);
        // The outline's far corner and the label's background are drawn.
        assert_eq!(*image.get_pixel(59, 49), BOX);
        assert_eq!(*image.get_pixel(20, 20), LABEL);
        // Some ink of the digit is inside the label.
        let (w, h) = label_size(7);
        let ink = (20..20 + w as u32).flat_map(|x| (20..20 + h as u32).map(move |y| (x, y))).filter(|(x, y)| *image.get_pixel(*x, *y) == INK).count();
        assert!(ink > 0);
        // A mark past the edge is clipped, not a panic.
        draw(&mut image, &[Mark { number: 123, x: 95, y: 55, width: 30, height: 30 }]);
    }
}
