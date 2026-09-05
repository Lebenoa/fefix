## Using pickers

fefix has a variety of pickers, which are interactive windows used to select various kinds of items. These include a file picker, global search picker, and more. Most pickers are accessed via keybindings in [space mode](./keymap.md#space-mode). Pickers have their own [keymap](./keymap.md#picker) for navigation.

### Filtering Picker Results

Most pickers perform fuzzy matching using [fzf syntax](https://github.com/junegunn/fzf?tab=readme-ov-file#search-syntax). Two exceptions are the global search picker, which uses regex, and the workspace symbol picker, which passes search terms to the language server. Note that OR operations (`|`) are not currently supported.

If a picker shows multiple columns, you may apply the filter to a specific column by prefixing the column name with `%`. Column names can be shortened to any prefix, so `%p`, `%pa` or `%pat` all mean the same as `%path`. For example, a query of `fefix %p .toml !lang` in the global search picker searches for the term "fefix" within files with paths ending in ".toml" but not including "lang".

You can insert the contents of a [register](./registers.md) using `Ctrl-r` followed by a register name. For example, one could insert the currently selected text using `Ctrl-r`-`.`, or the directory of the current file using `Ctrl-r`-`%` followed by `Ctrl-w` to remove the last path section. The global search picker will use the contents of the [search register](./registers.md#default-registers) if you press `Enter` without typing a filter. For example, pressing `*`-`Space-/`-`Enter` will start a global search for the currently selected text.

### File explorer

`Space-e` opens an interactive file explorer for browsing and opening files, rooted at the workspace; `Space-.` opens one rooted at the current buffer's directory. Unlike the file picker, the explorer does not ignore most files by default; its ignore behaviour is configured separately in the [`[editor.file-explorer]`](./editor.md#editorfile-explorer-section) section.

### File tree

`Space-t` toggles a VS Code / Zed style file tree window docked to the left of the editor, rooted at the workspace; `Space-T` opens (or re-roots) one at the current buffer's directory. Unlike the file explorer, the whole directory hierarchy is shown at once and directories expand and collapse in place. The tree is a persistent window, not a modal overlay: `Enter` on a file opens it and moves focus to the editor while the window stays open, `Esc` moves focus from the tree back to the editor, and clicking in either pane focuses it (use `Space-t` again or the `close_file_tree` command to close the window). `Right`/`l` expands a directory (or enters its first child if already expanded), `Left`/`h` collapses it or moves to its parent, `Enter` on a directory toggles it, and `r` refreshes the selected directory. Opening the tree reveals the current buffer if it is under the root. Directories and files get VS Code / Zed style icons when the terminal is detected as Nerd Font capable (folders show an open/closed glyph and files a type-specific glyph); see the `icons` option in the [`[editor.file-tree]`](./editor.md#editorfile-tree-section) section. The window's content width starts at the `width` option (default 30 columns) and can be resized live by dragging the separator between the tree and the editor with the mouse. Expanded/collapsed directories are remembered across tree sessions, and ignore behaviour is configured there too.
