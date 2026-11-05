# winrice

**Capture, share and apply your whole Windows look with one command.**

Dotfiles for Windows: save your wallpaper, theme, taskbar and Explorer settings into a small
setup folder, put it on GitHub, and let anyone apply it in seconds. Every apply is backed up,
so `winrice rollback` always brings your old desktop back.

> Early development. Not ready for daily use yet.

## Usage

```powershell
winrice capture my-setup        # save the current look into ./my-setup
winrice diff my-setup           # see what would change
winrice apply my-setup          # apply it (current state is backed up)
winrice rollback                # undo the last apply
winrice modules                 # list what winrice can handle
```

A setup is a folder with `setup.toml` and an `assets/` directory:

```toml
[meta]
name = "tokyo-night"
winrice = "0.1.0"

[theme]
dark_apps = true
dark_system = true
transparency = true

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
font = { face = "CaskaydiaCove Nerd Font", size = 11 }
```

Custom color schemes and themes used by the setup are stored next to it. Windows Terminal
`settings.json` is backed up before every change.

## Modules

| Module      | What it covers                                   |
|-------------|--------------------------------------------------|
| `theme`     | Dark/light mode, transparency, accent usage      |
| `taskbar`   | Alignment, search box, task view button          |
| `explorer`  | File extensions, hidden files, compact view      |
| `wallpaper` | Wallpaper image and fit style                    |
| `cursors`   | Cursor scheme, custom cursor files, cursor size  |
| `terminal`  | Windows Terminal theme, font, colors, background |

Planned: accent color, fonts, apps via winget, setup gallery
(`winrice apply user/setup`).

## License

MIT
