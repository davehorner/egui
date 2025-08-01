use crate::{
    Color32, Rgba, gamma_u8_from_linear_f32, linear_f32_from_gamma_u8, linear_u8_from_linear_f32,
};

/// Hue, saturation, value, alpha. All in the range [0, 1].
/// No premultiplied alpha.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hsva {
    /// hue 0-1
    pub h: f32,

    /// saturation 0-1
    pub s: f32,

    /// value 0-1
    pub v: f32,

    /// alpha 0-1. A negative value signifies an additive color (and alpha is ignored).
    pub a: f32,
}

impl Hsva {
    #[inline]
    pub fn new(h: f32, s: f32, v: f32, a: f32) -> Self {
        Self { h, s, v, a }
    }

    /// From `sRGBA` with premultiplied alpha
    #[inline]
    pub fn from_srgba_premultiplied([r, g, b, a]: [u8; 4]) -> Self {
        Self::from(Color32::from_rgba_premultiplied(r, g, b, a))
    }

    /// From `sRGBA` without premultiplied alpha
    #[inline]
    pub fn from_srgba_unmultiplied([r, g, b, a]: [u8; 4]) -> Self {
        Self::from(Color32::from_rgba_unmultiplied(r, g, b, a))
    }

    /// From linear RGBA with premultiplied alpha
    #[inline]
    pub fn from_rgba_premultiplied(r: f32, g: f32, b: f32, a: f32) -> Self {
        #![allow(clippy::many_single_char_names)]
        if a <= 0.0 {
            if r == 0.0 && b == 0.0 && a == 0.0 {
                Self::default()
            } else {
                Self::from_additive_rgb([r, g, b])
            }
        } else {
            let (h, s, v) = hsv_from_rgb([r / a, g / a, b / a]);
            Self { h, s, v, a }
        }
    }

    /// From linear RGBA without premultiplied alpha
    #[inline]
    pub fn from_rgba_unmultiplied(r: f32, g: f32, b: f32, a: f32) -> Self {
        #![allow(clippy::many_single_char_names)]
        let (h, s, v) = hsv_from_rgb([r, g, b]);
        Self { h, s, v, a }
    }

    #[inline]
    pub fn from_additive_rgb(rgb: [f32; 3]) -> Self {
        let (h, s, v) = hsv_from_rgb(rgb);
        Self {
            h,
            s,
            v,
            a: -0.5, // anything negative is treated as additive
        }
    }

    #[inline]
    pub fn from_additive_srgb([r, g, b]: [u8; 3]) -> Self {
        Self::from_additive_rgb([
            linear_f32_from_gamma_u8(r),
            linear_f32_from_gamma_u8(g),
            linear_f32_from_gamma_u8(b),
        ])
    }

    #[inline]
    pub fn from_rgb(rgb: [f32; 3]) -> Self {
        let (h, s, v) = hsv_from_rgb(rgb);
        Self { h, s, v, a: 1.0 }
    }

    #[inline]
    pub fn from_srgb([r, g, b]: [u8; 3]) -> Self {
        Self::from_rgb([
            linear_f32_from_gamma_u8(r),
            linear_f32_from_gamma_u8(g),
            linear_f32_from_gamma_u8(b),
        ])
    }

    // ------------------------------------------------------------------------

    #[inline]
    pub fn to_opaque(self) -> Self {
        Self { a: 1.0, ..self }
    }

    #[inline]
    pub fn to_rgb(&self) -> [f32; 3] {
        rgb_from_hsv((self.h, self.s, self.v))
    }

    #[inline]
    pub fn to_srgb(&self) -> [u8; 3] {
        let [r, g, b] = self.to_rgb();
        [
            gamma_u8_from_linear_f32(r),
            gamma_u8_from_linear_f32(g),
            gamma_u8_from_linear_f32(b),
        ]
    }

    #[inline]
    pub fn to_rgba_premultiplied(&self) -> [f32; 4] {
        let [r, g, b, a] = self.to_rgba_unmultiplied();
        let additive = a < 0.0;
        if additive {
            [r, g, b, 0.0]
        } else {
            [a * r, a * g, a * b, a]
        }
    }

    /// To linear space rgba in 0-1 range.
    ///
    /// Represents additive colors using a negative alpha.
    #[inline]
    pub fn to_rgba_unmultiplied(&self) -> [f32; 4] {
        let Self { h, s, v, a } = *self;
        let [r, g, b] = rgb_from_hsv((h, s, v));
        [r, g, b, a]
    }

    #[inline]
    pub fn to_srgba_premultiplied(&self) -> [u8; 4] {
        Color32::from(*self).to_array()
    }

    /// To gamma-space 0-255.
    #[inline]
    pub fn to_srgba_unmultiplied(&self) -> [u8; 4] {
        let [r, g, b, a] = self.to_rgba_unmultiplied();
        [
            gamma_u8_from_linear_f32(r),
            gamma_u8_from_linear_f32(g),
            gamma_u8_from_linear_f32(b),
            linear_u8_from_linear_f32(a.abs()),
        ]
    }
}

impl From<Hsva> for Rgba {
    #[inline]
    fn from(hsva: Hsva) -> Self {
        Self(hsva.to_rgba_premultiplied())
    }
}

impl From<Rgba> for Hsva {
    #[inline]
    fn from(rgba: Rgba) -> Self {
        Self::from_rgba_premultiplied(rgba.0[0], rgba.0[1], rgba.0[2], rgba.0[3])
    }
}

impl From<Hsva> for Color32 {
    #[inline]
    fn from(hsva: Hsva) -> Self {
        Self::from(Rgba::from(hsva))
    }
}

impl From<Color32> for Hsva {
    #[inline]
    fn from(srgba: Color32) -> Self {
        Self::from(Rgba::from(srgba))
    }
}

/// All ranges in 0-1, rgb is linear.
#[inline]
pub fn hsv_from_rgb([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    #![allow(clippy::many_single_char_names)]
    let min = r.min(g.min(b));
    let max = r.max(g.max(b)); // value

    let range = max - min;

    let h = if max == min {
        0.0 // hue is undefined
    } else if max == r {
        (g - b) / (6.0 * range)
    } else if max == g {
        (b - r) / (6.0 * range) + 1.0 / 3.0
    } else {
        // max == b
        (r - g) / (6.0 * range) + 2.0 / 3.0
    };
    let h = (h + 1.0).fract(); // wrap
    let s = if max == 0.0 { 0.0 } else { 1.0 - min / max };
    (h, s, max)
}

/// All ranges in 0-1, rgb is linear.
#[inline]
pub fn rgb_from_hsv((h, s, v): (f32, f32, f32)) -> [f32; 3] {
    #![allow(clippy::many_single_char_names)]
    let h = (h.fract() + 1.0).fract(); // wrap
    let s = s.clamp(0.0, 1.0);

    let f = h * 6.0 - (h * 6.0).floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);

    match (h * 6.0).floor() as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        5 => [v, p, q],
        _ => unreachable!(),
    }
}

#[test]
#[ignore] // a bit expensive
fn test_hsv_roundtrip() {
    for r in 0..=255 {
        for g in 0..=255 {
            for b in 0..=255 {
                let srgba = Color32::from_rgb(r, g, b);
                let hsva = Hsva::from(srgba);
                assert_eq!(srgba, Color32::from(hsva));
            }
        }
    }
}
