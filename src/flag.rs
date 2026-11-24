//! Pride flags. Any logo can be painted in the stripes of a flag
//! (`--flag trans`, or `flag = "pride"` under `[logo]`).
//!
//! The stripes run top to bottom across the visible part of the logo, so
//! every distro gets its own trans, rainbow, bi, ... version. Parts of a logo
//! drawn in a second colour (the "f" of Fedora, Tux's belly) keep their shape:
//! they get a lighter or darker shade of the stripe they sit on.

use crate::theme::ColorSpec;

pub type Rgb = (u8, u8, u8);

/// How `--list-flags` groups the flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Gender identities under the trans umbrella.
    TransUmbrella,
    Orientation,
    /// The whole LGBTQ+ community.
    Community,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::TransUmbrella, Group::Community, Group::Orientation];

    pub fn title(self) -> &'static str {
        match self {
            Group::TransUmbrella => "Trans umbrella",
            Group::Orientation => "Orientation",
            Group::Community => "LGBTQ+ community",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Flag {
    pub id: &'static str,
    pub display: &'static str,
    pub group: Group,
    pub aliases: &'static [&'static str],
    /// Stripes from top to bottom.
    pub stripes: &'static [Rgb],
}

pub const FLAGS: &[Flag] = &[
    Flag {
        id: "trans",
        display: "Transgender",
        group: Group::TransUmbrella,
        aliases: &["transgender", "tgirl", "tboy"],
        stripes: &[
            (91, 206, 250),
            (245, 169, 184),
            (255, 255, 255),
            (245, 169, 184),
            (91, 206, 250),
        ],
    },
    Flag {
        id: "nonbinary",
        display: "Non-binary",
        group: Group::TransUmbrella,
        aliases: &["enby", "nb"],
        // the black stripe is lifted a little so it shows on dark terminals
        stripes: &[
            (252, 244, 52),
            (255, 255, 255),
            (156, 89, 209),
            (70, 70, 70),
        ],
    },
    Flag {
        id: "genderfluid",
        display: "Genderfluid",
        group: Group::TransUmbrella,
        aliases: &["fluid"],
        stripes: &[
            (255, 118, 164),
            (255, 255, 255),
            (192, 17, 215),
            (70, 70, 70),
            (47, 60, 190),
        ],
    },
    Flag {
        id: "pride",
        display: "Rainbow / LGBTQ+",
        group: Group::Community,
        aliases: &["lgbtq", "lgbtq+", "lgbt", "rainbow", "gay"],
        stripes: &[
            (228, 3, 3),
            (255, 140, 0),
            (255, 237, 0),
            (0, 128, 38),
            (36, 64, 142),
            (115, 41, 130),
        ],
    },
    Flag {
        id: "lesbian",
        display: "Lesbian",
        group: Group::Orientation,
        aliases: &["wlw"],
        stripes: &[
            (213, 45, 0),
            (255, 154, 86),
            (255, 255, 255),
            (211, 98, 164),
            (163, 2, 98),
        ],
    },
    Flag {
        id: "bi",
        display: "Bisexual",
        group: Group::Orientation,
        aliases: &["bisexual"],
        stripes: &[
            (214, 2, 112),
            (214, 2, 112),
            (155, 79, 150),
            (0, 56, 168),
            (0, 56, 168),
        ],
    },
    Flag {
        id: "pan",
        display: "Pansexual",
        group: Group::Orientation,
        aliases: &["pansexual"],
        stripes: &[(255, 33, 140), (255, 216, 0), (33, 177, 255)],
    },
    Flag {
        id: "ace",
        display: "Asexual",
        group: Group::Orientation,
        aliases: &["asexual"],
        stripes: &[
            (70, 70, 70),
            (163, 163, 163),
            (255, 255, 255),
            (128, 0, 128),
        ],
    },
    Flag {
        id: "aro",
        display: "Aromantic",
        group: Group::Orientation,
        aliases: &["aromantic"],
        stripes: &[
            (61, 165, 66),
            (167, 211, 121),
            (255, 255, 255),
            (169, 169, 169),
            (70, 70, 70),
        ],
    },
];

/// Look a flag up by id or alias (case-insensitive).
pub fn find(name: &str) -> Option<&'static Flag> {
    let n = name.trim().to_ascii_lowercase();
    FLAGS
        .iter()
        .find(|f| f.id == n || f.aliases.contains(&n.as_str()))
}

/// One flag, or several mixed (`trans+lesbian`): each flag gets a diagonal
/// band of the logo, and inside its band the flag's own stripes run top to
/// bottom.
#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    pub flags: Vec<&'static Flag>,
}

/// Resolve a flag setting: `None`, empty, `none` or `off` mean no flag.
/// Several flags are joined with `+` (`trans+lesbian`, `trans+gay`).
pub fn resolve(name: Option<&str>) -> Result<Option<Paint>, String> {
    let Some(n) = name.map(|n| n.trim().to_ascii_lowercase()) else {
        return Ok(None);
    };
    if n.is_empty() || n == "none" || n == "off" {
        return Ok(None);
    }
    let flags = n
        .split(['+', ','])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            find(p).ok_or_else(|| {
                format!(
                    "unknown flag '{p}' (available: {}; mix them with +, e.g. trans+lesbian)",
                    FLAGS.iter().map(|f| f.id).collect::<Vec<_>>().join(", ")
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if flags.is_empty() {
        return Ok(None);
    }
    if flags.len() > 4 {
        return Err("at most 4 flags can be mixed".into());
    }
    Ok(Some(Paint { flags }))
}

/// Painted area of a logo: first/last column and row with anything in them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Flag {
    /// Index of the stripe for `row` when the painted rows run `top..=bottom`.
    pub fn stripe(&self, row: usize, top: usize, bottom: usize) -> usize {
        let n = self.stripes.len();
        let span = bottom.saturating_sub(top) + 1;
        (row.saturating_sub(top) * n / span).min(n - 1)
    }
}

impl Paint {
    #[cfg(test)]
    pub fn single(flag: &'static Flag) -> Paint {
        Paint { flags: vec![flag] }
    }

    fn stripes(&self) -> impl Iterator<Item = Rgb> + '_ {
        self.flags.iter().flat_map(|f| f.stripes.iter().copied())
    }

    /// Number of stripe colours; the shade of colour `i` is `i + count()`.
    pub fn count(&self) -> usize {
        self.flags.iter().map(|f| f.stripes.len()).sum()
    }

    /// All stripes of all flags, followed by their contrasting shades.
    pub fn palette(&self) -> Vec<ColorSpec> {
        self.stripes()
            .chain(self.stripes().map(shade))
            .map(|(r, g, b)| ColorSpec::Rgb(r, g, b))
            .collect()
    }

    /// Which flag paints `(x, y)`: diagonal bands from top-left to
    /// bottom-right, one per flag.
    pub fn band(&self, x: usize, y: usize, b: Bounds) -> usize {
        let n = self.flags.len();
        if n == 1 {
            return 0;
        }
        let w = (b.x1.saturating_sub(b.x0) + 1) as f32;
        let h = (b.y1.saturating_sub(b.y0) + 1) as f32;
        let t =
            (x.saturating_sub(b.x0) as f32 + 0.5) / w + (y.saturating_sub(b.y0) as f32 + 0.5) / h;
        ((t / 2.0 * n as f32) as usize).min(n - 1)
    }

    /// Palette index (0-based) of the stripe colour at `(x, y)`.
    pub fn index(&self, x: usize, y: usize, b: Bounds) -> usize {
        let k = self.band(x, y, b);
        let offset: usize = self.flags[..k].iter().map(|f| f.stripes.len()).sum();
        offset + self.flags[k].stripe(y, b.y0, b.y1)
    }

    /// Colour of palette index `i` (stripes, then shades).
    pub fn rgb(&self, i: usize) -> Rgb {
        let n = self.count();
        let c = self.stripes().nth(i % n).unwrap_or((255, 255, 255));
        if i >= n {
            shade(c)
        } else {
            c
        }
    }
}

pub fn luma((r, g, b): Rgb) -> f32 {
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// A shade of `c` that stands out against it: darker on light stripes,
/// lighter on dark ones.
pub fn shade(c: Rgb) -> Rgb {
    if luma(c) > 0.55 {
        mix(c, (0, 0, 0), 0.45)
    } else {
        mix(c, (255, 255, 255), 0.55)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_by_id_and_alias() {
        assert_eq!(find("trans").unwrap().id, "trans");
        assert_eq!(find("LGBTQ").unwrap().id, "pride");
        assert_eq!(find("rainbow").unwrap().id, "pride");
        assert_eq!(find("enby").unwrap().id, "nonbinary");
        assert!(find("nope").is_none());
    }

    #[test]
    fn resolve_handles_off() {
        assert!(resolve(None).unwrap().is_none());
        assert!(resolve(Some("none")).unwrap().is_none());
        assert!(resolve(Some("")).unwrap().is_none());
        let one = resolve(Some("trans")).unwrap().unwrap();
        assert_eq!(one.flags.len(), 1);
        assert_eq!(one.flags[0].id, "trans");
        assert!(resolve(Some("sparkly")).is_err());
        assert!(resolve(Some("trans+sparkly")).is_err());
        assert!(resolve(Some("a+b+c+d+e")).is_err());
    }

    #[test]
    fn nonbinary_and_genderfluid_are_under_the_trans_umbrella() {
        for id in ["trans", "nonbinary", "genderfluid"] {
            assert_eq!(find(id).unwrap().group, Group::TransUmbrella, "{id}");
        }
        for f in FLAGS {
            assert!(Group::ALL.contains(&f.group));
        }
    }

    #[test]
    fn mixes() {
        let p = resolve(Some("Trans + Lesbian")).unwrap().unwrap();
        let ids: Vec<_> = p.flags.iter().map(|f| f.id).collect();
        assert_eq!(ids, ["trans", "lesbian"]);
        assert_eq!(p.count(), 10);
        assert_eq!(p.palette().len(), 20);
        let gay = resolve(Some("trans+gay")).unwrap().unwrap();
        assert_eq!(gay.flags[1].id, "pride");
        // top-left is the first flag, bottom-right the second
        let b = Bounds {
            x0: 0,
            y0: 0,
            x1: 39,
            y1: 39,
        };
        assert_eq!(p.band(0, 0, b), 0);
        assert_eq!(p.band(39, 39, b), 1);
        assert_eq!(p.band(39, 0, b), p.band(0, 39, b));
        assert_eq!(p.rgb(p.index(0, 0, b)), (91, 206, 250));
        assert_eq!(p.rgb(p.index(39, 39, b)), (163, 2, 98));
        assert_eq!(p.rgb(p.index(39, 39, b) + p.count()), shade((163, 2, 98)));
    }

    #[test]
    fn stripes_cover_the_whole_height_in_order() {
        let f = find("trans").unwrap();
        let rows: Vec<usize> = (0..20).map(|y| f.stripe(y, 0, 19)).collect();
        assert_eq!(rows[0], 0);
        assert_eq!(rows[19], 4);
        assert!(rows.windows(2).all(|w| w[0] <= w[1]));
        for i in 0..5 {
            assert!(rows.contains(&i));
        }
        // rows outside the painted range clamp to the first / last stripe
        assert_eq!(f.stripe(0, 5, 9), 0);
        assert_eq!(f.stripe(50, 5, 9), 4);
    }

    #[test]
    fn shades_contrast() {
        for f in FLAGS {
            assert!(f.stripes.len() >= 3 && f.stripes.len() <= 8, "{}", f.id);
            assert_eq!(Paint::single(f).palette().len(), f.stripes.len() * 2);
            for &c in f.stripes {
                assert!((luma(c) - luma(shade(c))).abs() > 0.15, "{} {c:?}", f.id);
            }
        }
    }
}
