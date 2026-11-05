<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/readme/logo-dark.svg">
    <img alt="winrice" src=".github/readme/logo-light.svg" width="380">
  </picture>
</p>

<h3 align="center">Dotfiles for your Windows look</h3>

<p align="center">
  winrice saves your wallpaper, theme, accent, taskbar, cursors, fonts and Windows Terminal<br>
  into a small folder you can put on GitHub, and applies anyone's setup with one command.
</p>

<p align="center">
  <a href="https://github.com/ayokiru/winrice/actions/workflows/ci.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/ayokiru/winrice/ci.yml?branch=main&style=flat-square&label=build&labelColor=1a1b26&color=9ece6a"></a>
  <a href="https://github.com/ayokiru/winrice/releases"><img alt="Release" src="https://img.shields.io/github/v/release/ayokiru/winrice?style=flat-square&label=release&labelColor=1a1b26&color=7aa2f7"></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/windows-10%20%7C%2011-bb9af7?style=flat-square&labelColor=1a1b26">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-ff7aa8?style=flat-square&labelColor=1a1b26"></a>
</p>

<br>

<p align="center">
  <img alt="winrice apply downloads a setup from GitHub, shows every change, then applies it" src=".github/readme/demo.svg" width="860">
</p>

<br>

> [!NOTE]
> winrice is in early development and has rough edges. It backs up your settings before every apply, so `winrice rollback` can restore your previous look.

## Install

```powershell
winget install ayokiru.winrice     # available with the first release
```

Or download `winrice.exe` from [Releases](https://github.com/ayokiru/winrice/releases). It is a single file under 2 MB with no dependencies and needs no admin rights.

## Quick start

```powershell
winrice capture my-setup        # save your current look into ./my-setup
winrice apply user/repo         # try someone else's setup from GitHub
winrice rollback                # changed your mind? get your old desktop back
```

## Share your setup

```powershell
winrice capture tokyo-night --author "you" --description "Dark, minimal, left taskbar"
```

Push the `tokyo-night` folder to a public GitHub repository. Anyone can now run:

```powershell
winrice apply you/tokyo-night
```

| You want to | Run |
| --- | --- |
| Apply a setup from a repository root | `winrice apply you/tokyo-night` |
| Pick one of several setups in a repo | `winrice apply you/setups/tokyo-night` |
| Pin a tag or branch | `winrice apply you/tokyo-night@v1` |
| Paste a GitHub link | `winrice apply https://github.com/you/setups/tree/main/tokyo-night` |
| See the changes without applying | `winrice diff you/tokyo-night` |

## What it changes

| Module | Covers |
| --- | --- |
| `theme` | Dark or light mode for apps and system, transparency, accent on taskbar and title bars |
| `accent` | Accent color with its full palette, or accent picked from the wallpaper |
| `taskbar` | Alignment, search box, Task View button |
| `explorer` | File extensions, hidden files, compact view, start folder |
| `wallpaper` | Wallpaper image and fit style |
| `cursors` | Cursor scheme, custom cursor files, cursor size |
| `fonts` | Fonts the setup needs, installed per user without admin rights |
| `terminal` | Windows Terminal theme, color scheme, font, opacity, background image |

Run `winrice modules` to see the list your version supports.

## Safety and backups

winrice lists every change and asks before it applies anything. Before each apply it backs up the settings it is about to change, and `winrice rollback` restores them.

A setup is a TOML file plus images, cursors and fonts. It cannot run commands, and only the modules listed above can change your system.

Applied wallpapers and cursors are copied into `%LOCALAPPDATA%\winrice` and fonts into your user fonts folder, so you can delete the setup folder afterwards. winrice also keeps a copy of Windows Terminal's `settings.json` before rewriting it.

<details>
<summary>The setup format</summary>
<br>

A setup is a folder with `setup.toml` and an `assets/` directory. Capture one to get a complete example, or write it by hand:

```toml
[meta]
name = "tokyo-night"
author = "you"
description = "Dark, minimal, left taskbar"
winrice = "0.1.0"

[theme]
dark_apps = true
dark_system = true
transparency = true

[accent]
color = "#7AA2F7"     # a full captured palette is used as is when present

[taskbar]
alignment = "left"
search = "icon"

[wallpaper]
file = "wallpaper.jpg"
style = "fill"

[cursors]
size = 48
arrow = "cursors/arrow.cur"       # shipped with the setup
text = "system:beam_r.cur"        # from C:\Windows\Cursors
busy = "default"

[terminal]
theme = "dark"

[terminal.defaults]
colorScheme = "Tokyo Night"
opacity = 85
font = { face = "JetBrains Mono", size = 11 }

[[terminal.schemes]]
name = "Tokyo Night"
background = "#1A1B26"
foreground = "#C0CAF5"
```

Every section is optional. Keys a module does not know are skipped with a warning.

</details>

<details>
<summary>Build from source</summary>
<br>

```powershell
git clone https://github.com/ayokiru/winrice
cd winrice
cargo build --release
```

Tests run on any OS because modules reach Windows through a thin layer that the tests replace with a mock.

```powershell
cargo test
```

</details>

## Roadmap

- [x] Theme, accent, taskbar, Explorer, wallpaper, cursors, fonts, Windows Terminal
- [x] Apply setups straight from GitHub
- [ ] Apps via winget
- [ ] A gallery of community setups

Ideas and bug reports are welcome in [Issues](https://github.com/ayokiru/winrice/issues).

## License

[MIT](LICENSE)
