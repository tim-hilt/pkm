//! CSS colour strings as `tinycolor2` 1.6.0 reads them (the library
//! Excalidraw validates colour input with, `normalizeInputColor`), read into
//! `0xRRGGBBAA`. The rules are tinycolor's, quirks included: input is trimmed
//! and lower-cased; one of the 148 CSS names, `transparent`, hex with or
//! without `#` (3, 4, 6 or 8 digits), or `rgb`/`rgba`/`hsl`/`hsla`/`hsv`/`hsva`
//! followed by whitespace, `(` or `|`, and numbers separated by commas,
//! whitespace or `|`. Numbers may be percentages. The function name is found
//! anywhere in the string and trailing text is ignored, as in tinycolor.

/// Excalidraw's default ink, `#1e1e1e`, drawn for strings tinycolor rejects.
const DEFAULT: u32 = 0x1e1e_1eff;

/// `0xRRGGBBAA` for a CSS colour; the default ink for anything tinycolor
/// rejects. (A canvas would keep its previous colour instead.)
pub(crate) fn parse_color(css: &str) -> u32 {
    read(css).unwrap_or(DEFAULT)
}

fn read(css: &str) -> Option<u32> {
    // What Excalidraw writes (`#ffc9c9`) needs neither trimming nor
    // lower-casing; this is asked for every Element drawn or measured.
    if let Some(digits) = css.strip_prefix('#')
        && let Some(colour) = hex(digits)
    {
        return Some(colour);
    }
    let css = css.trim().to_lowercase();
    if css == "transparent" {
        return Some(0);
    }
    if let Ok(i) = NAMES.binary_search_by_key(&css.as_str(), |&(name, _)| name) {
        return Some(NAMES[i].1 << 8 | 0xff);
    }
    let call = |keyword: &str, count: usize| {
        css.match_indices(keyword)
            .find_map(|(at, _)| units(&css[at + keyword.len()..], count))
    };
    if let Some(u) = call("rgb", 3) {
        return Some(pack(rgb(&u), 1.0));
    }
    if let Some(u) = call("rgba", 4) {
        return Some(pack(rgb(&u), alpha(&u[3])));
    }
    if let Some(u) = call("hsl", 3) {
        return Some(pack(hsl(&u), 1.0));
    }
    if let Some(u) = call("hsla", 4) {
        return Some(pack(hsl(&u), alpha(&u[3])));
    }
    if let Some(u) = call("hsv", 3) {
        return Some(pack(hsv(&u), 1.0));
    }
    if let Some(u) = call("hsva", 4) {
        return Some(pack(hsv(&u), alpha(&u[3])));
    }
    hex(css.strip_prefix('#').unwrap_or(&css))
}

fn hex(digits: &str) -> Option<u32> {
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(match digits.len() {
        6 => value << 8 | 0xff,
        8 => value,
        // Each digit stands for itself twice: #f80 is #ff8800.
        3 | 4 => {
            let doubled = digits.chars().fold(0, |acc, c| {
                (acc << 8) | (c.to_digit(16).unwrap_or_default() * 0x11)
            });
            if digits.len() == 3 {
                doubled << 8 | 0xff
            } else {
                doubled
            }
        }
        _ => return None,
    })
}

/// A number as written: `value` without its `%`.
#[derive(Clone, Copy)]
struct Unit {
    value: f64,
    percent: bool,
    /// Written like `1.0`: tinycolor reads that as 100%, not as the number 1.
    one_point_zero: bool,
}

/// Reads exactly `count` units: `[\s|(]+ unit ([,|\s]+ unit)*`.
fn units(text: &str, count: usize) -> Option<Vec<Unit>> {
    let mut rest = text.trim_start_matches(|c: char| c.is_whitespace() || c == '|' || c == '(');
    if rest.len() == text.len() {
        return None;
    }
    let mut found = Vec::with_capacity(count);
    for n in 0..count {
        if n > 0 {
            let after =
                rest.trim_start_matches(|c: char| c.is_whitespace() || c == '|' || c == ',');
            if after.len() == rest.len() {
                return None;
            }
            rest = after;
        }
        let (unit, after) = unit(rest)?;
        found.push(unit);
        rest = after;
    }
    Some(found)
}

/// `[-+]?(\d*\.\d+|\d+)%?`
fn unit(text: &str) -> Option<(Unit, &str)> {
    let bytes = text.as_bytes();
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let sign = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let whole = digits(sign);
    let mut end = sign + whole;
    let mut dotted = false;
    if bytes.get(end) == Some(&b'.') {
        let fraction = digits(end + 1);
        if fraction > 0 {
            end += 1 + fraction;
            dotted = true;
        }
    }
    if whole == 0 && !dotted {
        return None;
    }
    let value: f64 = text[..end].parse().ok()?;
    let percent = bytes.get(end) == Some(&b'%');
    let unit = Unit {
        value,
        percent,
        one_point_zero: dotted && value == 1.0,
    };
    Some((unit, &text[end + usize::from(percent)..]))
}

/// tinycolor's `bound01`: the unit as a fraction of `max`, in `[0, 1]`.
fn bound01(unit: Unit, max: f64) -> f64 {
    let (mut n, percent) = if unit.one_point_zero {
        (100.0, true)
    } else {
        (unit.value, unit.percent)
    };
    n = n.max(0.0).min(max);
    if percent {
        n = (n * max).trunc() / 100.0;
    }
    if (n - max).abs() < 0.000_001 {
        return 1.0;
    }
    n % max / max
}

/// tinycolor's `convertToPercentage`: plain numbers up to 1 are fractions.
fn fraction_to_percent(unit: Unit) -> Unit {
    if !unit.percent && unit.value <= 1.0 {
        Unit {
            value: unit.value * 100.0,
            percent: true,
            one_point_zero: false,
        }
    } else {
        unit
    }
}

/// Unrounded channels in `[0, 255]`.
type Rgb = [f64; 3];

fn rgb(u: &[Unit]) -> Rgb {
    [0, 1, 2].map(|i| bound01(u[i], 255.0) * 255.0)
}

fn hsl(u: &[Unit]) -> Rgb {
    let h = bound01(u[0], 360.0);
    let s = bound01(fraction_to_percent(u[1]), 100.0);
    let l = bound01(fraction_to_percent(u[2]), 100.0);
    let channel = |t: f64| {
        let t = if t < 0.0 {
            t + 1.0
        } else if t > 1.0 {
            t - 1.0
        } else {
            t
        };
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    if s == 0.0 {
        return [l * 255.0; 3];
    }
    [h + 1.0 / 3.0, h, h - 1.0 / 3.0].map(|t| channel(t) * 255.0)
}

fn hsv(u: &[Unit]) -> Rgb {
    let h = bound01(u[0], 360.0) * 6.0;
    let s = bound01(fraction_to_percent(u[1]), 100.0);
    let v = bound01(fraction_to_percent(u[2]), 100.0);
    let sector = h.floor();
    let f = h - sector;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match sector as i64 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r * 255.0, g * 255.0, b * 255.0]
}

/// tinycolor's `boundAlpha`: anything outside `[0, 1]` is opaque.
fn alpha(unit: &Unit) -> f64 {
    if (0.0..=1.0).contains(&unit.value) {
        unit.value
    } else {
        1.0
    }
}

/// Rounds like JavaScript's `Math.round` (halves go up).
fn byte(x: f64) -> u32 {
    (x.clamp(0.0, 255.0) + 0.5).floor() as u32
}

fn pack(rgb: Rgb, alpha: f64) -> u32 {
    let [r, g, b] = rgb.map(byte);
    r << 24 | g << 16 | b << 8 | byte(alpha * 255.0)
}

/// The CSS colour names tinycolor knows, sorted, as `0xRRGGBB`.
const NAMES: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("burntsienna", 0xea7e5d),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;

    const INVALID: u32 = DEFAULT;

    /// Expected values are `tinycolor(input).toHex8()` from tinycolor2 1.6.0.
    #[test]
    fn colours_read_like_tinycolor() {
        let cases: &[(&str, u32)] = &[
            ("red", 0xff0000ff),
            ("RebeccaPurple", 0x663399ff),
            ("  Red  ", 0xff0000ff),
            ("rgb(255, 0, 0)", 0xff0000ff),
            ("rgb(255 0 0)", 0xff0000ff),
            ("rgb 255 0 0", 0xff0000ff),
            ("RGB(10,20,30)", 0x0a141eff),
            ("rgba(255,0,0,0.5)", 0xff000080),
            ("rgba(0,128,255,.25)", 0x0080ff40),
            ("rgb(100%, 50%, 0%)", 0xff8000ff),
            ("rgb(150%,0,0)", 0x800000ff),
            ("rgb(300,-5,12.6)", 0xff000dff),
            ("rgb(1.0,0,0)", 0xff0000ff),
            ("rgba(1,2,3,2)", 0x010203ff),
            ("rgba(1,2,3,50%)", 0x010203ff),
            ("hsl(120, 100%, 50%)", 0x00ff00ff),
            ("hsl(210, 0.5, 0.4)", 0x336699ff),
            ("hsl(400,50%,50%)", 0xbf4040ff),
            ("hsl(0,0%,50%)", 0x808080ff),
            ("hsla(30, 60%, 40%, 0.3)", 0xa366294d),
            ("hsv(200, 50%, 80%)", 0x66aaccff),
            ("hsva(10,100%,100%,0.8)", 0xff2a00cc),
            ("#f80", 0xff8800ff),
            ("#f808", 0xff880088),
            ("#e03131", 0xe03131ff),
            ("#e0313180", 0xe0313180),
            ("e03131", 0xe03131ff),
            ("f80", 0xff8800ff),
            ("bad", 0xbbaaddff),
            ("transparent", 0x00000000),
            ("argb(1,2,3)", 0x010203ff),
            ("rgb(1,2,3,4)", 0x010203ff),
            ("rgb(1,2,3)xyz", 0x010203ff),
            ("hsl(-30,50%,50%)", 0xbf4040ff),
            ("hsl(360,50%,50%)", 0xbf4040ff),
            ("rgb(50%,50%,50%)", 0x808080ff),
            ("rgba(10,20,30,0)", 0x0a141e00),
            ("hsl(0,5,10)", 0x1b1818ff),
        ];
        for &(input, expected) in cases {
            assert_eq!(parse_color(input), expected, "{input:?}");
        }
    }

    #[test]
    fn strings_tinycolor_rejects_draw_in_the_default_ink() {
        for input in [
            "",
            "not a colour",
            "#12345",
            "rgb(1,2)",
            "#ggg",
            "hsl(a,b,c)",
            "rgb()",
        ] {
            assert_eq!(parse_color(input), INVALID, "{input:?}");
        }
    }

    #[test]
    fn every_name_is_sorted_for_lookup() {
        assert!(NAMES.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }
}
