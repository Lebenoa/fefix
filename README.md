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
based on commit `079a789e`, 2026-07-23) with the following additions:

- **File tree window** — a VS Code / Zed style file tree docked to the left
  of the editor. With `[editor.file-explorer] mode = "tree"` (set `mode` to
  `"tree"`; the default `"picker"` keeps the modal picker), `<space>e`
  toggles the tree at the workspace root and `<space>.` opens or re-roots it
  at the current buffer's directory; `ffx <dir>` (e.g. `ffx .`) opens the tree
  at that directory too. It is a persistent window, not a modal overlay:
  `enter` on a file opens it and moves focus to the editor while the window
  stays open, `esc` moves focus from the tree to the editor, `q` closes it
  while focused, and clicking in either pane focuses it. Directories expand
  and collapse in place (`<right>`/`<left>` or `l`/`h`, `enter` to toggle),
  `r` refreshes the selected directory, and `/` filters the visible entries
  by path. Backed by the `file_tree`, `file_tree_in_current_buffer_directory`,
  `file_tree_in_current_directory` and `close_file_tree` commands.

  Behavior is configured in the `[editor.file-tree]` section (ignore handling,
  `width`, `icons`, and `folder-icons`). The content width starts at the
  `width` option (default 30 columns) and can be resized live by dragging the
  separator between the tree and the editor with the mouse. Icons use Nerd
  Font glyphs (v3 or later): `icons = "auto"` (the default) enables them when
  the terminal is detected as Nerd Font capable, `icons = "nerdfont"` always
  renders them and `icons = "ascii"` falls back to ASCII expand/collapse
  arrows. Folders get an open/closed folder glyph (or a special glyph for
  well-known folder names) and files a type-specific glyph (rust, markdown,
  git, etc.). Expanded directories are remembered across tree sessions.
- **Auto-reload** — unmodified buffers reload immediately when their file
  changes on disk, controlled by the new `[editor] auto-reload` option
  (default `true`). Reload is skipped for buffers with unsaved changes and
  while in insert mode.

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

Pre-built binaries are published on the
[GitHub Releases page](https://github.com/Lebenoa/fefix/releases). Each
archive (`fefix-<version>-<arch>-linux.tar.xz`,
`fefix-<version>-<arch>-macos.tar.xz`, or `fefix-<version>-<arch>-windows.zip`)
contains the `ffx` binary **and** a `runtime/` directory, so it is fully
self-contained — no extra setup needed.

### Linux / macOS (tar.xz)

```sh
# download and extract the archive matching your architecture
curl -LO https://github.com/Lebenoa/fefix/releases/latest/download/fefix-26.9.0-x86_64-linux.tar.xz
tar xJf fefix-26.9.0-x86_64-linux.tar.xz
cd fefix-26.9.0-x86_64-linux

# run it from anywhere (runtime/ is resolved relative to the binary)
./ffx --version
```

To use `ffx` from any directory, add it to your `$PATH` while keeping the
extracted directory intact (the binary resolves `runtime/` relative to its
real location, even through a symlink):

```sh
ln -sf "$(pwd)/ffx" ~/.local/bin/ffx   # keep runtime/ next to ffx in this dir
ffx --version
```

If you move the binary alone (without its `runtime/` dir), point `FEFIX_RUNTIME`
at the runtime dir, or copy `runtime/` to `~/.config/fefix/runtime` (see
`ffx --health` for the exact search list).

### Windows (zip)

Extract the archive and run `ffx.exe`; `runtime/` ships alongside it the same
way.

### Debian / Ubuntu (.deb)

Install the `.deb` asset directly:

```sh
sudo dpkg -i fefix-v26.9.0-x86_64-linux.deb   # if named helix_26.9.0-1_amd64.deb
```

### Building from source

See [Installation documentation](./book/src/install.md) and
[building from source](./book/src/building-from-source.md) for package
managers and source builds.

# Contributing

Contributing guidelines can be found [here](./docs/CONTRIBUTING.md).

# Getting help

Your question might already be answered on the upstream [FAQ](https://github.com/helix-editor/helix/wiki/FAQ).

Discuss the editor on the upstream community [Matrix Space](https://matrix.to/#/#helix-community:matrix.org) (join `#helix-editor:matrix.org` if your client doesn't support Matrix Spaces yet).

# Credits

Thanks to [@jakenvac](https://github.com/jakenvac) for designing the logo!
