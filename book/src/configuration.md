# Configuration

To override global configuration parameters, create a `config.toml` file located in your config directory:

- Linux and Mac: `~/.config/fefix/config.toml`
- Windows: `%AppData%\fefix\config.toml`

> 💡 You can easily open the config file by typing `:config-open` within fefix normal mode.

Example config:

```toml
theme = "onedark"

[editor]
line-number = "relative"
mouse = false

[editor.cursor-shape]
insert = "bar"
normal = "block"
select = "underline"

[editor.file-picker]
hidden = false
```

You can use a custom configuration file by specifying it with the `-c` or
`--config` command line argument, for example `ffx -c path/to/custom-config.toml`.
You can reload the config file by issuing the `:config-reload` command. Alternatively, on Unix operating systems, you can reload it by sending the USR1
signal to the fefix process, such as by using the command `pkill -USR1 ffx`.

Finally, you can have a `config.toml` and a `languages.toml` local to a project by putting it under a `.fefix` directory in your repository.
Its settings will be merged with the configuration directory and the built-in configuration.

## Additional properties over upstream Helix

fefix extends the upstream Helix `[editor]` configuration with two properties
that do not exist in upstream. Every other key matches upstream Helix exactly.

### `[editor] auto-reload`

`auto-reload` (`bool`, default `true`) — Reload buffers from disk
automatically when their file changes externally. Buffers with unsaved changes
are never touched and are left open, so your edits are never silently lost.

```toml
[editor]
auto-reload = false # disable automatic reload on external file change
```

This is an fefix-only key (commit `d212cd03`). Upstream Helix has no
equivalent; it requires a manual reload.

### `[editor.file-explorer] mode` and `[editor.file-tree]`

The file tree is a **style of the file explorer**, not a separate feature.
Upstream Helix ships only the modal explorer; fefix adds a `mode` field to
`[editor.file-explorer]` plus an `[editor.file-tree]` section that the tree
style consumes.

```toml
[editor.file-explorer]
mode = "picker"   # "picker" (default) | "tree"
```

- `mode = "picker"` — the modal picker overlay listing one directory at a
  time; pressing enter on a directory descends into it. This matches upstream
  Helix behavior.
- `mode = "tree"` — the persistent file-tree window docked to the left of the
  editor, expanding directories in place. It reads the `[editor.file-tree]`
  settings below (ignore behaviour, width, icons) and can be resized by
  dragging the separator with the mouse.

`[editor.file-tree]` — configuration for the tree style. Defaults shown:

```toml
[editor.file-tree]
hidden = true           # hide hidden files (dotfiles)
follow-symlinks = false # follow directory symlinks
parents = true          # read upsearch .ignore / .gitignore from parent dirs
ignore = true           # read .ignore files
git-ignore = true       # read .gitignore files
git-global = true       # read the global gitignore (core.excludesFile)
git-exclude = true      # read .git/info/exclude
width = 30              # preferred content width in columns
icons = "auto"          # "auto" | "nerdfont" | "ascii"
# Override Nerd Font glyphs for well-known folder names (e.g. "src"):
folder-icons = {}
```

- `width` — Preferred width of the file-tree content in columns (the separator
  column excluded). Can be adjusted per session by dragging the separator with
  the mouse. `0` falls back to this value for the session width. Defaults to
  `30`.
- `icons` — Whether to show VS Code / Zed style folder and file icons. Icons
  use Nerd Font glyphs and require a Nerd Font (v3 or later) in the terminal.
  `"auto"` (the default) enables icons when the terminal is likely to support
  Nerd Fonts, `"nerdfont"` always renders them, and `"ascii"` falls back to
  ASCII expand/collapse arrows.
- `folder-icons` — Custom Nerd Font glyphs for well-known folder names,
  overriding the built-in icons (`src`, `assets`, `scripts`, `node_modules`,
  ...). Keys are folder names and values the glyph to render for them, written
  either as the glyph itself or as a `\UXXXXXXXX` escape, e.g.
  `folder-icons = { src = "\U000F107F" }`. Folders without an entry keep their
  built-in icon (or the generic folder glyph).

