//! Colour maths for the contrast rule: parse a CSS colour, WCAG 2.x relative luminance and
//! contrast ratio. Pure functions only.

/// An sRGB colour with straight (not premultiplied) alpha, channels 0–255 and alpha 0–1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub const fn opaque(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Paints `self` over an opaque `backdrop` (source-over).
    pub fn over(self, backdrop: Rgba) -> Rgba {
        let mix = |f: f64, b: f64| f * self.a + b * (1.0 - self.a);
        Rgba::opaque(
            mix(self.r, backdrop.r),
            mix(self.g, backdrop.g),
            mix(self.b, backdrop.b),
        )
    }

    /// WCAG 2.x relative luminance of the opaque colour.
    pub fn luminance(self) -> f64 {
        let channel = |c: f64| {
            let s = c / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }
}

/// WCAG 2.x contrast ratio between two opaque colours, 1.0–21.0.
pub fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` and `rgba()` (comma or space syntax).
pub fn parse_colour(value: &str) -> Option<Rgba> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    let inner = v
        .strip_prefix("rgba(")
        .or_else(|| v.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split([',', ' ', '/'])
        .filter(|p| !p.is_empty())
        .collect();
    let num = |s: &str| s.parse::<f64>().ok();
    match parts.as_slice() {
        [r, g, b] => Some(Rgba::opaque(num(r)?, num(g)?, num(b)?)),
        [r, g, b, a] => Some(Rgba {
            r: num(r)?,
            g: num(g)?,
            b: num(b)?,
            a: num(a)?,
        }),
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    let byte = |s: &str| u8::from_str_radix(s, 16).ok().map(f64::from);
    let nibble = |c: char| c.to_digit(16).map(|d| f64::from(d * 17));
    let chars: Vec<char> = hex.chars().collect();
    match chars.len() {
        3 | 4 => {
            let a = chars.get(3).map_or(Some(255.0), |&c| nibble(c))?;
            Some(Rgba {
                r: nibble(chars[0])?,
                g: nibble(chars[1])?,
                b: nibble(chars[2])?,
                a: a / 255.0,
            })
        }
        6 | 8 => {
            let a = if hex.len() == 8 {
                byte(&hex[6..8])?
            } else {
                255.0
            };
            Some(Rgba {
                r: byte(&hex[0..2])?,
                g: byte(&hex[2..4])?,
                b: byte(&hex[4..6])?,
                a: a / 255.0,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgba = Rgba::opaque(255.0, 255.0, 255.0);
    const BLACK: Rgba = Rgba::opaque(0.0, 0.0, 0.0);

    #[test]
    fn black_on_white_is_21() {
        assert!((contrast(BLACK, WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn matches_known_ratios() {
        // #767676 on white is the classic 4.54:1 grey.
        let grey = parse_colour("#767676").unwrap();
        assert!((contrast(grey, WHITE) - 4.54).abs() < 0.01);
    }

    #[test]
    fn parses_hex_and_rgb_forms() {
        assert_eq!(parse_colour("#fff"), Some(WHITE));
        assert_eq!(parse_colour("#000000"), Some(BLACK));
        assert_eq!(parse_colour("rgb(255, 255, 255)"), Some(WHITE));
        let half = parse_colour("rgba(0, 0, 0, .5)").unwrap();
        assert!((half.a - 0.5).abs() < 1e-9);
        assert!(parse_colour("#ff80").is_some_and(|c| c.a < 0.01));
        assert_eq!(parse_colour("tomato"), None);
    }

    #[test]
    fn composites_alpha_over_backdrop() {
        let half = Rgba { a: 0.5, ..BLACK };
        let grey = half.over(WHITE);
        assert!((grey.r - 127.5).abs() < 1e-9);
    }
}
