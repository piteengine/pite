use ab_glyph::{Font, FontArc, PxScale};
use anyhow::{Context, Result};

pub const FONT_BYTES: &[u8] = include_bytes!("../../fonts/Inter-Regular.ttf");

pub struct BakedText {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

pub struct TextAtlas {
    font: FontArc,
}

impl TextAtlas {
    pub fn new() -> Result<Self> {
        Self::from_bytes(FONT_BYTES)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let font = FontArc::try_from_vec(bytes.to_vec()).context("cannot parse bundled font")?;
        Ok(Self { font })
    }

    pub fn has_glyph(&self, ch: char) -> bool {
        let gid = self.font.glyph_id(ch);
        gid.0 != 0
    }

    pub fn bake(&self, text: &str, px: f32, color: [u8; 4]) -> BakedText {
        let px = px.max(1.0);
        let scale = PxScale::from(px);
        let units = self.font.units_per_em().unwrap_or(2048.0);
        let ascent = self.font.ascent_unscaled() * px / units;
        let descent = self.font.descent_unscaled().abs() * px / units;
        let mut width = 0.0f32;
        let mut glyphs = Vec::new();
        for ch in text.chars() {
            let gid = self.font.glyph_id(ch);
            let advance = self.font.h_advance_unscaled(gid) * px / units;
            glyphs.push((gid, width));
            width += advance;
        }
        let w = width.ceil().max(1.0) as u32;
        let h = (ascent + descent).ceil().max(1.0) as u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for (gid, pen_x) in glyphs {
            let positioned = gid.with_scale_and_position(scale, ab_glyph::point(pen_x, ascent));
            let Some(outlined) = self.font.outline_glyph(positioned) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, v| {
                let x = (bounds.min.x as i32 + gx as i32).clamp(0, w as i32 - 1);
                let y = (bounds.min.y as i32 + gy as i32).clamp(0, h as i32 - 1);
                let i = ((y as u32 * w + x as u32) * 4) as usize;
                let a = (v * color[3] as f32).round() as u8;
                rgba[i] = color[0];
                rgba[i + 1] = color[1];
                rgba[i + 2] = color[2];
                rgba[i + 3] = rgba[i + 3].max(a);
            });
        }
        BakedText { rgba, w, h }
    }
}

pub fn parse_color(s: &str) -> [u8; 4] {
    let hex = s.strip_prefix('#').unwrap_or(s);
    let digits: Vec<u8> = match hex.len() {
        3 => hex
            .chars()
            .flat_map(|c| [c, c])
            .collect::<String>()
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|pair| u8::from_str_radix(&pair.iter().collect::<String>(), 16).unwrap_or(255))
            .collect(),
        6 | 8 => (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(255))
            .collect(),
        _ => return [255, 255, 255, 255],
    };
    let mut out = [255, 255, 255, 255];
    for (i, d) in digits.iter().take(4).enumerate() {
        out[i] = *d;
    }
    if hex.len() == 6 {
        out[3] = 255;
    }
    out
}

pub fn glyph_cache_key(text: &str, px: u32, color: [u8; 4]) -> String {
    format!(
        "text\0{text}\0{px}\0{}\0{}\0{}\0{}",
        color[0], color[1], color[2], color[3]
    )
}

pub fn lru_touch(order: &mut std::collections::VecDeque<String>, key: &str, cap: usize) {
    order.retain(|k| k != key);
    order.push_back(key.to_string());
    while order.len() > cap {
        order.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_parses_and_covers_latin() {
        let atlas = TextAtlas::new().unwrap();
        assert!(atlas.has_glyph('H'));
        assert!(atlas.has_glyph('3'));
        assert!(atlas.has_glyph(':'));
    }

    #[test]
    fn baked_text_has_ink() {
        let atlas = TextAtlas::new().unwrap();
        let baked = atlas.bake("HP: 3", 20.0, [255, 255, 255, 255]);
        assert!(baked.w > 10 && baked.h > 10);
        let inked = baked.rgba.chunks(4).filter(|px| px[3] > 0).count();
        assert!(inked > 20, "expected visible glyphs, got {inked}");
    }

    #[test]
    fn empty_text_bakes_fully_transparent() {
        let atlas = TextAtlas::new().unwrap();
        let baked = atlas.bake("", 20.0, [255, 255, 255, 255]);
        assert_eq!(baked.w, 1);
        assert!(baked.rgba.iter().all(|b| *b == 0));
    }

    #[test]
    fn color_forms_parse() {
        assert_eq!(parse_color("#ff0000"), [255, 0, 0, 255]);
        assert_eq!(parse_color("#0f0"), [0, 255, 0, 255]);
        assert_eq!(parse_color("#00000000"), [0, 0, 0, 0]);
        assert_eq!(parse_color("junk"), [255, 255, 255, 255]);
    }
}
