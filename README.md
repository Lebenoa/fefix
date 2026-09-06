<div align="center">

<h1>
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="logo_dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="logo_light.svg">
  <img alt="fefix" height="128" src="logo_light.svg">
</picture>
</h1>

[![Build status](https://github.com/Lebenoa/fefix/actions/workflows/build.yml/badge.svg)](https://github.com/Lebenoa/fefix/actions)
[![GitHub Release](https://img.shields.io/github/v/release/Lebenoa/fefix)](https://github.com/Lebenoa/fefix/releases/latest)
[![Documentation](https://shields.io/badge/-documentation-452859)](https://github.com/Lebenoa/fefix/tree/master/book#readme)
[![GitHub contributors](https://img.shields.io/github/contributors/Lebenoa/fefix)](https://github.com/Lebenoa/fefix/graphs/contributors)

</div>

![Screenshot](./screenshot.png)

A [Kakoune](https://github.com/mawww/kakoune) / [Neovim](https://github.com/neovim/neovim) inspired editor, written in Rust.

The editing model is very heavily based on Kakoune; during development I found
myself agreeing with most of Kakoune's design decisions.

For more information, see the [documentation](https://github.com/Lebenoa/fefix/tree/master/book#readme).

All shortcuts/keymaps can be found [in the book](./book/src/keymap.md).

# Differences from upstream

This fork tracks [upstream Helix](https://github.com/helix-editor/helix) (currently
based on commit `079a789e`, 2026-07-23) with the following addition:

- **File tree window** — a VS Code / Zed style file tree docked to the left
  of the editor, toggled with `<space>t` (workspace root, revealing the
  current buffer) while `<space>T` opens or re-roots it at the current
  buffer's directory. It is a persistent window, not a modal overlay:
  `enter` on a file opens it and moves focus to the editor while the window
  stays open, `esc` moves focus from the tree to the editor, and clicking in
  either pane focuses it. Directories expand and collapse lazily
  (`<right>`/`<left>` or `l`/`h`, `enter` to toggle), and `r` refreshes the
  selected directory. Backed by the `file_tree`,
  `file_tree_in_current_buffer_directory`, `file_tree_in_current_directory`
  and `close_file_tree` commands.

  Behavior is configured in the `[editor.file-tree]` section (ignore handling,
  `width`, and `icons`). The content width starts at the `width` option
  (default 30 columns) and can be resized live by dragging the separator
  between the tree and the editor with the mouse. Icons use Nerd Font glyphs (v3 or later): `icons = "auto"`
  (the default) enables them when the terminal is detected as Nerd Font
  capable, `icons = "nerdfont"` always renders them and `icons = "ascii"`
  falls back to ASCII expand/collapse arrows. Folders get an open/closed
  folder glyph and files a type-specific glyph (rust, markdown, git, etc.).
  Expanded directories are remembered across tree sessions. With `[editor.file-explorer] mode = "tree"`, `fx <directory>` (e.g. `fx .`) opens the tree docked at that directory instead of a file picker.

See the [CHANGELOG](./CHANGELOG.md) for details.

[Troubleshooting](https://github.com/helix-editor/helix/wiki/Troubleshooting)

# Features

- Vim-like modal editing
- Multiple selections
- Built-in language server support
- Smart, incremental syntax highlighting and code editing via tree-sitter

Although it's primarily a terminal-based editor, I am interested in exploring
a custom renderer (similar to Emacs) using wgpu.

Note: Only certain languages have indentation definitions at the moment. Check
`runtime/queries/<lang>/` for `indents.scm`.

# Installation

[Installation documentation](./book/src/install.md).

# Contributing

Contributing guidelines can be found [here](./docs/CONTRIBUTING.md).

# Getting help

Your question might already be answered on the upstream [FAQ](https://github.com/helix-editor/helix/wiki/FAQ).

Discuss the editor on the upstream community [Matrix Space](https://matrix.to/#/#helix-community:matrix.org) (join `#helix-editor:matrix.org` if your client doesn't support Matrix Spaces yet).

# Credits

Thanks to [@jakenvac](https://github.com/jakenvac) for designing the logo!
