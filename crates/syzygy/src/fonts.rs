//! The system fonts, as the text stack finds them.
//!
//! fontdb lists a variable font once, at its default instance's weight:
//! Noto Sans CJK's is Thin. cosmic-text picks a script's preset fallback
//! (Noto Sans CJK JP for Japanese) only at the exact weight asked for, so
//! Regular and Bold Japanese skipped it and tried every installed font, one
//! character at a time: ~0.75 ms a character, a second for a song's
//! lyrics. Listing each variable font at every standard weight its axis
//! covers makes the preset match; cosmic-text sets the axis to the weight
//! it draws.

use iced::advanced::graphics::text::{cosmic_text::fontdb, font_system};

/// The standard weights, Thin to Black.
const WEIGHTS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// List the variable system fonts at the weights they cover. Before any
/// text is laid out.
pub fn list_variable_weights() {
    let mut system = font_system().write().expect("Write font system");
    let db = system.raw().db_mut();
    let extra: Vec<fontdb::FaceInfo> = db
        .faces()
        .flat_map(|face| {
            let (min, max) = db
                .with_face_data(face.id, weight_axis)
                .flatten()
                .unwrap_or((0.0, 0.0));
            WEIGHTS
                .into_iter()
                .filter(move |&weight| {
                    weight != face.weight.0 && (min..=max).contains(&f32::from(weight))
                })
                .map(|weight| fontdb::FaceInfo {
                    weight: fontdb::Weight(weight),
                    ..face.clone()
                })
        })
        .collect();
    log::debug!("Listed variable fonts at {} more weights", extra.len());
    for face in extra {
        db.push_face_info(face);
    }
}

/// The range of the font's weight axis, if it has one.
fn weight_axis(data: &[u8], index: u32) -> Option<(f32, f32)> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    face.variation_axes()
        .into_iter()
        .find(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"wght"))
        .map(|axis| (axis.min_value, axis.max_value))
}
