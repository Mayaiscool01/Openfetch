# Changelog

## 0.4.0

- First public release.
- Solid block logos for every built-in distro (Debian, Ubuntu, openSUSE and
  Tux join Arch and Fedora), drawn with two coloured pixels per cell.
- `--noip` / `hide_ip`: hide IP addresses before sharing a screenshot.

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
