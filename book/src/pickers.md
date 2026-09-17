## Using pickers

fefix has a variety of pickers, which are interactive windows used to select various kinds of items. These include a file picker, global search picker, and more. Most pickers are accessed via keybindings in [space mode](./keymap.md#space-mode). Pickers have their own [keymap](./keymap.md#picker) for navigation.

### Filtering Picker Results

Most pickers perform fuzzy matching using [fzf syntax](https://github.com/junegunn/fzf?tab=readme-ov-file#search-syntax). Two exceptions are the global search picker, which uses regex, and the workspace symbol picker, which passes search terms to the language server. Note that OR operations (`|`) are not currently supported.

If a picker shows multiple columns, you may apply the filter to a specific column by prefixing the column name with `%`. Column names can be shortened to any prefix, so `%p`, `%pa` or `%pat` all mean the same as `%path`. For example, a query of `fefix %p .toml !lang` in the global search picker searches for the term "fefix" within files with paths ending in ".toml" but not including "lang".

You can insert the contents of a [register](./registers.md) using `Ctrl-r` followed by a register name. For example, one could insert the currently selected text using `Ctrl-r`-`.`, or the directory of the current file using `Ctrl-r`-`%` followed by `Ctrl-w` to remove the last path section. The global search picker will use the contents of the [search register](./registers.md#default-registers) if you press `Enter` without typing a filter. For example, pressing `*`-`Space-/`-`Enter` will start a global search for the currently selected text.

### File explorer

`Space-e` opens an interactive file explorer for browsing and opening files, rooted at the workspace; `Space-.` opens one rooted at the current buffer's directory. Unlike the file picker, the explorer does not ignore most files by default; its ignore behaviour is configured separately in the [`[editor.file-explorer]`](./editor.md#editorfile-explorer-section) section. Set `[editor.file-tree] enable = true` to make the explorer commands open the persistent file tree window (see below) instead of the modal picker.

### File tree

With `[editor.file-tree] enable = true`, `Space-e` toggles a VS Code / Zed style file tree window docked to the left of the editor rooted at the workspace (open it, or close it again), and `Space-.` opens (or re-roots) one at the current buffer's directory. The `file_tree` command line and other `file_tree*` commands toggle or open the window when bound. Unlike the file explorer, the whole directory hierarchy is shown at once and directories expand and collapse in place. The tree is a persistent window, not a modal overlay: `Enter` on a file opens it and moves focus to the editor while the window stays open, `Esc` moves focus from the tree back to the editor, and clicking in either pane focuses it (`q` closes the tree while it is focused, or use the `close_file_tree` command). The tree is also part of Helm's window navigation: `C-w w` (or `Space-w w`) rotates it into the window cycle, `C-w h` (or `Space-w h`) jumps to it as the leftmost window, and `C-w l` / `C-w w` from the tree hand focus back to the editor. `Right`/`l` expands a directory (or enters its first child if already expanded), `Left`/`h` collapses it or moves to its parent, `Enter` on a directory toggles it, `r` renames the selected file or directory via an inline bar at the top (`Enter` confirms, `Esc` cancels), `R` refreshes the selected directory, and `d` (or `Delete`) deletes it (recursively — the tree root cannot be deleted). Deletion needs a second `d`/`Delete` press to confirm; any other key or moving the selection cancels it. The tree tracks the disk: expanded directories are re-read automatically while the tree is visible (and again when the filter opens), so files another process creates or removes show up without a manual `R`. `?` (Shift+`/`) shows a which-key style popup listing every binding while the tree is focused — the same `Info` popup, in the same position, as the `space` leader key shows while waiting for the next key; any key pressed then closes it and runs that binding. `/` starts a filter: a search bar appears at the top of the tree and the visible entries narrow to paths containing the typed query (case-insensitive, matched across the whole tree rather than just expanded directories); `Esc` clears the filter and restores the tree. Opening the tree reveals the current buffer if it is under the root. Directories and files get VS Code / Zed style icons when the terminal is detected as Nerd Font capable (folders show an open/closed glyph, or a special glyph for well-known folder names like `src`/`assets`, and files a type-specific glyph); see the `icons` and `folder-icons` options in the [`[editor.file-tree]`](./editor.md#editorfile-tree-section) section. The window's content width starts at the `width` option (default 30 columns) and can be resized live by dragging the separator between the tree and the editor with the mouse. Expanded/collapsed directories are remembered across tree sessions, and ignore behaviour is configured there too.

#### Batch deletion

While the tree is focused, `Space` toggles a file or directory in the selection
buffer; marked entries have a `*` prefix. `Alt-Space` clears all marks. Marks
survive navigation, collapsing directories, filtering, and refresh, and follow
renames made in the tree. Closing or re-rooting the tree discards the buffer.
While editing a filter, `Space` is query text; press `Esc` before marking.

With marks present, `d` or `Delete` targets the marked paths, including ones
currently hidden by a collapsed directory or filter, rather than the cursor
entry. The status line lists the targets. Press `d` or `Delete` again to confirm
permanent deletion; any other key cancels confirmation without clearing marks.
Directories are deleted recursively, and marking both a directory and its
descendants deletes that directory only once. The root cannot be marked or
deleted. Successful deletions clear their marks; failures are reported and stay
marked. With no marks, deletion still acts on the cursor entry.

#### Batch move

`m` opens a destination bar; pressing `Enter` moves the marked entries there
(or the cursor entry when nothing is marked); `Esc` cancels. When the cursor
is on a directory the bar is prefilled with it, so `m` then `Enter` moves the
entries inside that directory; otherwise it is prefilled with the cursor
entry's parent, and typing a path and pressing `Enter` moves into it. A
relative destination is resolved
against the tree root, so `m`, then a bare directory name like `sub`, moves the
marked items into `<root>/sub`. Moved marks follow their entries, and moved
directories carry their loaded descendants. The root cannot be moved into itself
or into one of the items being moved. Failed moves stay in place and marked.
