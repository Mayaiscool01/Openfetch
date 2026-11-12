//! ASCII logos. Logos are plain text with colour markers: `$1`..`$9` switch to
//! the n-th palette colour for the text that follows, `$$` is a literal `$`.

use crate::bitmap::{Bitmap, TimedBitmap};
use crate::config::{expand_tilde, LogoConfig};
use crate::os::OsRelease;
use crate::output::{Line, Span};
use crate::theme::ColorSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    /// 1-based palette slot.
    pub slot: u8,
}

/// A logo split into cells so that effects can recolour every character.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parsed {
    pub rows: Vec<Vec<Cell>>,
}

impl Parsed {
    pub fn parse(text: &str) -> Parsed {
        let rows = text
            .trim_matches('\n')
            .lines()
            .map(|line| {
                let mut slot = 1u8;
                let mut cells = Vec::new();
                let mut chars = line.trim_end().chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '$' {
                        match chars.peek() {
                            Some('$') => {
                                chars.next();
                                cells.push(Cell { ch: '$', slot });
                            }
                            Some(d) if ('1'..='9').contains(d) => {
                                slot = d.to_digit(10).unwrap_or(1) as u8;
                                chars.next();
                            }
                            _ => cells.push(Cell { ch: '$', slot }),
                        }
                    } else {
                        cells.push(Cell { ch: c, slot });
                    }
                }
                cells
            })
            .collect();
        Parsed { rows }
    }

    pub fn width(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    /// Colour for a slot; `None` when the palette is empty.
    pub fn slot_color(palette: &[ColorSpec], slot: u8) -> Option<ColorSpec> {
        (!palette.is_empty()).then(|| palette[(slot as usize - 1) % palette.len()])
    }

    /// Render with palette colours, merging neighbouring cells of one colour.
    pub fn render(&self, palette: &[ColorSpec]) -> Vec<Line> {
        self.render_with(|_, _, cell| Self::slot_color(palette, cell.slot))
    }

    /// Render with a per-cell colour function `(x, y, cell)`.
    pub fn render_with(
        &self,
        color: impl Fn(usize, usize, &Cell) -> Option<ColorSpec>,
    ) -> Vec<Line> {
        self.rows
            .iter()
            .enumerate()
            .map(|(y, row)| {
                let mut line: Line = Vec::new();
                for (x, cell) in row.iter().enumerate() {
                    let c = if cell.ch == ' ' {
                        None
                    } else {
                        color(x, y, cell)
                    };
                    match line.last_mut() {
                        Some(last) if last.color == c && last.bg.is_none() && !last.bold => {
                            last.text.push(cell.ch)
                        }
                        _ => line.push(Span {
                            text: cell.ch.to_string(),
                            color: c,
                            bg: None,
                            bold: false,
                        }),
                    }
                }
                line
            })
            .collect()
    }
}

/// What is drawn as the logo: text cells, or pixel images (PNG/JPEG/GIF).
#[derive(Debug, Clone)]
pub enum Art {
    Cells(Parsed),
    Images(Vec<TimedBitmap>),
}

impl Art {
    pub fn width(&self) -> usize {
        match self {
            Art::Cells(p) => p.width(),
            Art::Images(f) => f.first().map_or(0, |(b, _)| b.w),
        }
    }

    /// One-colour version for output without colours, so that shapes made of
    /// several colours still read correctly.
    pub fn mono(self) -> Art {
        match self {
            Art::Images(f) => Art::Images(
                f.into_iter()
                    .map(|(b, d)| (b.map(|_, _, _| (255, 255, 255)), d))
                    .collect(),
            ),
            cells => cells,
        }
    }

    /// The still version (first frame).
    pub fn lines(&self, palette: &[ColorSpec]) -> Vec<Line> {
        match self {
            Art::Cells(p) => p.render(palette),
            Art::Images(f) => f.first().map(|(b, _)| b.render()).unwrap_or_default(),
        }
    }
}

pub struct Builtin {
    pub id: &'static str,
    pub display: &'static str,
    pub aliases: &'static [&'static str],
    pub colors: &'static [(u8, u8, u8)],
    pub large: &'static str,
    pub small: &'static str,
    /// Solid versions as pixel grids: one character per pixel (`.` = empty,
    /// `1`-`9` = palette slot), two text rows per terminal row.
    pub large_block: &'static str,
    pub small_block: &'static str,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "fedora",
        display: "Fedora",
        aliases: &["rhel", "centos", "rocky", "almalinux", "nobara", "bazzite"],
        colors: &[(60, 110, 180), (240, 240, 240)],
        large: r"
        $1.-======-.
     $1.-'  $2.----.$1   '-.
   $1.'    $2/  .-.\$1      '.
  $1/     $2|  /  _/$1        \
 $1/   $2.--'  '-. |__$1       \
 $1|   $2'--.  .--'|__|$1      |
 $1|      $2|  |  ___ $1       |
 $1|      $2|  | |_  |$1      |
 $1|   $2.-'   '-. _||$1      |
  $1\  $2'-.____.-'$1        /
   $1'.                 .'
     $1'-.__       __.-'
         $1'-----'
",
        small: r"
   $1.-====-.
  $1/  $2.-.$1__/\
 $1|  $2-| |-$1   |
 $1|   $2| |$1    |
  $1\  $2'-'$1   /
   $1'-.__.-'
",
        large_block: r"
..................1111..................
..............111111111111..............
...........111111111111111111...........
.........1111111111111111111111.........
........1111111111....1111111111........
.......1111111............1111111.......
......111111................111111......
.....111111..................111111.....
....11111...............22.....11111....
...11111.............22222222...11111...
...11111............2222222222..11111...
..11111............222222222222..11111..
..1111............222222..22222...1111..
..1111............22222....2222...1111..
.1111.............2222......2222...1111.
.1111.............2222......2222...1111.
.1111.............2222.............1111.
.1111.............2222.............1111.
1111..............2222..............1111
1111..........2222222222222.........1111
1111..........2222222222222.........1111
1111..........2222222222222.........1111
.1111.........2222222222222........1111.
.1111........2222222222............1111.
.1111.......2222222222.............1111.
.1111......2222.222222.............1111.
..1111.....22....22222............1111..
..1111....222....22222............1111..
..11111...222....22222...........11111..
...11111...222..222222..........11111...
...11111...22222222222..........11111...
....11111...2222222222.........11111....
.....111111.......2222.......111111.....
......111111................111111......
.......1111111............1111111.......
........1111111111....1111111111........
.........1111111111111111111111.........
...........111111111111111111...........
..............111111111111..............
..................1111..................
",
        small_block: r"
........1111........
.....1111111111.....
....1111....1111....
...111........111...
..11.......222..11..
.111......22222.111.
.11......222.222.11.
.11......22...22.11.
11.......22.......11
11.......222......11
11.....2222222....11
11.....222222.....11
.11...22222......11.
.11..22..22......11.
.111.22.222.....111.
..11..22222.....11..
...111...22...111...
....1111....1111....
.....1111111111.....
........1111........
",
    },
    Builtin {
        id: "arch",
        display: "Arch Linux",
        aliases: &[
            "manjaro",
            "endeavouros",
            "cachyos",
            "garuda",
            "artix",
            "arcolinux",
        ],
        colors: &[(23, 147, 209), (200, 230, 245)],
        large: r"
              $1/\
             $1/  \
            $1/    \
           $1/      \
          $1/   ,,   \
         $1/   |  |   \
        $1/   _$2'  '$1_   \
       $1/   ,$2'    '$1,   \
      $1/  ,$2'        '$1,  \
     $1/ ,$2'            '$1, \
    $1/,$2'                '$1,\
   $1/$2'                    '$1\
",
        small: r"
      $1/\
     $1/  \
    $1/ ,, \
   $1/ $2/  \$1 \
  $1/$2.'    '.$1\
",
        large_block: r"
...................11...................
...................11...................
..................1111..................
..................1111..................
..................1111..................
.................111111.................
.................111111.................
................11111111................
................11111111................
...............1111111111...............
...............1111111111...............
..............111111111111..............
..............111111111111..............
.............11111111111111.............
.............11111111111111.............
.............111111111111111............
..............11111111111111............
................1111111111111...........
...........111.....1111111111...........
..........1111111...1111111111..........
..........11111111111111111111..........
.........1111111111111111.11111.........
.........11111111......11..1111.........
........11111111........11..1111........
........1111111..........1...111........
.......11111111..........11..1111.......
.......11111111..........111..111.......
......111111111..........1111..111......
.....1111111111..........1111...111.....
.....1111111111..........11111..111.....
....11111111111..........111111.1111....
....1111111111............1111111111....
...1111111111..............1111111111...
...111111111................111111111...
..111111111..................111111111..
.11111111......................11111111.
.1111111........................1111111.
111111............................111111
",
        small_block: r"
.........11.........
.........11.........
........1111........
........1111........
.......111111.......
.......111111.......
......11111111......
........111111......
.....111..11111.....
.....1111111111.....
....1111....1.11....
....1111....1.11....
...1111......1.11...
...1111......11.1...
..11111......11111..
.11111........11111.
.1111..........1111.
111..............111
",
    },
    Builtin {
        id: "debian",
        display: "Debian",
        aliases: &["raspbian", "devuan", "kali", "mx", "pureos"],
        colors: &[(215, 10, 83), (240, 240, 240)],
        large: r"
          ___---___
       .-'  _____  '-.
     .'   ,'  ___ '.  '.
    /    /  .'   '.  \  \
   |    |  |  .-.  |  |  |
   |    |  | (   ) |  |  |
   |    |   \ '-'  /   |  |
    \    \   '.___.'   /  /
     '.   '.          .'  /
       '-.  '-.__   .-'  .'
          '-._  ' '  _.-'
              '-.__.-'
",
        small: r"
    ,---._
  /'  __  \
 |   /    |
 |   \'.  /
  \    '-'
   '-.
      '-.
",
        large_block: r"
.................1111...................
.............111111111111...............
...........1111111111111111.............
..........1111111111111111111...........
........1111111111111111111111..........
.......111111111111111111111111.........
......111111111111...11111111111........
......111111111.........111111111.......
.....11111111.............1111111.......
....11111111...............1111111......
....1111111................1111111......
....111111..................1111111.....
...1111111...................111111.....
...111111....................111111.....
...111111.......1............111111.....
...11111.......11............111111.....
...11111......111............111111.....
...11111......11.............111111.....
...11111.....111.............111111.....
...11111.....111.............111111.....
...11111.....1111...........1111111.....
....1111......111..........1111111......
....1111......1111........11111111......
....11111.....1111111111111111111.......
....11111......11111111111111111........
.....1111.......111111111111111.........
.....1111........111111111111...........
......1111..........1111111.............
......11111.............................
.......1111.............................
........1111............................
.........1111...........................
..........1111..........................
...........11111........................
.............1111.......................
...............11111....................
.................11111111...............
.......................111..............
",
        small_block: r"
.......11111........
.....111111111......
....11111111111.....
...11111...11111....
..1111.......1111...
..111.........111...
..111.........111...
.111....1......111..
.111...1.......111..
.111...1......111...
..11...1......111...
..11...111..11111...
..11....11111111....
...11....11111......
...11...............
....11..............
.....11.............
.......11...........
..........111.......
....................
",
    },
    Builtin {
        id: "ubuntu",
        display: "Ubuntu",
        aliases: &["linuxmint", "mint", "pop", "elementary", "zorin", "neon"],
        colors: &[(233, 84, 32), (240, 240, 240)],
        large: r"
             $1.-/+oossssoo+/-.
         :+ssssssssssssssssss+:
       -+ssssssssssssssssssyyssss+-
     .ossssssssssssssssss$2dMMMNy$1sssso.
    /sssssssssss$2hdmmNNmmyNMMMMh$1ssssss/
   +sssssssss$2hm$1yd$2MMMMMMMNddddy$1ssssssss+
  /ssssssss$2hNMMM$1yh$2hyyyyhmNMMMNh$1ssssssss/
 .ssssssss$2dMMMNh$1ssssssssss$2hNMMMd$1ssssssss.
 +ssss$2hhhyNMMNy$1ssssssssssss$2yNMMMy$1sssssss+
 oss$2yNMMMNyMMh$1ssssssssssssss$2hmmmh$1ssssssso
 oss$2yNMMMNyMMh$1ssssssssssssss$2hmmmh$1ssssssso
 +ssss$2hhhyNMMNy$1ssssssssssss$2yNMMMy$1sssssss+
 .ssssssss$2dMMMNh$1ssssssssss$2hNMMMd$1ssssssss.
  /ssssssss$2hNMMM$1yh$2hyyyyhdNMMMNh$1ssssssss/
   +sssssssss$2dm$1yd$2MMMMMMMMddddy$1ssssssss+
    /sssssssssss$2hdmNNNNmyNMMMMh$1ssssss/
     .ossssssssssssssssss$2dMMMNy$1sssso.
       -+sssssssssssssssss$2yyy$1ssss+-
         :+ssssssssssssssssss+:
             .-/+oossssoo+/-.
",
        small: r"
      $1.-.
  $2.-'$1'-.$2'-.
 $2(  $1o   $2  )
  $2'-.$1.-'$2.-'
   $1'-'
",
        large_block: r"
..................1111..................
..............111111111111..............
...........111111111111111111...........
.........1111111111111111111111.........
........111111111111111111111111........
.......11111111111111111122221111.......
......1111111111111111112222221111......
.....111111111111111111122222211111.....
....11111111111111222211222222111111....
...1111111111112222222211222211111111...
...1111111111111222222221112111111111...
..111111111122112222222221111111111111..
..111111111222111222222222222111111111..
..111111112222211222222222222211111111..
.11111111122222211111122222222111111111.
.11111111222222211111111222222211111111.
.11111111222222111111111122222211111111.
.11122211222222111111111122222211111111.
1112222211222211111111111122222211111111
1112222221222211111111111111111111111111
1112222221222211111111111111111111111111
1112222211222211111111111122222211111111
.11122211222222111111111122222211111111.
.11111111222222111111111122222211111111.
.11111111222222211111111222222211111111.
.11111111122222211111122222222111111111.
..111111112222211222222222222211111111..
..111111111222111222222222222111111111..
..111111111122112222222221111111111111..
...1111111111111222222221112111111111...
...1111111111112222222211222211111111...
....11111111111111222211222222111111....
.....111111111111111111122222211111.....
......1111111111111111112222221111......
.......11111111111111111122221111.......
........111111111111111111111111........
.........1111111111111111111111.........
...........111111111111111111...........
..............111111111111..............
..................1111..................
",
        small_block: r"
........1111........
.....1111111111.....
....111111111211....
...11111111122211...
..1111111111222111..
.111111122211211111.
.111112112221111111.
.111122211222221111.
11221222111122211111
12222122111111111111
12222122111111111111
11221222111122211111
.111122211222221111.
.111112112221111111.
.111111122211211111.
..1111111111222111..
...11111111122211...
....111111111211....
.....1111111111.....
........1111........
",
    },
    Builtin {
        id: "opensuse",
        display: "openSUSE",
        aliases: &[
            "suse",
            "sles",
            "opensuse-leap",
            "opensuse-tumbleweed",
            "opensuse-slowroll",
        ],
        colors: &[(115, 186, 37), (30, 120, 60)],
        large: r"
          $1_______
       .-'       '-.
     .'   $2.---.$1      '.
    /    $2/ .-. \$1      \
   /    $2| (   ) |$1  __  \
  |      $2\ '-' /$1  /  '. |
  |       $2'---'$1  | $2o$1   ||
  |     $2.-----.$1   \__.' |
   \   $2/ .---. \$1       /
    \  $2'-'   '-'$1     /
     '.            .'
       '-._____.-'
",
        small: r"
    $1.-----.
  .' $2.--.$1  '.
 /  $2(  o)$1   \
 |   $2'--'$1 _  |
  \   $2____$1/  /
   '-.____.'
",
        large_block: r"
..........................1111..........
........................111111..........
.......................1111111..........
.......................1111122211.......
.......................111122222111.....
......................11111211122111....
.....................111111211122111....
.................11111111112222211111...
.............1111111111111112221111111..
...........111111111111111111111111111..
..........1111111111111111111111........
....111111111111111111111111111111111...
...111111111111111111111111111111.......
...111..11111111111111111111111.........
..111....111111111111111111111..........
..111....11111111111111111111...........
..111..111111111111111111111............
..111..111.111111111111111..............
...1111..11111111.......111.............
...1111111111.111.......111.............
....11111111..11........111.............
.......111....11.........11.............
.............111.........111............
.............111111......111111.........
................111.........111.........
........................................
",
        small_block: r"
............111.....
............1111....
...........1122211..
..........11122211..
......1111111111111.
...1111111111111111.
.111111111111111....
.1..11111111111.....
.1111111111111......
..111111....1.......
...111.1....11......
.......11....11.....
",
    },
    Builtin {
        id: "linux",
        display: "Generic Linux (Tux)",
        aliases: &["tux", "gnu", "generic"],
        colors: &[(240, 240, 240), (250, 200, 40), (120, 120, 130)],
        large: r"
          $3.-------.
         /  $1.---.$3  \
        |  $1| $3o$1 $3o$1 |$3  |
        |  $1'-$2<>$1-'$3  |
        |   $1/    \$3   |
       /   $1/      \$3   \
      /   $1|        |$3   \
     |   $1/|        |\$3   |
     |  $1/ |        | \$3  |
     '-'$1/  \      /  \$3'-'
        $1\___/'.____.'\___/
       $2(___)        (___)
",
        small: r"
    $3.---.
   /$1.-.-.$3\
   |$1|$3o$1 $3o$1|$3|
   |$1'$2<>$1'$3|
  /$1/     \$3\
 (_$1\     /$3_)
   $2(_)(_)
",
        large_block: r"
........................................
..................3333..................
................33333333................
...............3333333333...............
..............333333333333..............
..............331133331133..............
.............33111133111133.............
.............33133133133133.............
.............33111133111133.............
.............33311333311333.............
.............33332222223333.............
.............33322222222333.............
.............33332222223333.............
............3333111111113333............
...........333331111111133333...........
.........3333333111111113333333.........
........333333331111111133333333........
........333333311111111113333333........
.......33333333111111111133333333.......
.......33333331111111111113333333.......
......3333333311111111111133333333......
......3333333311111111111133333333......
......3333333111111111111113333333......
.....333333331111111111111133333333.....
.....333333331111111111111133333333.....
.....333333331111111111111133333333.....
.....3333.33311111111111111333.3333.....
.....333..33331111111111113333..333.....
.....333...333111111111111333...333.....
...........333111111111111333...........
............3331111111111333............
.............33111111111133.............
..........22222211111111222222..........
........222222222211112222222222........
.......22222222222233222222222222.......
.......222222222222..222222222222.......
.........222222222....222222222.........
........................................
",
        small_block: r"
........3333........
.......333333.......
.......333333.......
......33133133......
.......322223.......
......33122133......
.....3331111333.....
....333311113333....
...33331111113333...
...33331111113333...
...33331111113333...
..3333311111133333..
..33.3311111133.33..
.....3311111133.....
......33111133......
....222221122222....
....222222222222....
.....222....222.....
",
    },
];

/// Look up a built-in logo by id or alias.
pub fn builtin(name: &str) -> Option<&'static Builtin> {
    let n = name.trim().to_ascii_lowercase();
    BUILTINS
        .iter()
        .find(|b| b.id == n || b.aliases.contains(&n.as_str()))
}

/// Pick the built-in logo matching the running distribution.
pub fn detect(os: Option<&OsRelease>) -> &'static Builtin {
    let generic = &BUILTINS[BUILTINS.len() - 1];
    let Some(os) = os else { return generic };
    std::iter::once(&os.id)
        .chain(os.id_like.iter())
        .find_map(|id| builtin(id))
        .unwrap_or(generic)
}

pub struct Selected {
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: String,
    pub large: Parsed,
    pub small: Parsed,
    /// Solid versions (empty `Parsed` when the logo has none).
    pub large_block: Grid,
    pub small_block: Grid,
    /// Palette of the distro (or a neutral one for custom logos).
    pub palette: Vec<ColorSpec>,
}

fn rgb_palette(c: &[(u8, u8, u8)]) -> Vec<ColorSpec> {
    c.iter().map(|&(r, g, b)| ColorSpec::Rgb(r, g, b)).collect()
}

/// Choose the logo: custom file/lines from the config, else `override_name`,
/// else the configured name, else auto-detection.
pub fn select(
    cfg: &LogoConfig,
    override_name: Option<&str>,
    os: Option<&OsRelease>,
) -> Result<Selected, String> {
    let custom = if let Some(f) = &cfg.file {
        let path = expand_tilde(f);
        Some(
            std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read logo file {}: {e}", path.display()))?,
        )
    } else {
        cfg.lines.as_ref().map(|l| l.join("\n"))
    };
    if let (Some(text), None) = (custom, override_name) {
        let p = Parsed::parse(&text);
        if p.height() == 0 {
            return Err("custom logo is empty".into());
        }
        return Ok(Selected {
            name: "custom".into(),
            large: p.clone(),
            small: p,
            large_block: Grid::new(),
            small_block: Grid::new(),
            palette: vec![
                ColorSpec::Ansi(14),
                ColorSpec::Ansi(12),
                ColorSpec::Ansi(15),
                ColorSpec::Ansi(7),
            ],
        });
    }
    let wanted = override_name.unwrap_or(&cfg.name);
    let b = if wanted.eq_ignore_ascii_case("auto") {
        detect(os)
    } else {
        builtin(wanted).ok_or_else(|| {
            format!(
                "unknown logo '{wanted}' (available: {})",
                BUILTINS.iter().map(|b| b.id).collect::<Vec<_>>().join(", ")
            )
        })?
    };
    Ok(Selected {
        name: b.id.to_string(),
        large: Parsed::parse(b.large),
        small: Parsed::parse(b.small),
        large_block: parse_grid(b.large_block),
        small_block: parse_grid(b.small_block),
        palette: rgb_palette(b.colors),
    })
}

/// Pixel rows of a block logo; 0 = empty, otherwise a 1-based palette slot.
pub type Grid = Vec<Vec<u8>>;

pub fn parse_grid(text: &str) -> Grid {
    text.trim_matches('\n')
        .lines()
        .map(|l| {
            l.chars()
                .map(|c| c.to_digit(10).unwrap_or(0) as u8)
                .collect()
        })
        .filter(|row: &Vec<u8>| !row.is_empty())
        .collect()
}

impl Selected {
    /// Pick what to draw. The solid block version is used when allowed
    /// (`style`: `auto` = only with Unicode, `block`, `ascii`) and available;
    /// otherwise the ASCII art.
    pub fn art(&self, compact: bool, unicode: bool, style: &str, palette: &[ColorSpec]) -> Art {
        let (block, ascii) = if compact {
            (&self.small_block, &self.small)
        } else {
            (&self.large_block, &self.large)
        };
        let want_block = match style {
            "block" => true,
            "ascii" => false,
            _ => unicode,
        };
        if want_block && !block.is_empty() {
            Art::Images(vec![(Bitmap::from_slots(block, palette), 0.1)])
        } else if ascii.height() > 0 {
            Art::Cells(ascii.clone())
        } else {
            Art::Cells(self.large.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markers_and_escapes() {
        let p = Parsed::parse("$1ab$2cd$$e\n");
        let row = &p.rows[0];
        assert_eq!(row.iter().map(|c| c.ch).collect::<String>(), "abcd$e");
        assert_eq!(row[0].slot, 1);
        assert_eq!(row[2].slot, 2);
        assert_eq!(row[4].slot, 2);
    }

    #[test]
    fn slot_resets_each_line_and_lone_dollar_is_literal() {
        let p = Parsed::parse("$3x\ny$ z");
        assert_eq!(p.rows[1][0].slot, 1);
        assert_eq!(p.rows[1].iter().map(|c| c.ch).collect::<String>(), "y$ z");
    }

    #[test]
    fn render_merges_runs_and_skips_colour_on_spaces() {
        let p = Parsed::parse("$1ab $2cd");
        let pal = [ColorSpec::Ansi(1), ColorSpec::Ansi(2)];
        let lines = p.render(&pal);
        let spans = &lines[0];
        assert_eq!(spans[0].text, "ab");
        assert_eq!(spans[0].color, Some(ColorSpec::Ansi(1)));
        assert_eq!(spans[1].text, " ");
        assert_eq!(spans[1].color, None);
        assert_eq!(spans[2].text, "cd");
    }

    #[test]
    fn empty_palette_renders_plain() {
        let lines = Parsed::parse("$1ab").render(&[]);
        assert_eq!(lines[0][0].color, None);
    }

    #[test]
    fn every_builtin_logo_is_ascii_and_reasonably_sized() {
        for b in BUILTINS {
            for (kind, art) in [("large", b.large), ("small", b.small)] {
                let p = Parsed::parse(art);
                assert!(p.height() >= 4, "{} {kind} too short", b.id);
                assert!(p.width() > 0 && p.width() <= 44, "{} {kind} width", b.id);
                assert!(art.is_ascii(), "{} {kind} must be ASCII", b.id);
            }
            assert!(!b.colors.is_empty());
        }
    }

    #[test]
    fn every_builtin_has_block_logos_of_sane_size() {
        for b in BUILTINS {
            let (large, small) = (parse_grid(b.large_block), parse_grid(b.small_block));
            for (kind, g) in [("large", &large), ("small", &small)] {
                assert!(g.len() >= 8 && g.len() % 2 == 0, "{} {kind} rows", b.id);
                let w = g[0].len();
                assert!(w > 0 && w <= 44, "{} {kind} width", b.id);
                assert!(
                    g.iter().all(|r| r.len() == w),
                    "{} {kind} rectangular",
                    b.id
                );
                let max = g.iter().flatten().copied().max().unwrap();
                assert!(
                    max >= 1 && max as usize <= b.colors.len(),
                    "{} {kind} slots",
                    b.id
                );
            }
            assert!(small.len() < large.len(), "{}", b.id);
        }
    }

    #[test]
    fn grid_parsing() {
        assert_eq!(
            parse_grid("\n.12\n3..\n"),
            vec![vec![0, 1, 2], vec![3, 0, 0]]
        );
        assert!(parse_grid("").is_empty());
    }

    #[test]
    fn art_selection() {
        let pal = [
            ColorSpec::Rgb(1, 2, 3),
            ColorSpec::Rgb(4, 5, 6),
            ColorSpec::Rgb(7, 8, 9),
        ];
        let is_block = |a: &Art| matches!(a, Art::Images(_));
        for b in BUILTINS {
            let sel = select(&LogoConfig::default(), Some(b.id), None).unwrap();
            assert!(is_block(&sel.art(false, true, "auto", &pal)), "{}", b.id);
            assert!(!is_block(&sel.art(false, false, "auto", &pal)), "{}", b.id);
            assert!(!is_block(&sel.art(false, true, "ascii", &pal)), "{}", b.id);
            assert!(is_block(&sel.art(false, false, "block", &pal)), "{}", b.id);
            let large = sel.art(false, true, "auto", &pal);
            let small = sel.art(true, true, "auto", &pal);
            assert!(
                small.lines(&pal).len() < large.lines(&pal).len(),
                "{}",
                b.id
            );
        }
        // custom text logos have no block version and stay as they are
        let custom = LogoConfig {
            lines: Some(vec!["$1 /\\".into(), "$1/__\\".into()]),
            ..Default::default()
        };
        let sel = select(&custom, None, None).unwrap();
        assert!(!is_block(&sel.art(false, true, "block", &pal)));
    }

    #[test]
    fn block_art_uses_the_palette_and_mono_flattens_it() {
        let pal = [ColorSpec::Rgb(10, 20, 30), ColorSpec::Rgb(200, 210, 220)];
        let sel = select(&LogoConfig::default(), Some("fedora"), None).unwrap();
        let art = sel.art(false, true, "auto", &pal);
        let colours = |a: &Art| -> std::collections::HashSet<ColorSpec> {
            a.lines(&pal)
                .iter()
                .flatten()
                .flat_map(|s| [s.color, s.bg])
                .flatten()
                .collect()
        };
        let used = colours(&art);
        assert!(used.contains(&pal[0]) && used.contains(&pal[1]));
        assert_eq!(colours(&art.mono()).len(), 1);
    }

    #[test]
    fn small_logos_are_smaller() {
        for b in BUILTINS {
            assert!(
                Parsed::parse(b.small).height() < Parsed::parse(b.large).height(),
                "{}",
                b.id
            );
        }
    }

    #[test]
    fn detects_by_id_and_id_like() {
        let os = |id: &str, like: &[&str]| OsRelease {
            id: id.into(),
            id_like: like.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        assert_eq!(detect(Some(&os("fedora", &[]))).id, "fedora");
        assert_eq!(detect(Some(&os("manjaro", &["arch"]))).id, "arch");
        assert_eq!(
            detect(Some(&os("linuxmint", &["ubuntu", "debian"]))).id,
            "ubuntu"
        );
        assert_eq!(
            detect(Some(&os("rocky", &["rhel", "centos", "fedora"]))).id,
            "fedora"
        );
        assert_eq!(
            detect(Some(&os("opensuse-tumbleweed", &["opensuse", "suse"]))).id,
            "opensuse"
        );
        assert_eq!(detect(Some(&os("gentoo", &[]))).id, "linux");
        assert_eq!(detect(None).id, "linux");
    }

    #[test]
    fn selection_rules() {
        let cfg = LogoConfig::default();
        assert_eq!(select(&cfg, Some("arch"), None).unwrap().name, "arch");
        assert!(select(&cfg, Some("nope"), None).is_err());
        let custom = LogoConfig {
            lines: Some(vec!["$1 /\\".into(), "$1/__\\".into()]),
            ..Default::default()
        };
        assert_eq!(select(&custom, None, None).unwrap().name, "custom");
        // an explicit --distro beats a custom logo
        assert_eq!(
            select(&custom, Some("debian"), None).unwrap().name,
            "debian"
        );
        let missing = LogoConfig {
            file: Some("/definitely/not/here.txt".into()),
            ..Default::default()
        };
        assert!(select(&missing, None, None).is_err());
    }
}
