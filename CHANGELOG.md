# Changelog

## Unreleased

- Pride flags: `--flag trans`, `nonbinary` and `genderfluid` (grouped as the
  trans umbrella), `--flag pride` (alias `lgbtq`), plus lesbian, bi, pan, ace
  and aro. Every distro logo (block and
  ASCII, large and compact) and custom text logos are painted in the flag's
  stripes; second-colour details keep their shape as a shade of the stripe.
  Set `flag = "..."` under `[logo]` to make it stick; `--flag none` turns it
  off for one run. `--list-flags` lists them.
- Mixed flags: `--flag trans+lesbian`, `--flag trans+gay`, ... (up to four);
  each flag paints a diagonal slice of the logo.
- `--tui` with a flag paints the dashboard too: usage bars show the flag's
  stripes, CPU / network graphs and panel titles run through its colours,
  and the info arrows go down each panel in stripe order. Turn it off with
  `flag_panels = false` under `[tui]`.

## 0.4.0

- First public release.
- Solid block logos for every built-in distro (Debian, Ubuntu, openSUSE and
  Tux join Arch and Fedora), drawn with two coloured pixels per cell.
- `--noip` / `hide_ip`: hide IP addresses before sharing a screenshot.
  (--noip may not work with --tui, untested)

## 0.3.2

- Code formatted with rustfmt and clean under clippy; CI and release workflows.

## 0.3.1

- Default panel titles are now "Software" and "Hardware".

## 0.3.0

- `--tui`: live full-screen dashboard (CPU, memory, swap, disks, network) with
  theme switching and an optional spinning logo.
- `--image`: use a PNG, JPEG or GIF as the logo; animated GIFs play.
- New `spin` animation effect.

## 0.2.x

- Panel layout with titled boxes from `[[sections]]`, arrow bullets and
  value-only rows.
- Solid half-block logos for Arch and Fedora.
- New modules: theme, icons, cursor, font, load, processes, init, locale;
  monitor name and refresh rate; shell version.

## 0.1.0

- First version: system detection, ASCII logos, themes, TOML config, JSON
  output, compact layout, rainbow / wave / pulse and custom frame animations.
