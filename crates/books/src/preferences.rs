//! Preferences: per-user choices that belong to the Library, not to a Book. Stored
//! as one JSON object: `default_color` references the Default Highlight Color, a
//! built-in color by its name, a Custom Color by its ID, which is never a color
//! name; `custom_colors` maps each Custom Color's ID to its color (sRGB as
//! `0xRRGGBB`). An ID starts with the time it was added, so sorting IDs gives
//! the order added.

use crate::markup::Rgb;
use serde_json::{Value, json};

/// One of the Highlight Colors that always exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInColor {
    pub name: &'static str,
    pub rgb: Rgb,
}

const fn built_in(name: &'static str, r: u8, g: u8, b: u8) -> BuiltInColor {
    BuiltInColor {
        name,
        rgb: Rgb { r, g, b },
    }
}

/// The built-in Highlight Colors, yellow (the default) first.
pub const BUILT_IN_COLORS: [BuiltInColor; 6] = [
    built_in("Yellow", 255, 235, 60),
    built_in("Green", 126, 217, 87),
    built_in("Blue", 102, 178, 255),
    built_in("Pink", 255, 128, 191),
    built_in("Purple", 178, 140, 255),
    built_in("Red", 255, 99, 99),
];

impl BuiltInColor {
    /// The built-in color called `name`, if there is one.
    pub fn named(name: &str) -> Option<Self> {
        BUILT_IN_COLORS.into_iter().find(|c| c.name == name)
    }

    /// Yellow, the Default Highlight Color until the user picks another.
    pub const YELLOW: Self = BUILT_IN_COLORS[0];
}

/// A color to highlight in: a built-in one, or a Custom Color of the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightColor {
    /// A built-in color's name, or a Custom Color's ID.
    pub id: String,
    pub rgb: Rgb,
    pub is_custom: bool,
}

impl From<BuiltInColor> for HighlightColor {
    fn from(color: BuiltInColor) -> Self {
        Self {
            id: color.name.to_owned(),
            rgb: color.rgb,
            is_custom: false,
        }
    }
}

/// The choices stored in the Preferences.
#[derive(Debug, Default)]
pub(crate) struct Preferences {
    default_color: Option<String>,
    /// Custom Colors by ID, which sorts them in the order added.
    custom: Vec<(String, Rgb)>,
}

impl Preferences {
    /// The Preferences stored as `bytes`; the defaults if they are missing or unreadable.
    pub(crate) fn decode(bytes: Option<&[u8]>) -> Self {
        let json: Value = bytes
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or_default();
        let mut custom: Vec<_> = json
            .get("custom_colors")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(id, rgb)| Some((id.clone(), Rgb::unpacked(rgb.as_u64()?)?)))
            .collect();
        custom.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            default_color: json
                .get("default_color")
                .and_then(Value::as_str)
                .map(str::to_owned),
            custom,
        }
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let custom: serde_json::Map<_, _> = self
            .custom
            .iter()
            .map(|(id, rgb)| (id.clone(), json!(rgb.packed())))
            .collect();
        json!({ "default_color": self.default_color, "custom_colors": custom })
            .to_string()
            .into_bytes()
    }

    /// The Highlight Colors to choose from and the one new Highlights get. The
    /// default is yellow if none is stored, or it references a color that
    /// doesn't exist (any more).
    pub(crate) fn highlight_colors(&self) -> HighlightColors {
        let custom = self.custom.iter().map(|(id, rgb)| HighlightColor {
            id: id.clone(),
            rgb: *rgb,
            is_custom: true,
        });
        let colors: Vec<HighlightColor> = BUILT_IN_COLORS
            .into_iter()
            .map(HighlightColor::from)
            .chain(custom)
            .collect();
        let default_color = self
            .default_color
            .as_ref()
            .and_then(|id| colors.iter().find(|c| &c.id == id))
            .cloned()
            .unwrap_or_else(|| BuiltInColor::YELLOW.into());
        HighlightColors {
            colors,
            default_color,
        }
    }

    /// Makes the color `id` the Default Highlight Color. It doesn't check that
    /// `id` is a color.
    pub(crate) fn set_default_color(&mut self, id: &str) {
        self.default_color = Some(id.to_owned());
    }

    /// Adds a new Custom Color `rgb`, added at `now` in milliseconds since the
    /// Unix epoch, and returns it. It comes after every Custom Color already
    /// there, even if clocks disagree.
    pub(crate) fn add_custom_color(&mut self, rgb: Rgb, now: u64) -> HighlightColor {
        let after = self.custom.last().and_then(|(id, _)| added_at(id));
        let time = after.map_or(now, |last| now.max(last + 1));
        let id = format!("{time:013x}-{}", crate::state::new_id());
        self.custom.push((id.clone(), rgb));
        HighlightColor {
            id,
            rgb,
            is_custom: true,
        }
    }

    /// Removes Custom Color `id`. Built-in colors and unknown IDs are left alone.
    pub(crate) fn remove_custom_color(&mut self, id: &str) {
        self.custom.retain(|(custom, _)| custom != id);
    }
}

/// When the Custom Color `id` was added, as it was given to `add_custom_color`.
fn added_at(id: &str) -> Option<u64> {
    u64::from_str_radix(id.split_once('-')?.0, 16).ok()
}

/// The Highlight Colors to choose from and the one new Highlights get.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightColors {
    /// The built-in colors, then the Custom Colors in the order added.
    pub colors: Vec<HighlightColor>,
    /// The Default Highlight Color, one of `colors`.
    pub default_color: HighlightColor,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEAL: Rgb = Rgb {
        r: 0,
        g: 128,
        b: 128,
    };
    const BROWN: Rgb = Rgb {
        r: 150,
        g: 75,
        b: 0,
    };

    fn built_in(name: &str) -> HighlightColor {
        BuiltInColor::named(name).unwrap().into()
    }

    fn reloaded(prefs: &Preferences) -> Preferences {
        Preferences::decode(Some(&prefs.encode()))
    }

    #[test]
    fn the_built_in_colors_start_with_yellow_and_are_distinct() {
        assert_eq!(BUILT_IN_COLORS.len(), 6);
        assert_eq!(BUILT_IN_COLORS[0].name, "Yellow");
        for (i, a) in BUILT_IN_COLORS.iter().enumerate() {
            assert!(BUILT_IN_COLORS[i + 1..].iter().all(|b| a.rgb != b.rgb));
        }
    }

    #[test]
    fn a_stored_default_color_reads_back() {
        let mut prefs = Preferences::default();
        prefs.set_default_color("Green");
        assert_eq!(
            reloaded(&prefs).highlight_colors().default_color,
            built_in("Green")
        );
    }

    #[test]
    fn a_reference_to_a_color_that_does_not_exist_reads_as_yellow() {
        let mut prefs = Preferences::default();
        prefs.set_default_color("Chartreuse");
        assert_eq!(prefs.highlight_colors().default_color, built_in("Yellow"));
    }

    #[test]
    fn missing_or_unreadable_preferences_have_only_the_built_in_colors_and_yellow() {
        for bytes in [None, Some(&b"garbage"[..]), Some(&b"[1]"[..])] {
            let colors = Preferences::decode(bytes).highlight_colors();
            assert_eq!(colors.colors.len(), 6);
            assert_eq!(colors.default_color, built_in("Yellow"));
        }
    }

    #[test]
    fn colors_are_the_built_in_ones_then_the_custom_ones_in_the_order_added() {
        let mut prefs = Preferences::default();
        let teal = prefs.add_custom_color(TEAL, 1_000);
        let brown = prefs.add_custom_color(BROWN, 2_000);
        let colors = reloaded(&prefs).highlight_colors().colors;
        assert_eq!(colors[..6], BUILT_IN_COLORS.map(HighlightColor::from));
        assert_eq!(colors[6..], [teal, brown]);
    }

    #[test]
    fn colors_added_within_one_millisecond_or_with_a_slow_clock_keep_the_order_added() {
        let mut prefs = Preferences::default();
        let first = prefs.add_custom_color(TEAL, 5_000);
        let second = prefs.add_custom_color(BROWN, 5_000);
        let third = prefs.add_custom_color(TEAL, 1);
        assert_eq!(prefs.highlight_colors().colors[6..], [first, second, third]);
    }

    #[test]
    fn a_removed_custom_color_is_gone_and_the_others_stay() {
        let mut prefs = Preferences::default();
        let teal = prefs.add_custom_color(TEAL, 1);
        let brown = prefs.add_custom_color(BROWN, 2);
        prefs.remove_custom_color(&teal.id);
        assert_eq!(reloaded(&prefs).highlight_colors().colors[6..], [brown]);
    }

    #[test]
    fn built_in_colors_and_unknown_ids_cannot_be_removed() {
        let mut prefs = Preferences::default();
        prefs.remove_custom_color("Yellow");
        prefs.remove_custom_color("nope");
        assert_eq!(prefs.highlight_colors().colors.len(), 6);
    }

    #[test]
    fn a_custom_color_can_be_the_default_until_it_is_removed() {
        let mut prefs = Preferences::default();
        let teal = prefs.add_custom_color(TEAL, 1);
        prefs.set_default_color(&teal.id);
        assert_eq!(reloaded(&prefs).highlight_colors().default_color, teal);
        prefs.remove_custom_color(&teal.id);
        assert_eq!(prefs.highlight_colors().default_color, built_in("Yellow"));
    }
}
