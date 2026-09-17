//! A VS Code / Zed style file tree window docked to the left of the editor.
//!
//! While open, the tree is a persistent window: the editor is laid out to its
//! right and keeps working normally, with keyboard focus living in either the
//! tree or the editor. The tree lazily loads directory contents on expansion,
//! remembers which directories are expanded across sessions and reveals
//! (expands the ancestors of) the current buffer when opened.
//!
//! `/` starts a filter: a search bar appears at the top of the tree and the
//! visible entries narrow to paths containing the typed query
//! (case-insensitive). The first typed character loads the whole tree so
//! matches are found anywhere, not just under expanded directories; `Esc`
//! clears the filter and restores the tree. `?` shows a which-key style popup
//! listing every binding while the tree is focused. The tree is part of the
//! window navigation: `C-w w` rotates it into the window cycle, `C-w h` jumps
//! to it as the leftmost window, and `C-w l` / `C-w w` return to the editor.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    error::Error,
    path::{Path, PathBuf},
};

use helix_view::{
    Editor,
    editor::{Action, FileTreeConfig},
    graphics::{CursorKind, Rect},
    info::Info,
    input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
};
use tui::buffer::Buffer as Surface;

use crate::{
    compositor::{Component, Compositor, Context, Event, EventResult},
    ctrl, key, shift,
};

/// The interval at which the expanded directories are re-read from disk
/// while the tree is visible, so externally created or removed files show up
/// without a manual `R` refresh.
const AUTO_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

pub const ID: &str = "file-tree";

const ARROW_EXPANDED: &str = "▾";
const ARROW_COLLAPSED: &str = "▸";
// Nerd Font glyphs (v3 codepoints, verified against the nerd-fonts generated
// CSS v3.5.1), the same style of icons VS Code uses. Requires a Nerd Font (v3
// or later) to render; see `IconMode`.
const ICON_FOLDER: &str = "\u{ea83}"; // nf-cod-folder
const ICON_FOLDER_OPEN: &str = "\u{eaf7}"; // nf-cod-folder_opened
const ICON_FILE: &str = "\u{ea7b}"; // nf-cod-file
const ICON_RUST: &str = "\u{e7a8}"; // nf-dev-rust
const ICON_GO: &str = "\u{e724}"; // nf-dev-go
const ICON_PYTHON: &str = "\u{e73c}"; // nf-dev-python
const ICON_JAVASCRIPT: &str = "\u{e781}"; // nf-dev-javascript
const ICON_TYPESCRIPT: &str = "\u{e8ca}"; // nf-dev-typescript
const ICON_REACT: &str = "\u{e7ba}"; // nf-dev-react
const ICON_SVELTE: &str = "\u{e8b7}"; // nf-dev-svelte
const ICON_HTML: &str = "\u{e736}"; // nf-dev-html5
const ICON_CSS: &str = "\u{e749}"; // nf-dev-css3
const ICON_MARKDOWN: &str = "\u{eeab}"; // nf-fa-markdown
const ICON_JSON: &str = "\u{e60b}"; // nf-seti-json
const ICON_CONFIG: &str = "\u{e615}"; // nf-seti-config
const ICON_SHELL: &str = "\u{f120}"; // nf-fa-terminal
const ICON_GIT: &str = "\u{e702}"; // nf-dev-git
const ICON_C: &str = "\u{e771}"; // nf-dev-c
const ICON_CPLUSPLUS: &str = "\u{e7a3}"; // nf-dev-cplusplus
const ICON_JAVA: &str = "\u{e738}"; // nf-dev-java
const ICON_PHP: &str = "\u{e73d}"; // nf-dev-php
const ICON_RUBY: &str = "\u{e739}"; // nf-dev-ruby
const ICON_LUA: &str = "\u{e620}"; // nf-seti-lua
const ICON_HASKELL: &str = "\u{e777}"; // nf-dev-haskell
const ICON_ELIXIR: &str = "\u{e7cd}"; // nf-dev-elixir
const ICON_DOCKER: &str = "\u{e7b0}"; // nf-dev-docker
const ICON_MAKEFILE: &str = "\u{e673}"; // nf-seti-makefile
const ICON_DATABASE: &str = "\u{f1c0}"; // nf-fa-database
const ICON_IMAGE: &str = "\u{f03e}"; // nf-fa-image
const ICON_VIDEO: &str = "\u{f008}"; // nf-fa-film
const ICON_LICENSE: &str = "\u{eb12}"; // nf-cod-law
/// Special folder glyphs (also Nerd Font v3.5.1), used for well-known folder
/// names so they stand out from the generic folder icon, the same style VS
/// Code's file explorer uses. Unlike the generic folder, these stay the same
/// whether the folder is expanded or collapsed.
const ICON_FOLDER_CONFIG: &str = "\u{f107f}"; // nf-md-folder_cog
const ICON_FOLDER_SRC: &str = "\u{ebdf}"; // nf-cod-folder_library
const ICON_FOLDER_SCRIPT: &str = "\u{f19fc}"; // nf-md-folder_wrench
const ICON_FOLDER_IMAGE: &str = "\u{f024f}"; // nf-md-folder_image
const ICON_FOLDER_MUSIC: &str = "\u{f1359}"; // nf-md-folder_music
const ICON_FOLDER_DOCS: &str = "\u{f0c82}"; // nf-md-folder_text
const ICON_FOLDER_TEST: &str = "\u{f197e}"; // nf-md-folder_check
const ICON_FOLDER_NODE_MODULES: &str = "\u{f0253}"; // nf-md-folder_multiple
const ICON_FOLDER_BUILD: &str = "\u{f024d}"; // nf-md-folder_download
const ICON_FOLDER_GITHUB: &str = "\u{ea84}"; // nf-cod-github
const ICON_FOLDER_ENV: &str = "\u{f08ac}"; // nf-md-folder_key
const ICON_FOLDER_DATA: &str = "\u{f12e3}"; // nf-md-folder_table
const ICON_FOLDER_CACHE: &str = "\u{f0aba}"; // nf-md-folder_clock
const ICON_FOLDER_ARCHIVE: &str = "\u{f06eb}"; // nf-md-folder_zip
/// Smallest content width (in columns) the window can be resized to with the
/// mouse.
const MIN_WIDTH: u16 = 10;
/// Number of columns of indentation per tree level.
const INDENT: usize = 2;
/// Number of rows scrolled per mouse wheel step.
const WHEEL_SCROLL: isize = 3;

/// The content width in columns the tree gets on `area`, given a preferred
/// width: never wider than the terminal allows (two columns are kept for the
/// editor and its gutter) and never wider than the preference.
fn content_width(area: Rect, preferred: u16) -> u16 {
    area.width.saturating_sub(2).min(preferred)
}

/// The preferred content width for the tree of `editor`: the session width if
/// the user has resized it with the mouse, otherwise the configured default.
fn preferred_width(editor: &Editor) -> u16 {
    let width = editor.file_tree_window.width;
    if width > 0 {
        width
    } else {
        editor.config().file_tree.width
    }
}

/// The total number of columns the file tree window occupies on `area` for
/// `editor`, including the separator column between it and the editor. The
/// editor viewport is laid out to the right of this many columns while the
/// window is open.
pub(crate) fn dock_width(editor: &Editor, area: Rect) -> u16 {
    content_width(area, preferred_width(editor)).saturating_add(1)
}

// Expanded directories per tree root, shared across file tree sessions so
// that expansion state survives closing and reopening the tree. Kept in
// memory only, not across editor restarts.
thread_local! {
    static EXPANDED_STATE: RefCell<HashMap<PathBuf, HashSet<PathBuf>>> = RefCell::new(HashMap::new());
}

/// Remember that `path` (a directory) is expanded in the tree rooted at `root`.
fn remember_expanded(root: &Path, path: &Path) {
    EXPANDED_STATE.with(|state| {
        state
            .borrow_mut()
            .entry(root.to_path_buf())
            .or_default()
            .insert(path.to_path_buf());
    });
}

/// Forget that `path` is expanded in the tree rooted at `root`.
fn forget_expanded(root: &Path, path: &Path) {
    EXPANDED_STATE.with(|state| {
        if let Some(expanded) = state.borrow_mut().get_mut(root) {
            expanded.remove(path);
        }
    });
}

/// The directories previously expanded in the tree rooted at `root` that
/// still exist on disk.
fn recalled_expanded(root: &Path) -> Vec<PathBuf> {
    EXPANDED_STATE.with(|state| {
        state
            .borrow()
            .get(root)
            .map(|expanded| {
                expanded
                    .iter()
                    .filter(|path| path.is_dir())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// A single node of the file tree.
struct TreeEntry {
    path: PathBuf,
    is_dir: bool,
    expanded: bool,
    loaded: bool,
    children: Vec<TreeEntry>,
}

impl TreeEntry {
    fn new(path: PathBuf, is_dir: bool) -> Self {
        Self {
            path,
            is_dir,
            expanded: false,
            loaded: false,
            children: Vec::new(),
        }
    }

    /// The file name of the entry, used for display.
    fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    }
}

/// The file tree data structure: a lazily loaded, expandable tree of
/// directories and files rooted at a single directory.
struct Tree {
    root: PathBuf,
    /// The root entry is always the first element.
    entries: Vec<TreeEntry>,
    /// Path of the currently selected entry.
    selected: PathBuf,
    /// Batch marks are paths, independent of cursor, expansion and filtering.
    marked: HashSet<PathBuf>,
    config: FileTreeConfig,
}

impl Tree {
    fn new(root: PathBuf, config: FileTreeConfig) -> Self {
        let mut root_entry = TreeEntry::new(root.clone(), true);
        root_entry.expanded = true;
        root_entry.loaded = true;
        root_entry.children = read_children(&root, &config);
        let mut tree = Self {
            selected: root.clone(),
            marked: HashSet::new(),
            root,
            entries: vec![root_entry],
            config,
        };
        // Restore expansion state from previous sessions with this root.
        for path in recalled_expanded(&tree.root) {
            tree.expand(&path);
        }
        tree
    }

    /// Find an entry by path.
    fn find<'a>(entries: &'a [TreeEntry], path: &Path) -> Option<&'a TreeEntry> {
        if let Some(entry) = entries.iter().find(|entry| entry.path == path) {
            return Some(entry);
        }
        entries
            .iter()
            .find_map(|entry| Self::find(&entry.children, path))
    }

    /// Find an entry by path, mutably.
    fn find_mut<'a>(entries: &'a mut [TreeEntry], path: &Path) -> Option<&'a mut TreeEntry> {
        if let Some(index) = entries.iter().position(|entry| entry.path == path) {
            return Some(&mut entries[index]);
        }
        for entry in entries.iter_mut() {
            if let Some(found) = Self::find_mut(&mut entry.children, path) {
                return Some(found);
            }
        }
        None
    }

    /// Load the children of `path` if they haven't been loaded yet and expand
    /// the directory. If `path` is not yet visible because its ancestors are
    /// collapsed, the ancestors are expanded first (used when restoring
    /// expansion state from a previous session).
    fn expand(&mut self, path: &Path) {
        if Self::find_mut(&mut self.entries, path).is_none() {
            if let Some(parent) = self.parent(path) {
                self.expand(&parent);
            }
        }
        if let Some(entry) = Self::find_mut(&mut self.entries, path) {
            if !entry.is_dir {
                return;
            }
            if !entry.loaded {
                entry.children = read_children(path, &self.config);
                entry.loaded = true;
            }
            entry.expanded = true;
            remember_expanded(&self.root, path);
        }
    }

    /// Collapse the directory at `path`.
    fn collapse(&mut self, path: &Path) {
        if let Some(entry) = Self::find_mut(&mut self.entries, path) {
            entry.expanded = false;
            forget_expanded(&self.root, path);
        }
    }

    /// Toggle the expansion state of the directory at `path`.
    fn toggle(&mut self, path: &Path) {
        let expanded =
            Self::find(&self.entries, path).is_some_and(|entry| entry.is_dir && entry.expanded);
        if expanded {
            self.collapse(path);
            // If the selection was inside the collapsed subtree, move it to the
            // collapsed directory so it stays visible.
            if self.selected != path && self.selected.starts_with(path) {
                self.selected = path.to_path_buf();
            }
        } else {
            self.expand(path);
        }
    }

    /// Reload the children of the selected directory, or of the root if the
    /// selection is a file.
    fn refresh(&mut self, path: &Path) {
        if let Some(entry) = Self::find_mut(&mut self.entries, path) {
            if entry.is_dir {
                entry.children = read_children(path, &self.config);
                entry.loaded = true;
                return;
            }
        }
        if let Some(root_entry) = self.entries.first_mut() {
            root_entry.children = read_children(&self.root, &self.config);
            root_entry.loaded = true;
        }
    }

    /// Re-read the children of every expanded directory, so files created or
    /// removed outside the editor show up without a manual refresh. Entries
    /// surviving under the same path keep their expansion state and cached
    /// subtree; new entries are added and vanished ones dropped. Returns
    /// whether the listing changed.
    fn refresh_all(&mut self) -> bool {
        fn re_read_children(directory: &mut TreeEntry, config: &FileTreeConfig) -> bool {
            let mut old: HashMap<PathBuf, TreeEntry> = directory
                .children
                .drain(..)
                .map(|entry| (entry.path.clone(), entry))
                .collect();
            let fresh = read_children(&directory.path, config);
            let mut changed = false;
            let mut merged = Vec::with_capacity(fresh.len());
            for new in fresh {
                if let Some(survivor) = old.remove(&new.path) {
                    merged.push(survivor);
                } else {
                    changed = true;
                    merged.push(new);
                }
            }
            changed |= !old.is_empty(); // deleted entries
            directory.children = merged;
            changed
        }

        fn rec(entries: &mut [TreeEntry], config: &FileTreeConfig) -> bool {
            let mut changed = false;
            for entry in entries.iter_mut() {
                if entry.is_dir && entry.expanded {
                    changed |= re_read_children(entry, config);
                }
                if entry.is_dir {
                    changed |= rec(&mut entry.children, config);
                }
            }
            changed
        }

        rec(&mut self.entries, &self.config)
    }

    /// Expand all ancestor directories of `path` (which must be under the
    /// root) and select it. Used to reveal the current buffer.
    fn reveal(&mut self, path: &Path) {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return;
        };
        let mut current = self.root.clone();
        for component in relative.components() {
            current.push(component);
            if current.is_dir() {
                self.expand(&current);
            }
        }
        self.select(path);
    }

    /// Set the selected entry.
    fn select(&mut self, path: &Path) {
        self.selected = path.to_path_buf();
    }

    /// The parent directory of `path`, if it is inside the tree.
    fn parent(&self, path: &Path) -> Option<PathBuf> {
        path.parent()
            .map(Path::to_path_buf)
            .filter(|parent| parent.starts_with(&self.root))
    }

    /// All currently visible entries, paired with their depth in the tree.
    fn visible(&self) -> Vec<(&TreeEntry, usize)> {
        let mut out = Vec::new();
        flatten(&self.entries, 0, &mut out);
        out
    }

    /// The index of the selected entry among the given visible entries, or 0
    /// if it is not visible (e.g. its ancestors got collapsed or a filter
    /// hides it).
    fn selected_index(&self, visible: &[(&TreeEntry, usize)]) -> usize {
        visible
            .iter()
            .position(|(entry, _)| entry.path == self.selected)
            .unwrap_or(0)
    }

    /// Load the children of every directory in the tree, so a filter can
    /// match files anywhere under the root, not just inside directories the
    /// user has expanded. The expansion state is left untouched; loaded
    /// children are kept as a cache afterwards.
    fn load_all(&mut self) {
        fn load_children_rec(entries: &mut [TreeEntry], config: &FileTreeConfig) {
            for entry in entries.iter_mut() {
                if entry.is_dir {
                    if !entry.loaded {
                        entry.children = read_children(&entry.path, config);
                        entry.loaded = true;
                    }
                    load_children_rec(&mut entry.children, config);
                }
            }
        }
        load_children_rec(&mut self.entries, &self.config);
    }

    /// The entries to display: the normally visible ones when `filter` is
    /// empty, otherwise every entry whose path contains `filter`
    /// (case-insensitive), searched across the whole tree regardless of
    /// expansion state (see [`Tree::load_all`]).
    fn filtered_visible(&self, filter: &str) -> Vec<(&TreeEntry, usize)> {
        if filter.is_empty() {
            return self.visible();
        }
        let query = filter.to_lowercase();
        let mut out = Vec::new();
        flatten_all(&self.entries, 0, &mut out);
        out.retain(|(entry, _)| {
            entry
                .path
                .strip_prefix(&self.root)
                .unwrap_or(&entry.path)
                .to_string_lossy()
                .to_lowercase()
                .contains(&query)
        });
        out
    }

    /// The path of the selected entry, if any.
    fn selected_path(&self) -> Option<PathBuf> {
        let entry = Self::find(&self.entries, &self.selected)?;
        Some(entry.path.clone())
    }

    /// Whether the selected entry is an expanded directory.
    fn selected_is_expanded_dir(&self) -> bool {
        Self::find(&self.entries, &self.selected)
            .is_some_and(|entry| entry.is_dir && entry.expanded)
    }

    /// Whether the selected entry is a directory.
    fn selected_is_dir(&self) -> bool {
        Self::find(&self.entries, &self.selected).is_some_and(|entry| entry.is_dir)
    }

    /// Remove the entry at `path` (and its loaded subtree) from the tree.
    /// Returns whether the entry was found. The root cannot be removed.
    fn remove(&mut self, path: &Path) -> bool {
        fn remove_rec(entries: &mut Vec<TreeEntry>, path: &Path) -> bool {
            if let Some(index) = entries.iter().position(|entry| entry.path == path) {
                entries.remove(index);
                return true;
            }
            entries
                .iter_mut()
                .any(|entry| remove_rec(&mut entry.children, path))
        }

        if path == self.root {
            return false;
        }
        remove_rec(&mut self.entries, path)
    }

    /// Rewrite the path of the entry at `old` and of every loaded descendant
    /// to start with `new` instead, after the filesystem rename happened.
    fn rename_paths(&mut self, old: &Path, new: &Path) {
        fn rec(entries: &mut [TreeEntry], old: &Path, new: &Path) {
            for entry in entries.iter_mut() {
                if entry.path.starts_with(old) {
                    let rel = entry.path.strip_prefix(old).unwrap_or(Path::new(""));
                    entry.path = new.join(rel);
                }
                rec(&mut entry.children, old, new);
            }
        }
        rec(&mut self.entries, old, new);
        self.marked = self
            .marked
            .drain()
            .map(|path| match path.strip_prefix(old) {
                Ok(relative) => new.join(relative),
                Err(_) => path,
            })
            .collect();
    }

    fn toggle_mark(&mut self) {
        if self.selected != self.root
            && Self::find(&self.entries, &self.selected).is_some()
            && !self.marked.remove(&self.selected)
        {
            self.marked.insert(self.selected.clone());
        }
    }

    /// Snapshot the batch; a marked ancestor already covers its descendants.
    fn deletion_targets(&self) -> Vec<PathBuf> {
        if self.marked.is_empty() {
            return self
                .selected_path()
                .filter(|path| path != &self.root)
                .into_iter()
                .collect();
        }
        let mut targets: Vec<_> = self
            .marked
            .iter()
            .filter(|path| {
                path.as_path() != self.root
                    && path.starts_with(&self.root)
                    && !path
                        .ancestors()
                        .skip(1)
                        .any(|ancestor| self.marked.contains(ancestor))
            })
            .cloned()
            .collect();
        targets.sort_unstable();
        targets
    }

    /// Continue after errors; only successful deletions leave the mark buffer.
    fn delete_paths(&mut self, paths: &[PathBuf]) -> (usize, Vec<String>) {
        let mut deleted = 0;
        let mut errors = Vec::new();
        for path in paths {
            if path == &self.root || !path.starts_with(&self.root) {
                errors.push(format!("Cannot delete {}", path.display()));
                continue;
            }
            let result = std::fs::symlink_metadata(path).and_then(|metadata| {
                if metadata.is_dir() {
                    std::fs::remove_dir_all(path)
                } else {
                    // Windows directory links require remove_dir, not remove_file.
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::FileTypeExt;
                        if metadata.file_type().is_symlink_dir() {
                            return std::fs::remove_dir(path);
                        }
                    }
                    std::fs::remove_file(path)
                }
            });
            match result {
                Ok(()) => {
                    deleted += 1;
                    forget_expanded(&self.root, path);
                    self.remove(path);
                    self.marked.retain(|marked| !marked.starts_with(path));
                }
                Err(err) => errors.push(format!("Failed to delete {}: {err}", path.display())),
            }
        }
        (deleted, errors)
    }

    /// Reparent the entry at `path` to the directory `dest` after the
    /// filesystem move succeeded, rewriting its path and those of its loaded
    /// descendants and remapping the marks under it. The destination directory
    /// is expanded (loading it if needed) so the moved node has a home. The
    /// destination parent is expanded through the same `expand` helper.
    fn move_into(&mut self, path: &Path, dest: &Path) {
        let Some(name) = path.file_name() else {
            return;
        };
        let new_path = dest.join(name);
        self.expand(dest);
        // Lift the node out of whatever ancestor chain holds it.
        let Some(mut node) = self.take_entry(path) else {
            return;
        };
        // Rewrite the node's path and every loaded descendant to the new
        // location.
        fn rewrite(entry: &mut TreeEntry, old: &Path, new_parent: &Path) {
            if let Ok(rel) = entry.path.strip_prefix(old) {
                entry.path = new_parent.join(rel);
            }
            for child in entry.children.iter_mut() {
                rewrite(child, old, new_parent);
            }
        }
        rewrite(&mut node, path, &new_path);
        if let Some(dest_entry) = Self::find_mut(&mut self.entries, dest) {
            dest_entry.children.push(node);
            dest_entry.loaded = true;
        }
        forget_expanded(&self.root, path);
        remember_expanded(&self.root, dest);
    }

    /// Remove the entry at `path` (and its loaded subtree) from the tree,
    /// returning it. The root cannot be removed, yielding `None`.
    fn take_entry(&mut self, path: &Path) -> Option<TreeEntry> {
        fn take_rec(entries: &mut Vec<TreeEntry>, path: &Path) -> Option<TreeEntry> {
            if let Some(index) = entries.iter().position(|entry| entry.path == path) {
                return Some(entries.remove(index));
            }
            for entry in entries.iter_mut() {
                if let Some(found) = take_rec(&mut entry.children, path) {
                    return Some(found);
                }
            }
            None
        }
        if path == self.root {
            return None;
        }
        take_rec(&mut self.entries, path)
    }

    /// Move the given entry paths into the directory `dest`. Only successful
    /// moves reparent their entry node and clear its mark; failures are
    /// reported and left in place. Returns the number moved and per-failure
    /// messages.
    fn move_paths(&mut self, paths: &[PathBuf], dest: &Path) -> (usize, Vec<String>) {
        let mut moved = 0;
        let mut errors = Vec::new();
        let mut successes = Vec::new();
        for path in paths {
            if path == &self.root
                || !path.starts_with(&self.root)
                || path == dest
                || dest.starts_with(path)
            {
                errors.push(format!("Cannot move {}", path.display()));
                continue;
            }
            let Some(name) = path.file_name() else {
                errors.push(format!("Failed to move {}: no file name", path.display()));
                continue;
            };
            let target = dest.join(name);
            if target == *path {
                continue;
            }
            match std::fs::rename(path, &target) {
                Ok(()) => {
                    moved += 1;
                    successes.push((path.clone(), target.clone()));
                }
                Err(err) => errors.push(format!("Failed to move {}: {err}", path.display())),
            }
        }
        for (old, target) in successes {
            // A mark on the moved item itself (or, for a directory, on any
            // of its moved descendants) is remapped so it still refers to the
            // entry now that it lives under `target`.
            let old_prefix = old.clone();
            self.marked = self
                .marked
                .drain()
                .map(|m| {
                    if m.starts_with(&old_prefix) {
                        target.join(m.strip_prefix(&old_prefix).unwrap_or(Path::new("")))
                    } else {
                        m
                    }
                })
                .collect();
            self.move_into(&old, dest);
        }
        (moved, errors)
    }
}

fn flatten<'a>(entries: &'a [TreeEntry], depth: usize, out: &mut Vec<(&'a TreeEntry, usize)>) {
    for entry in entries {
        out.push((entry, depth));
        if entry.is_dir && entry.expanded {
            flatten(&entry.children, depth + 1, out);
        }
    }
}

/// Like [`flatten`], but descends into every directory regardless of its
/// expansion state, so a filter can see the whole tree.
fn flatten_all<'a>(entries: &'a [TreeEntry], depth: usize, out: &mut Vec<(&'a TreeEntry, usize)>) {
    for entry in entries {
        out.push((entry, depth));
        if entry.is_dir {
            flatten_all(&entry.children, depth + 1, out);
        }
    }
}

/// List the direct children of `dir`, honoring the file tree configuration.
fn read_children(dir: &Path, config: &FileTreeConfig) -> Vec<TreeEntry> {
    use ignore::WalkBuilder;

    let mut entries: Vec<(PathBuf, bool)> = WalkBuilder::new(dir)
        .hidden(config.hidden)
        .parents(config.parents)
        .ignore(config.ignore)
        .follow_links(config.follow_symlinks)
        .git_ignore(config.git_ignore)
        .git_global(config.git_global)
        .git_exclude(config.git_exclude)
        .max_depth(Some(1))
        .sort_by_file_name(|name1, name2| name1.cmp(name2))
        .add_custom_ignore_filename(helix_loader::config_dir().join("ignore"))
        .add_custom_ignore_filename(".fefix/ignore")
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path() != dir)
        .map(|entry| {
            let path = entry.into_path();
            (path.clone(), path.is_dir())
        })
        .collect();

    // Directories first, then files, each sorted by name.
    entries.sort_by(|(path1, is_dir1), (path2, is_dir2)| {
        (!is_dir1, path1.file_name()).cmp(&(!is_dir2, path2.file_name()))
    });

    entries
        .into_iter()
        .map(|(path, is_dir)| TreeEntry::new(path, is_dir))
        .collect()
}

/// A VS Code / Zed style file tree window docked to the left of the editor.
pub struct FileTree {
    tree: Tree,
    /// First visible row, for scrolling.
    offset: usize,
    /// The area the tree content was last rendered into, used for mouse
    /// handling. Covers the tree columns only, not the separator.
    area: Rect,
    /// Whether to render Nerd Font folder/file icons instead of ASCII arrows.
    icons: bool,
    /// Whether a filter is being edited: while `true` a search bar is shown at
    /// the top of the tree and the visible entries are narrowed to the
    /// `filter` query.
    filtering: bool,
    /// Whether the `C-w` window prefix has been pressed: the next key
    /// navigates between the tree and the editor splits like \"C-w h\" does
    /// in the editor.
    window_prefix: bool,
    /// Whether the which-key style keymap modal is open (toggled with `?`
    /// while the tree is focused): the bindings overlay the tree, and any
    /// key closes them and then runs that binding.
    show_keymap: bool,
    /// The current filter query (see [`FileTree::filtering`]). Empty when no
    /// filter is active.
    filter: String,
    /// Snapshot waiting for a second `d` / `Delete`. Other input cancels it.
    pending_delete: Option<Vec<PathBuf>>,
    /// Whether a rename is being edited: while `true` a bar at the top shows
    /// the new name, and `Enter` renames the selected entry.
    renaming: bool,
    /// The new name being typed (see [`FileTree::renaming`]).
    rename_input: String,
    /// Whether a destination is being edited for moving the marked entry (or
    /// cursor entry when nothing is marked): while `true` a bar at the top
    /// shows the destination, and `Enter` moves.
    moving: bool,
    /// The destination path being typed (see [`FileTree::moving`]).
    move_input: String,
    /// The (column, content width) pair captured when a separator drag
    /// started, or `None` when not resizing.
    resizing: Option<(u16, u16)>,
    /// When the expanded directories were last re-read from disk, so
    /// [`FileTree::auto_refresh`] only does it every [`AUTO_REFRESH_INTERVAL`].
    last_auto_refresh: std::time::Instant,
    /// The widest content the window may grow to: the terminal width minus the
    /// columns reserved for the separator and the editor. Refreshed on render.
    max_width: u16,
}

impl FileTree {
    /// Create a new file tree window rooted at `root`, revealing the current
    /// buffer if it is located under the root and taking keyboard focus.
    pub fn new(root: PathBuf, editor: &mut Editor) -> Self {
        let config = editor.config().file_tree.clone();
        let icons = config.icons.enabled();
        let root = helix_stdx::path::normalize(root);
        let mut tree = Tree::new(root, config);
        if let Some(path) = doc!(editor).path() {
            tree.reveal(&helix_stdx::path::normalize(path));
        }
        editor.file_tree_window.focused = true;
        Self {
            tree,
            offset: 0,
            area: Rect::default(),
            icons,
            filtering: false,
            window_prefix: false,
            show_keymap: false,
            filter: String::new(),
            pending_delete: None,
            renaming: false,
            rename_input: String::new(),
            moving: false,
            move_input: String::new(),
            resizing: None,
            last_auto_refresh: std::time::Instant::now(),
            max_width: 0,
        }
    }

    /// Re-read the expanded directories from disk as long as the previous
    /// re-read is older than [`AUTO_REFRESH_INTERVAL`]. Returns whether the
    /// listing actually changed.
    fn auto_refresh(&mut self) -> bool {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_auto_refresh) < AUTO_REFRESH_INTERVAL {
            return false;
        }
        self.last_auto_refresh = now;
        self.tree.refresh_all()
    }

    /// The entries currently on display: narrowed to the filter query when a
    /// filter is active.
    fn visible_entries(&self) -> Vec<(&TreeEntry, usize)> {
        self.tree.filtered_visible(&self.filter)
    }

    /// Leave filter mode and clear the query.
    fn clear_filter(&mut self) {
        self.filtering = false;
        self.filter.clear();
    }

    fn move_selection(&mut self, delta: isize) {
        let visible = self.visible_entries();
        let len = visible.len();
        if len == 0 {
            return;
        }
        let index = self.tree.selected_index(&visible);
        let new_index = (index as isize + delta).clamp(0, len as isize - 1) as usize;
        if let Some(path) = visible.get(new_index).map(|(entry, _)| entry.path.clone()) {
            self.tree.select(&path);
        }
    }

    fn move_to(&mut self, position: usize) {
        let visible = self.visible_entries();
        let len = visible.len();
        if len == 0 {
            return;
        }
        if let Some(path) = visible
            .get(position.min(len - 1))
            .map(|(entry, _)| entry.path.clone())
        {
            self.tree.select(&path);
        }
    }

    /// Move the selection to the parent of the selected entry, or collapse
    /// the selected directory if it is expanded.
    fn collapse_or_go_up(&mut self) {
        let selected = self.tree.selected.clone();
        if self.tree.selected_is_expanded_dir() {
            self.tree.collapse(&selected);
            return;
        }
        if let Some(parent) = self.tree.parent(&selected) {
            self.tree.select(&parent);
        }
    }

    /// Open the selected entry: expand it if it is a directory, otherwise open
    /// the file and hand focus back to the editor. The window stays open.
    fn open_selected(&mut self, ctx: &mut Context, action: Action) -> EventResult {
        // While a filter is active, the entry to open is the one under the
        // selection in the narrowed listing (the raw selection may be filtered
        // out entirely), falling back to the first match.
        let path = if self.filtering {
            let visible = self.visible_entries();
            let index = self.tree.selected_index(&visible);
            visible.get(index).map(|(entry, _)| entry.path.clone())
        } else {
            self.tree.selected_path()
        };
        let Some(path) = path else {
            return EventResult::Consumed(None);
        };
        if path.is_dir() {
            self.tree.toggle(&path);
            return EventResult::Consumed(None);
        }
        if let Err(e) = ctx.editor.open(&path, action) {
            let err = if let Some(err) = e.source() {
                format!("{}", err)
            } else {
                format!("unable to open \"{}\"", path.display())
            };
            ctx.editor.set_error(err);
            return EventResult::Consumed(None);
        }
        ctx.editor.file_tree_window.focused = false;
        ctx.editor.autoinfo = None;
        // Opening a file hands focus to the editor; leave filter mode so the
        // next visit starts from the full tree.
        self.clear_filter();
        EventResult::Consumed(None)
    }

    /// Enter rename mode (`r`) for the selected entry: a bar at the top shows
    /// the entry's name and `Enter` renames it on disk.
    fn start_rename(&mut self) {
        let name = self
            .tree
            .selected_path()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        self.rename_input = name;
        self.renaming = true;
    }

    /// Apply the rename typed in the rename bar (`r` then `Enter`). On
    /// success the tree paths are rewritten, the new path is selected and
    /// rename mode ends; on failure an error is shown and the edit stays open.
    fn commit_rename(&mut self, ctx: &mut Context) {
        let Some(old_name) = self.tree.selected_path() else {
            self.renaming = false;
            return;
        };
        if self.rename_input.is_empty() || self.rename_input.trim().is_empty() {
            ctx.editor.set_error("Name cannot be empty");
            return;
        }
        let new = old_name
            .parent()
            .map(|parent| parent.join(self.rename_input.trim()));
        let Some(new) = new else {
            self.renaming = false;
            return;
        };
        if new == old_name {
            self.renaming = false;
            return;
        }
        if new.exists() {
            ctx.editor.set_error(format!(
                "{} already exists",
                new.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ));
            return;
        }
        if let Err(err) = std::fs::rename(&old_name, &new) {
            ctx.editor
                .set_error(format!("Failed to rename {}: {err}", old_name.display()));
            return;
        }
        forget_expanded(&self.tree.root, &old_name);
        self.tree.rename_paths(&old_name, &new);
        self.tree.select(&new);
        self.renaming = false;
        ctx.editor.set_status(format!(
            "Renamed {} -> {}",
            old_name
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default(),
            new.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        ));
    }

    /// Permanently delete the selected file or directory (`d`, `Delete`).
    /// The selection moves to the entry that took its place, or to the
    /// parent directory when nothing remains.
    fn delete_selected(&mut self, paths: &[PathBuf], ctx: &mut Context) {
        let visible = self.visible_entries();
        let index = self.tree.selected_index(&visible);
        drop(visible);
        let (deleted, errors) = self.tree.delete_paths(paths);
        let visible = self.visible_entries();
        let index = index.min(visible.len().saturating_sub(1));
        let next = visible.get(index).map(|(entry, _)| entry.path.clone());
        drop(visible);
        if let Some(path) = next {
            self.tree.select(&path);
        }
        if errors.is_empty() {
            ctx.editor.set_status(format!("Deleted {deleted} entries"));
        } else {
            ctx.editor
                .set_error(format!("Deleted {deleted} entries; {}", errors.join("; ")));
        }
    }

    /// Open a destination bar for moving the marked entry (or the cursor
    /// entry when nothing is marked). When the selection is a directory it is
    /// prefilled as the destination itself, so `Enter` moves the entries
    /// inside it; otherwise the selected entry's parent is prefilled and the
    /// user edits the destination.
    fn start_move(&mut self) {
        let selected = self.tree.selected.clone();
        let prefix = if self.tree.selected_is_dir() {
            selected.display().to_string()
        } else {
            selected
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| self.tree.root.display().to_string())
        };
        self.move_input = prefix;
        self.moving = true;
    }

    /// Move the marked entry (or the cursor entry when nothing is marked)
    /// into the destination typed in the move bar (`m` then `Enter`). On
    /// success the tree paths are reparented and moved marks follow; on
    /// failure an error is shown and the bar stays open.
    fn commit_move(&mut self, ctx: &mut Context) {
        let input = self.move_input.trim();
        if input.is_empty() {
            ctx.editor.set_error("Destination cannot be empty");
            return;
        }
        // Resolve a relative destination against the tree root so bare names
        // like `sub` point inside the root.
        let dest_path = PathBuf::from(input);
        let dest = if dest_path.is_absolute() {
            dest_path
        } else {
            self.tree.root.join(&dest_path)
        };
        let Some(dest) = dest.canonicalize().ok() else {
            ctx.editor
                .set_error(format!("Destination {input} does not exist"));
            return;
        };
        if !dest.is_dir() {
            ctx.editor
                .set_error(format!("Destination {} is not a directory", dest.display()));
            return;
        }
        let paths = if self.tree.marked.is_empty() {
            self.tree
                .selected_path()
                .map(|p| vec![p])
                .unwrap_or_default()
        } else {
            self.tree.deletion_targets()
        };
        // The destination must not be one of the move targets themselves.
        if paths.iter().any(|p| *p == dest || dest.starts_with(p)) {
            ctx.editor
                .set_error("Destination must be a directory outside the items being moved");
            return;
        }
        let (moved_count, errors) = self.tree.move_paths(&paths, &dest);
        self.moving = false;
        self.move_input.clear();
        if moved_count > 0 {
            if let Some(last) = paths.last() {
                let mut moved_path = last.clone();
                if let Some(name) = moved_path.file_name() {
                    moved_path = dest.join(name);
                }
                self.tree.select(&moved_path);
            }
        }
        if errors.is_empty() {
            ctx.editor
                .set_status(format!("Moved {moved_count} entries"));
        } else {
            ctx.editor.set_error(format!(
                "Moved {moved_count} entries; {}",
                errors.join("; ")
            ));
        }
    }

    fn handle_mouse(&mut self, event: &MouseEvent, ctx: &mut Context) -> EventResult {
        let MouseEvent {
            kind, row, column, ..
        } = *event;
        // Any click closes the keymap modal and cancels a pending deletion;
        // the click itself is then handled normally below.
        if self.show_keymap && matches!(kind, MouseEventKind::Down(_)) {
            self.show_keymap = false;
        }
        self.pending_delete = None;
        self.renaming = false;
        self.moving = false;
        self.move_input.clear();
        let area = self.area;
        // The separator column doubles as the resize handle. Only events over
        // the tree's own columns belong to the tree; anything else falls
        // through to the editor underneath.
        let separator = area.right();
        let vertical = row >= area.top() && row < area.bottom();
        let inside = vertical && column >= area.left() && column < separator;
        match kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                self.resizing = None;
                if !inside {
                    // Scrolling over the editor hands focus to it as well.
                    ctx.editor.file_tree_window.focused = false;
                    ctx.editor.autoinfo = None;
                    return EventResult::Ignored(None);
                }
                if kind == MouseEventKind::ScrollDown {
                    self.move_selection(WHEEL_SCROLL);
                } else {
                    self.move_selection(-WHEEL_SCROLL);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if vertical && column == separator {
                    // Start resizing the window from its separator.
                    self.resizing = Some((column, area.width));
                    return EventResult::Consumed(None);
                }
                self.resizing = None;
                if !inside {
                    // Clicking outside the tree hands focus to the editor.
                    ctx.editor.file_tree_window.focused = false;
                    ctx.editor.autoinfo = None;
                    return EventResult::Ignored(None);
                }
                ctx.editor.file_tree_window.focused = true;
                // The search bar occupies the first row while filtering, so
                // the entry rows below it are shifted by one.
                let local_row = (row - area.top()).saturating_sub(u16::from(self.filtering));
                let index = self.offset + local_row as usize;
                let hit = self
                    .visible_entries()
                    .get(index)
                    .map(|(entry, depth)| (entry.path.clone(), entry.is_dir, *depth));
                if let Some((path, is_dir, depth)) = hit {
                    // Clicking on the expand/collapse symbol toggles the directory.
                    let symbol_column = area.left() + (depth * INDENT) as u16;
                    if is_dir && column == symbol_column {
                        self.tree.toggle(&path);
                    } else {
                        self.tree.select(&path);
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((start_column, start_width)) = self.resizing {
                    let delta = i32::from(column) - i32::from(start_column);
                    let width = (i32::from(start_width) + delta)
                        .clamp(MIN_WIDTH as i32, i32::from(self.max_width))
                        as u16;
                    ctx.editor.file_tree_window.width = width;
                    return EventResult::Consumed(None);
                }
                if inside {
                    return EventResult::Consumed(None);
                }
                return EventResult::Ignored(None);
            }
            _ => {
                self.resizing = None;
                if !inside {
                    return EventResult::Ignored(None);
                }
            }
        }
        EventResult::Consumed(None)
    }
}

/// The VS Code style icon for a file, chosen from its file name or extension.
/// Falls back to the generic file icon for unknown types.
fn file_icon(name: &str) -> &'static str {
    match name {
        ".gitignore" | ".gitattributes" | ".gitmodules" => return ICON_GIT,
        "Dockerfile" | "Containerfile" | ".dockerignore" => return ICON_DOCKER,
        "Makefile" | "makefile" | "GNUmakefile" | "CMakeLists.txt" => return ICON_MAKEFILE,
        "LICENSE" | "LICENSE.md" | "COPYING" => return ICON_LICENSE,
        _ => {}
    }
    // Case-insensitive extension match.
    let ext = name
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => ICON_RUST,
        "go" => ICON_GO,
        "py" => ICON_PYTHON,
        "js" | "mjs" | "cjs" => ICON_JAVASCRIPT,
        "ts" | "tsx" => ICON_TYPESCRIPT,
        "jsx" => ICON_REACT,
        "svelte" => ICON_SVELTE,
        "html" | "htm" => ICON_HTML,
        "css" | "scss" | "sass" | "less" => ICON_CSS,
        "md" | "markdown" | "mdown" => ICON_MARKDOWN,
        "json" | "jsonc" | "json5" => ICON_JSON,
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" => ICON_CONFIG,
        "sh" | "bash" | "zsh" | "fish" | "ksh" | "command" => ICON_SHELL,
        "c" | "h" => ICON_C,
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "c++" | "h++" => ICON_CPLUSPLUS,
        "java" => ICON_JAVA,
        "php" => ICON_PHP,
        "rb" | "gemspec" | "rake" | "ru" => ICON_RUBY,
        "lua" => ICON_LUA,
        "hs" | "lhs" => ICON_HASKELL,
        "ex" | "exs" | "heex" => ICON_ELIXIR,
        "sql" | "db" | "sqlite" | "sqlite3" => ICON_DATABASE,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "bmp" => ICON_IMAGE,
        "mp4" | "mkv" | "avi" | "webm" | "mov" | "mp3" | "wav" | "flac" | "ogg" => ICON_VIDEO,
        _ => ICON_FILE,
    }
}

/// The VS Code style icon for a directory, chosen from its name: well-known
/// folders like `src` or `assets` get a dedicated folder glyph, anything else
/// falls back to the generic folder icon.
fn folder_icon(name: &str) -> Option<&'static str> {
    match name {
        // Configuration.
        "config" | ".config" | "settings" | ".settings" | ".vscode" | ".idea" | "dotfiles" => {
            Some(ICON_FOLDER_CONFIG)
        }
        // Version control.
        ".git" => Some(ICON_GIT),
        ".github" => Some(ICON_FOLDER_GITHUB),
        // Source code.
        "src" | "source" | "lib" | "include" | "inc" | "app" | "apps" | "core" | "internal" => {
            Some(ICON_FOLDER_SRC)
        }
        // Scripts and tooling.
        "scripts" | "script" | "bin" | "cmd" | "hooks" | "tasks" | "tools" | "utils"
        | "helpers" => Some(ICON_FOLDER_SCRIPT),
        // Images and other visual media.
        "assets" | "images" | "image" | "img" | "media" | "static" | "icons" | "pics"
        | "photos" | "screenshots" => Some(ICON_FOLDER_IMAGE),
        // Audio.
        "music" | "audio" | "sounds" | "sound" => Some(ICON_FOLDER_MUSIC),
        // Documentation.
        "docs" | "doc" | "documentation" | "wiki" | "man" | "help" | "notes" => {
            Some(ICON_FOLDER_DOCS)
        }
        // Tests.
        "test" | "tests" | "__tests__" | "spec" | "specs" | "testing" => Some(ICON_FOLDER_TEST),
        // Dependencies.
        "node_modules" | "vendor" | "third_party" | "thirdparty" | "deps" | "packages" => {
            Some(ICON_FOLDER_NODE_MODULES)
        }
        // Build output.
        "dist" | "build" | "out" | "target" | "coverage" | "bundle" => Some(ICON_FOLDER_BUILD),
        // Environments and secrets.
        "env" | ".env" | "environment" | "venv" | ".venv" => Some(ICON_FOLDER_ENV),
        // Data.
        "data" | "db" | "database" | "datasets" | "sql" => Some(ICON_FOLDER_DATA),
        // Caches and temporary files.
        "cache" | ".cache" | "tmp" | "temp" => Some(ICON_FOLDER_CACHE),
        // Archives.
        "archive" | "archives" | "backup" | "backups" | "old" => Some(ICON_FOLDER_ARCHIVE),
        _ => None,
    }
}

/// The glyph rendered before an entry's name: a Nerd Font folder/file icon
/// when icons are enabled, otherwise an ASCII expand/collapse arrow. Well-known
/// folders get their own folder glyph; other folders toggle between the open
/// and closed folder icons. A user-configured glyph from `folder_icons` (the
/// `[editor.file-tree] folder-icons` option) wins over the built-in folder
/// icons.
fn symbol_for<'a>(
    entry: &TreeEntry,
    icons: bool,
    folder_icons: &'a HashMap<String, String>,
) -> &'a str {
    if icons {
        if entry.is_dir {
            if let Some(icon) = folder_icons.get(entry.name()) {
                return icon;
            }
            folder_icon(entry.name()).unwrap_or_else(|| {
                if entry.expanded {
                    ICON_FOLDER_OPEN
                } else {
                    ICON_FOLDER
                }
            })
        } else {
            file_icon(entry.name())
        }
    } else if entry.is_dir {
        if entry.expanded {
            ARROW_EXPANDED
        } else {
            ARROW_COLLAPSED
        }
    } else {
        " "
    }
}

impl Component for FileTree {
    fn handle_event(&mut self, event: &Event, ctx: &mut Context) -> EventResult {
        if let Event::Mouse(event) = event {
            return self.handle_mouse(event, ctx);
        }
        // Without keyboard focus the window sits in the background: every
        // other input falls through to the editor underneath.
        if !ctx.editor.file_tree_window.focused {
            return EventResult::Ignored(None);
        }
        let key_event = match event {
            Event::Key(event) => *event,
            Event::Paste(..) | Event::Resize(..) => return EventResult::Consumed(None),
            _ => return EventResult::Ignored(None),
        };

        // `C-w` opens the window prefix, mirroring the editor's \"C-w\" mode:
        // the next key moves focus between the tree and the editor splits.
        if self.window_prefix {
            self.window_prefix = false;
            match key_event {
                key!(Esc) => return EventResult::Consumed(None),
                // The editor is the next window to the right of the tree.
                key!('w') | ctrl!('w') | key!('W') | shift!('w') | key!('l') | key!(Right) => {
                    ctx.editor.file_tree_window.focused = false;
                    return EventResult::Consumed(None);
                }
                // The tree is the leftmost (and full-height) window; there is
                // nowhere to go in these directions.
                key!('h') | key!(Left) | key!('k') | key!(Up) | key!('j') | key!(Down) => {
                    return EventResult::Consumed(None);
                }
                // Any other key cancels the prefix and runs normally below.
                _ => {}
            }
        }

        // While a filter is being edited, printable characters extend the
        // query (the search bar at the top shows it); navigation keys fall
        // through to the regular handlers below. `Esc` leaves filter mode
        // before handing focus back to the editor.
        if self.filtering {
            match key_event {
                key!(Esc) | ctrl!('c') => {
                    self.clear_filter();
                    return EventResult::Consumed(None);
                }
                key!(Backspace) | shift!(Backspace) => {
                    self.filter.pop();
                    return EventResult::Consumed(None);
                }
                key!(Enter) => {
                    return self.open_selected(ctx, Action::Replace);
                }
                KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers,
                } if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    // The first typed character loads the rest of the tree so
                    // matches are found anywhere, not just under expanded
                    // directories.
                    if self.filter.is_empty() {
                        self.tree.load_all();
                    }
                    self.filter.push(c);
                    return EventResult::Consumed(None);
                }
                _ => {}
            }
        }

        // While the keymap modal is open any key closes it first; the key
        // then falls through to the match below, so the listed bindings work
        // straight from the modal. `Esc` only dismisses it.
        if self.show_keymap {
            self.show_keymap = false;
            ctx.editor.autoinfo = None;
            match key_event {
                key!(Esc) | ctrl!('c') => return EventResult::Consumed(None),
                _ => {}
            }
        }

        // A deletion is armed after the first `d`: a second `d`/`Delete` on the
        // same entry confirms it, `Esc`/`Ctrl-c` cancels it, and any other key
        // cancels the arming and runs normally below (e.g. moving the
        // selection). Whatever the cancelling key is, the prompt's status line
        // is dismissed so the confirmation visibly goes away.
        if let Some(paths) = self.pending_delete.take() {
            match key_event {
                key!('d') | key!(Delete) if paths == self.tree.deletion_targets() => {
                    self.delete_selected(&paths, ctx);
                    return EventResult::Consumed(None);
                }
                key!(Esc) | ctrl!('c') => {
                    ctx.editor.set_status("Deletion cancelled");
                    return EventResult::Consumed(None);
                }
                // Any other key — including movement keys like `j`/`k` — cancels
                // the prompt as well: dismiss its status line, then let the key
                // act normally below (e.g. `j` moves the selection).
                _ => {
                    ctx.editor.set_status("Deletion cancelled");
                }
            }
        }

        // While a rename is being edited, printable characters extend the
        // name in the bar at the top; `Enter` renames the selected entry and
        // `Esc` cancels. Every other key is consumed so the selection cannot
        // drift away from the entry being renamed.
        if self.renaming {
            match key_event {
                key!(Esc) | ctrl!('c') => {
                    self.renaming = false;
                    return EventResult::Consumed(None);
                }
                key!(Enter) => {
                    self.commit_rename(ctx);
                    return EventResult::Consumed(None);
                }
                key!(Backspace) | shift!(Backspace) => {
                    self.rename_input.pop();
                    return EventResult::Consumed(None);
                }
                KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers,
                } if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    self.rename_input.push(c);
                    return EventResult::Consumed(None);
                }
                _ => return EventResult::Consumed(None),
            }
        }

        // While a destination is being edited for moving, printable
        // characters extend the path in the bar at the top; `Enter` moves the
        // marked entry (or the cursor entry) into it and `Esc` cancels. Every
        // other key is consumed so the selection cannot drift.
        if self.moving {
            match key_event {
                key!(Esc) | ctrl!('c') => {
                    self.moving = false;
                    self.move_input.clear();
                    return EventResult::Consumed(None);
                }
                key!(Enter) => {
                    self.commit_move(ctx);
                    return EventResult::Consumed(None);
                }
                key!(Backspace) | shift!(Backspace) => {
                    self.move_input.pop();
                    return EventResult::Consumed(None);
                }
                KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers,
                } if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    self.move_input.push(c);
                    return EventResult::Consumed(None);
                }
                _ => return EventResult::Consumed(None),
            }
        }

        match key_event {
            ctrl!('w') => {
                // Start the window prefix (see above).
                self.window_prefix = true;
            }
            // `?` is Shift+/; terminals disagree on how they report it:
            // legacy terminals send just the shifted character `Char('?')`,
            // crossterm's "disambiguate only" / Windows Terminal send the
            // shifted character with the SHIFT modifier set, and the full
            // kitty protocol sends the physical key `Char('/')` with SHIFT.
            // Accept all three; Ctrl/Alt-? is not the help key.
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
            } if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && (c == '?' || (c == '/' && modifiers.contains(KeyModifiers::SHIFT))) =>
            {
                // Which-key style help: the popup is an `Info` box rendered by
                // the editor view exactly like the one shown after the `space`
                // leader key, so it appears in the same position; any key
                // closes it and runs that binding (see the dismissal block
                // above).
                self.show_keymap = true;
                ctx.editor.autoinfo = Some(keymap_info());
            }
            key!('/') => {
                // Start filtering the tree to matching paths. Re-read the
                // expanded directories first so newly created files appear in
                // the results.
                self.tree.refresh_all();
                self.last_auto_refresh = std::time::Instant::now();
                self.filtering = true;
            }
            key!(Up) | key!('k') | ctrl!('p') => {
                self.move_selection(-1);
            }
            key!(Down) | key!('j') | ctrl!('n') => {
                self.move_selection(1);
            }
            key!(PageUp) | ctrl!('u') => {
                self.move_selection(-10);
            }
            key!(PageDown) | ctrl!('d') => {
                self.move_selection(10);
            }
            key!(Home) => {
                self.move_to(0);
            }
            key!(End) => {
                self.move_to(usize::MAX);
            }
            key!(Left) | key!('h') => {
                self.collapse_or_go_up();
            }
            key!(Right) | key!('l') => {
                let selected = self.tree.selected.clone();
                if self.tree.selected_is_expanded_dir() {
                    // Enter the first child of an expanded directory.
                    self.move_selection(1);
                } else if Tree::find(&self.tree.entries, &selected)
                    .is_some_and(|entry| entry.is_dir)
                {
                    self.tree.expand(&selected);
                } else {
                    return self.open_selected(ctx, Action::Replace);
                }
            }
            key!(Enter) => {
                return self.open_selected(ctx, Action::Replace);
            }
            key!(Esc) | ctrl!('c') => {
                // Hand focus back to the editor; the window stays open.
                ctx.editor.file_tree_window.focused = false;
                return EventResult::Ignored(None);
            }
            key!('q') => {
                // Close the tree window (its docked columns go back to the
                // editor). Reusing the same callback as `close_file_tree`.
                let callback = Box::new(|compositor: &mut Compositor, ctx: &mut Context| {
                    compositor.remove(ID);
                    ctx.editor.file_tree_window.open = false;
                    compositor.need_full_redraw();
                });
                return EventResult::Consumed(Some(callback));
            }
            key!(' ') => {
                self.tree.toggle_mark();
                ctx.editor
                    .set_status(format!("{} marked", self.tree.marked.len()));
            }
            crate::alt!(' ') => {
                self.tree.marked.clear();
                ctx.editor.set_status("Selection cleared");
            }
            key!('r') => {
                self.start_rename();
            }
            key!('R') | shift!('r') => {
                self.tree.refresh(&self.tree.selected.clone());
            }
            key!('m') => {
                self.start_move();
            }
            key!('d') | key!(Delete) => {
                let paths = self.tree.deletion_targets();
                if paths.is_empty() {
                    ctx.editor
                        .set_error("Cannot delete the tree root directory");
                } else {
                    let names = paths
                        .iter()
                        .map(|path| {
                            path.strip_prefix(&self.tree.root)
                                .unwrap_or(path)
                                .display()
                                .to_string()
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    ctx.editor.set_status(format!(
                        "Press d/Delete again to permanently delete {} entries (directories recursively): {names}",
                        paths.len()
                    ));
                    self.pending_delete = Some(paths);
                }
            }
            _ => {}
        }
        EventResult::Consumed(None)
    }

    fn render(&mut self, area: Rect, surface: &mut Surface, ctx: &mut Context) {
        // Pick up files created or removed outside the editor: re-read the
        // expanded directories at most every AUTO_REFRESH_INTERVAL, so the
        // listing drawn below matches the disk.
        self.auto_refresh();

        let theme = &ctx.editor.theme;
        let background = theme.get("ui.background");
        let directory_style = theme.get("ui.text.directory");
        let file_style = theme.get("ui.text");
        let selected_style = theme.get("ui.cursorline.primary");
        let window_style = theme.get("ui.window");

        let width = content_width(area, preferred_width(ctx.editor));
        self.max_width = area.width.saturating_sub(2);
        // The window spans the whole height of the screen area; the editor is
        // laid out to its right, so the editor's statusline rows never
        // overlap it.
        let tree_area = Rect {
            x: area.x,
            y: area.y,
            width,
            height: area.height,
        };
        self.area = tree_area;
        if width == 0 || tree_area.height == 0 {
            return;
        }

        // Clear the whole window so no stale cells from the editor (or from
        // an earlier wider layout) remain, then draw the separator column.
        surface.clear_with(tree_area, background);
        if tree_area.right() < area.right() {
            for y in tree_area.top()..tree_area.bottom() {
                surface[(tree_area.right(), y)]
                    .set_symbol(tui::symbols::line::VERTICAL)
                    .set_style(window_style);
            }
        }

        // The search / rename / move bar occupies the first row while one is
        // active; the rows below it (minus the editor's statusline /
        // commandline rows) hold tree entries.
        let bar_rows = u16::from(self.filtering || self.renaming || self.moving);
        let height = tree_area.height.saturating_sub(2 + bar_rows) as usize;
        if height == 0 {
            return;
        }

        // While filtering, renaming or moving, highlight the first row as the
        // input bar showing the current query / new name / destination.
        if self.filtering || self.renaming || self.moving {
            let bar_style = theme.get("ui.text").patch(selected_style);
            surface.clear_with(
                Rect {
                    x: tree_area.x,
                    y: tree_area.y,
                    width: tree_area.width,
                    height: 1,
                },
                bar_style,
            );
            let bar = if self.filtering {
                format!("/{}", self.filter)
            } else if self.moving {
                format!("move to: {}", self.move_input)
            } else {
                format!("rename: {}", self.rename_input)
            };
            surface.set_stringn(
                tree_area.x,
                tree_area.y,
                &bar,
                tree_area.width as usize,
                bar_style,
            );
        }

        // Inlined (rather than via `visible_entries`) so only `self.tree` and
        // `self.filter` stay borrowed here; the scroll adjustment below
        // mutates `self.offset`.
        let visible = self.tree.filtered_visible(&self.filter);
        let selected_index = self.tree.selected_index(&visible);

        // Scroll to keep the selection visible.
        if selected_index < self.offset {
            self.offset = selected_index;
        } else if selected_index >= self.offset + height {
            self.offset = selected_index - height + 1;
        }

        let mut line = String::new();
        for (row, (entry, depth)) in visible.iter().enumerate().skip(self.offset).take(height) {
            let y = tree_area.y + bar_rows + (row - self.offset) as u16;
            let symbol = symbol_for(entry, self.icons, &self.tree.config.folder_icons);

            line.clear();
            if !self.tree.marked.is_empty() {
                line.push_str(if self.tree.marked.contains(&entry.path) {
                    "* "
                } else {
                    "  "
                });
            }
            for _ in 0..*depth {
                line.push(' ');
                line.push(' ');
            }
            line.push_str(symbol);
            line.push(' ');
            line.push_str(entry.name());

            let base_style = if entry.is_dir {
                directory_style
            } else {
                file_style
            };
            // Only the focused window highlights its selection; unfocused it
            // stays in the background like an inactive split.
            let style = if ctx.editor.file_tree_window.focused && row == selected_index {
                base_style.patch(selected_style)
            } else {
                base_style
            };
            surface.set_stringn(tree_area.x, y, &line, tree_area.width as usize, style);
        }
    }

    fn cursor(&self, _area: Rect, _ctx: &Editor) -> (Option<helix_core::Position>, CursorKind) {
        (None, CursorKind::Hidden)
    }

    fn id(&self) -> Option<&'static str> {
        Some(ID)
    }
}

/// The which-key style keymap popup shown with `?` while the tree is focused:
/// an [`Info`] box handed to the editor view, exactly like the popup the
/// `space` leader key shows while waiting for the next key, so it renders in
/// the same position and style. Any key closes it (see the dismissal block in
/// [`FileTree::handle_event`]) and then runs that binding.
fn keymap_info() -> Info {
    const BINDINGS: &[(&str, &str)] = &[
        ("k/↑, j/↓", "move selection"),
        ("ctrl-p/n", "move selection (alternate)"),
        ("ctrl-u/d", "jump 10 entries"),
        ("Home/End", "jump to first/last entry"),
        ("h/←, l/→", "collapse/expand, or open"),
        ("Enter", "open file or toggle directory"),
        ("/", "filter the tree"),
        ("r", "rename file/dir"),
        ("R", "refresh selected directory"),
        ("m", "move marked or cursor entry to a directory"),
        ("Space", "toggle batch mark"),
        ("Alt-Space", "clear batch marks"),
        ("d / Del", "delete marks or cursor entry (again to confirm)"),
        ("q", "close the tree"),
        ("Esc", "hand focus back to the editor"),
        ("C-w w", "focus next window (editor)"),
        ("C-w h", "no-op: tree is leftmost"),
        ("C-w l", "focus the editor"),
        ("?", "show this help"),
    ];
    Info::new("File tree", &BINDINGS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A config which shows every file (no ignore filtering), so tests don't
    /// depend on git state or hidden files.
    fn permissive_config() -> FileTreeConfig {
        FileTreeConfig {
            hidden: false,
            follow_symlinks: false,
            parents: false,
            ignore: false,
            git_ignore: false,
            git_global: false,
            git_exclude: false,
            ..FileTreeConfig::default()
        }
    }

    /// Create a unique temporary directory which is removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("ffx-file-tree-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "").unwrap();
    }

    /// The paths of the visible entries, relative to the tree root, with
    /// `/` separators so assertions read the same on every platform.
    fn visible_paths(tree: &Tree) -> Vec<String> {
        tree.visible()
            .iter()
            .map(|(entry, _)| {
                entry
                    .path
                    .strip_prefix(&tree.root)
                    .unwrap_or(&entry.path)
                    .display()
                    .to_string()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn refresh_all_picks_up_external_changes() {
        let tmp = TempDir::new("autorefresh");
        write_file(&tmp.path().join("a.txt"));
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        assert_eq!(visible_paths(&tree), vec!["", "a.txt"]);
        assert!(!tree.refresh_all()); // nothing changed externally yet

        // A file created outside the editor shows up on the next refresh.
        write_file(&tmp.path().join("b.txt"));
        assert!(tree.refresh_all());
        assert_eq!(visible_paths(&tree), vec!["", "a.txt", "b.txt"]);

        // A file removed outside the editor disappears again.
        std::fs::remove_file(tmp.path().join("a.txt")).unwrap();
        assert!(tree.refresh_all());
        assert_eq!(visible_paths(&tree), vec!["", "b.txt"]);
        assert!(!tree.refresh_all());
    }

    #[test]
    fn refresh_all_keeps_expansion_state_and_sees_inside() {
        let tmp = TempDir::new("autorefresh-expand");
        write_file(&tmp.path().join("d/s.txt"));
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        let d = tmp.path().join("d");
        tree.expand(&d);
        assert_eq!(visible_paths(&tree), vec!["", "d", "d/s.txt"]);

        // A new file inside the expanded directory is noticed, and the
        // surviving entries keep their expanded state and cached children.
        write_file(&tmp.path().join("d/t.txt"));
        assert!(tree.refresh_all());
        assert_eq!(visible_paths(&tree), vec!["", "d", "d/s.txt", "d/t.txt"]);
        assert!(!tree.refresh_all());
    }

    #[test]
    fn content_width_fits_preference_and_screen() {
        // A wide terminal: the preferred width decides.
        assert_eq!(content_width(Rect::new(0, 0, 200, 50), 30), 30);
        // A narrow terminal: the tree gives way instead of hiding the editor.
        assert_eq!(content_width(Rect::new(0, 0, 30, 50), 30), 28);
        assert_eq!(content_width(Rect::new(0, 0, 30, 50), 40), 28);
        // The separator column sits between the tree content and the editor.
        assert_eq!(content_width(Rect::new(0, 0, 200, 50), 40) + 1, 41);
    }

    #[test]
    fn lists_directories_first_then_files_sorted() {
        let tmp = TempDir::new("ordering");
        write_file(&tmp.path().join("z.txt"));
        write_file(&tmp.path().join("a.txt"));
        fs::create_dir(tmp.path().join("d2")).unwrap();
        fs::create_dir(tmp.path().join("d1")).unwrap();

        let tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        assert_eq!(
            visible_paths(&tree),
            vec![
                "", // the root entry itself
                "d1", "d2", "a.txt", "z.txt",
            ]
        );
    }

    #[test]
    fn expand_and_collapse_directory() {
        let tmp = TempDir::new("expand");
        write_file(&tmp.path().join("dir/a.rs"));
        write_file(&tmp.path().join("b.rs"));

        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        // Directories start collapsed.
        assert_eq!(visible_paths(&tree), vec!["", "dir", "b.rs"]);

        tree.expand(&tmp.path().join("dir"));
        assert_eq!(visible_paths(&tree), vec!["", "dir", "dir/a.rs", "b.rs"]);

        tree.collapse(&tmp.path().join("dir"));
        assert_eq!(visible_paths(&tree), vec!["", "dir", "b.rs"]);
    }

    #[test]
    fn reveal_expands_ancestors_and_selects_file() {
        let tmp = TempDir::new("reveal");
        let file = tmp.path().join("x/y/f.rs");
        write_file(&file);

        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        tree.reveal(&file);

        let visible = tree.visible();
        // The root entry is depth 0, so `x/y/f.rs` is at depth 3.
        assert!(
            visible
                .iter()
                .any(|(entry, depth)| { entry.path == file && *depth == 3 })
        );
        assert_eq!(tree.selected, file);
        assert_eq!(tree.selected_index(&visible), visible.len() - 1);
    }

    #[test]
    fn reveal_ignores_paths_outside_root() {
        let tmp = TempDir::new("reveal-outside");
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        // No panic and selection stays on the root.
        tree.reveal(Path::new("/definitely/not/under/root/file.rs"));
        assert_eq!(tree.selected, tree.root);
    }

    #[test]
    fn collapsing_a_directory_moves_selection_out_of_it() {
        let tmp = TempDir::new("collapse-sel");
        write_file(&tmp.path().join("dir/sub/file.rs"));

        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        let dir = tmp.path().join("dir");
        let sub = dir.join("sub");
        tree.expand(&dir);
        tree.expand(&sub);
        tree.select(&sub.join("file.rs"));

        // Toggling (collapsing) a directory that is not directly selected
        // pulls the selection up to it so it stays visible.
        tree.toggle(&dir);
        assert_eq!(tree.selected, dir);
        assert_eq!(tree.selected_index(&tree.visible()), 1);
    }

    #[test]
    fn filter_matches_anywhere_case_insensitively() {
        let tmp = TempDir::new("filter");
        write_file(&tmp.path().join("src/main.rs"));
        write_file(&tmp.path().join("src/lib.rs"));
        write_file(&tmp.path().join("docs/guide.md"));

        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        // Children of collapsed directories are not visible normally...
        assert_eq!(visible_paths(&tree), vec!["", "docs", "src"]);
        // ...but once the tree is loaded, a filter finds them anywhere, and
        // matches substrings case-insensitively.
        tree.load_all();
        let names = |filter: &str| -> Vec<String> {
            tree.filtered_visible(filter)
                .iter()
                .map(|(entry, _)| entry.name().to_string())
                .collect()
        };
        assert_eq!(names("main"), vec!["main.rs"]);
        assert_eq!(names("MAIN"), vec!["main.rs"]); // case-insensitive
        assert_eq!(names(".md"), vec!["guide.md"]);
        // Matching a directory also brings in its subtree, whose paths contain
        // the query as a prefix.
        assert_eq!(names("src"), vec!["src", "lib.rs", "main.rs"]);
        // The root entry is hidden while filtering, and a filter with no
        // matches hides everything.
        assert!(names("zzz").is_empty());
    }

    #[test]
    fn filter_preserves_entry_depth() {
        let tmp = TempDir::new("filter-depth");
        write_file(&tmp.path().join("x/y/f.rs"));
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        tree.load_all();
        // Matches keep their natural depth (3 for `x/y/f.rs`).
        let visible = tree.filtered_visible("f.rs");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].1, 3);
    }

    #[test]
    fn symbols_switch_between_icons_and_arrows() {
        let mut dir = TreeEntry::new(PathBuf::from("dir"), true);
        let file = TreeEntry::new(PathBuf::from("f.xyz"), false);
        let none = &HashMap::new();

        // Without icons: ASCII arrows for directories, blank for files.
        assert_eq!(symbol_for(&dir, false, none), ARROW_COLLAPSED);
        dir.expanded = true;
        assert_eq!(symbol_for(&dir, false, none), ARROW_EXPANDED);
        assert_eq!(symbol_for(&file, false, none), " ");

        // With icons: VS Code style folder icons (open/closed) and a file icon.
        assert_eq!(symbol_for(&dir, true, none), ICON_FOLDER_OPEN);
        dir.expanded = false;
        assert_eq!(symbol_for(&dir, true, none), ICON_FOLDER);
        assert_eq!(symbol_for(&file, true, none), ICON_FILE);
    }

    #[test]
    fn user_folder_icons_override_builtins() {
        let mut overrides = HashMap::new();
        overrides.insert("src".to_string(), "*x*".to_string());
        let dir = TreeEntry::new(PathBuf::from("src"), true);
        // A configured glyph wins over the built-in folder icon...
        assert_eq!(symbol_for(&dir, true, &overrides), "*x*");
        // ...and also applies to folders without a built-in icon.
        overrides.insert("misc".to_string(), "*y*".to_string());
        let misc = TreeEntry::new(PathBuf::from("misc"), true);
        assert_eq!(symbol_for(&misc, true, &overrides), "*y*");
        // ...while folders without an entry keep their built-in icon.
        let assets = TreeEntry::new(PathBuf::from("assets"), true);
        assert_eq!(symbol_for(&assets, true, &overrides), ICON_FOLDER_IMAGE);
        // Overrides never apply when icons are disabled.
        assert_eq!(symbol_for(&dir, false, &overrides), ARROW_COLLAPSED);
    }

    #[test]
    fn folder_icons_are_type_specific() {
        assert_eq!(folder_icon("src"), Some(ICON_FOLDER_SRC));
        assert_eq!(folder_icon("assets"), Some(ICON_FOLDER_IMAGE));
        assert_eq!(folder_icon("scripts"), Some(ICON_FOLDER_SCRIPT));
        assert_eq!(folder_icon("docs"), Some(ICON_FOLDER_DOCS));
        assert_eq!(folder_icon("tests"), Some(ICON_FOLDER_TEST));
        assert_eq!(folder_icon(".config"), Some(ICON_FOLDER_CONFIG));
        assert_eq!(folder_icon("music"), Some(ICON_FOLDER_MUSIC));
        assert_eq!(folder_icon("node_modules"), Some(ICON_FOLDER_NODE_MODULES));
        assert_eq!(folder_icon("vendor"), Some(ICON_FOLDER_NODE_MODULES));
        assert_eq!(folder_icon("dist"), Some(ICON_FOLDER_BUILD));
        assert_eq!(folder_icon("target"), Some(ICON_FOLDER_BUILD));
        assert_eq!(folder_icon(".github"), Some(ICON_FOLDER_GITHUB));
        assert_eq!(folder_icon(".git"), Some(ICON_GIT));
        assert_eq!(folder_icon(".env"), Some(ICON_FOLDER_ENV));
        assert_eq!(folder_icon("data"), Some(ICON_FOLDER_DATA));
        assert_eq!(folder_icon("cache"), Some(ICON_FOLDER_CACHE));
        assert_eq!(folder_icon("archive"), Some(ICON_FOLDER_ARCHIVE));
        assert_eq!(folder_icon("tools"), Some(ICON_FOLDER_SCRIPT));
        // Unknown folders fall back to the generic folder icon.
        assert_eq!(folder_icon("misc"), None);
        assert_eq!(folder_icon("README"), None);
    }

    #[test]
    fn special_folder_icons_ignore_expansion_state() {
        let mut dir = TreeEntry::new(PathBuf::from("src"), true);
        let none = &HashMap::new();
        // A well-known folder keeps its dedicated glyph whether collapsed...
        assert_eq!(symbol_for(&dir, true, none), ICON_FOLDER_SRC);
        // ...or expanded, unlike the generic folder icon.
        dir.expanded = true;
        assert_eq!(symbol_for(&dir, true, none), ICON_FOLDER_SRC);
        // Without icons the arrows still reflect the expansion state.
        assert_eq!(symbol_for(&dir, false, none), ARROW_EXPANDED);
        dir.expanded = false;
        assert_eq!(symbol_for(&dir, false, none), ARROW_COLLAPSED);
    }

    #[test]
    fn file_icons_are_type_specific() {
        assert_eq!(file_icon("main.rs"), ICON_RUST);
        assert_eq!(file_icon("UPPER.RS"), ICON_RUST); // case-insensitive
        assert_eq!(file_icon("README.md"), ICON_MARKDOWN);
        assert_eq!(file_icon(".gitignore"), ICON_GIT);
        assert_eq!(file_icon("Makefile"), ICON_MAKEFILE);
        assert_eq!(file_icon("Dockerfile"), ICON_DOCKER);
        assert_eq!(file_icon("data.json"), ICON_JSON);
        assert_eq!(file_icon("app.tsx"), ICON_TYPESCRIPT);
        // Unknown types and extensionless files fall back to the generic icon.
        assert_eq!(file_icon("unknown.xyz"), ICON_FILE);
        assert_eq!(file_icon("noext"), ICON_FILE);
    }

    #[test]
    fn expansion_state_persists_across_sessions() {
        let tmp = TempDir::new("persist");
        let dir = tmp.path().join("dir");
        let sub = dir.join("sub");
        write_file(&sub.join("file.rs"));

        // First session: expand `dir` and `sub`.
        {
            let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
            tree.expand(&dir);
            tree.expand(&sub);
        }

        // A new tree over the same root restores the expansion state.
        let tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        assert_eq!(
            visible_paths(&tree),
            vec!["", "dir", "dir/sub", "dir/sub/file.rs"]
        );
    }

    #[test]
    fn collapsed_state_persists_across_sessions() {
        let tmp = TempDir::new("persist-collapsed");
        let dir = tmp.path().join("dir");
        write_file(&dir.join("file.rs"));

        {
            let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
            tree.expand(&dir);
            tree.collapse(&dir);
        }

        let tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        assert_eq!(visible_paths(&tree), vec!["", "dir"]);
    }

    #[test]
    fn batch_delete_deduplicates_parent_and_child_marks() {
        let tmp = TempDir::new("batch-nested");
        let dir = tmp.path().join("dir");
        let child = dir.join("child");
        let keep = tmp.path().join("keep");
        write_file(&child);
        write_file(&keep);
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        tree.reveal(&child);
        tree.toggle_mark();
        tree.select(&dir);
        tree.toggle_mark();
        tree.collapse(&dir);
        tree.refresh_all();
        let targets = tree.deletion_targets();
        assert_eq!(targets, vec![dir.clone()]);
        let (deleted, errors) = tree.delete_paths(&targets);
        assert_eq!(deleted, 1);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(!dir.exists());
        assert!(keep.exists());
        assert!(tree.marked.is_empty());
    }

    #[test]
    fn batch_delete_keeps_failed_marks_and_continues() {
        let tmp = TempDir::new("batch-failure");
        let missing = tmp.path().join("a-missing");
        let present = tmp.path().join("b-present");
        write_file(&missing);
        write_file(&present);
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        for path in [&missing, &present] {
            tree.select(path);
            tree.toggle_mark();
        }
        fs::remove_file(&missing).unwrap();
        let (deleted, errors) = tree.delete_paths(&tree.deletion_targets());
        assert_eq!(deleted, 1);
        assert_eq!(errors.len(), 1);
        assert!(!present.exists());
        assert_eq!(tree.marked, HashSet::from([missing]));
    }

    #[test]
    fn batch_marks_follow_directory_rename() {
        let tmp = TempDir::new("batch-rename");
        let old = tmp.path().join("old");
        let new = tmp.path().join("new");
        let child = old.join("child");
        write_file(&child);
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        tree.reveal(&child);
        tree.toggle_mark();
        fs::rename(&old, &new).unwrap();
        tree.rename_paths(&old, &new);
        let (deleted, errors) = tree.delete_paths(&tree.deletion_targets());
        assert_eq!(deleted, 1);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(!new.join("child").exists());
        assert!(new.is_dir());
    }

    #[test]
    fn hides_hidden_files_when_configured() {
        let tmp = TempDir::new("hidden");
        write_file(&tmp.path().join(".secret"));
        write_file(&tmp.path().join("visible.rs"));

        let config = FileTreeConfig {
            hidden: true,
            ..permissive_config()
        };
        let tree = Tree::new(tmp.path().to_path_buf(), config);
        assert_eq!(visible_paths(&tree), vec!["", "visible.rs"]);
    }

    #[test]
    fn moves_a_file_into_a_directory() {
        let tmp = TempDir::new("move-file");
        let dest = tmp.path().join("sub");
        write_file(&tmp.path().join("a.txt"));
        fs::create_dir(&dest).unwrap();
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        tree.move_paths(&[tmp.path().join("a.txt")], &dest);
        assert!(!tmp.path().join("a.txt").exists());
        assert!(dest.join("a.txt").exists());
        // The moved entry now lives under `sub` in the tree.
        assert!(Tree::find(&tree.entries, &dest.join("a.txt")).is_some());
    }

    #[test]
    fn moving_a_directory_moves_its_descendants_and_marks() {
        let tmp = TempDir::new("move-dir");
        let dest = tmp.path().join("sub");
        let dir = tmp.path().join("dir");
        fs::create_dir_all(&dir.join("nested")).unwrap();
        write_file(&dir.join("nested").join("x.rs"));
        fs::create_dir(&dest).unwrap();
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        tree.reveal(&dir.join("nested").join("x.rs"));
        tree.select(&dir);
        tree.toggle_mark();
        let (moved, errors) = tree.move_paths(&[dir.clone()], &dest);
        assert_eq!(moved, 1);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(!dir.exists());
        assert!(dest.join("dir").join("nested").join("x.rs").exists());
        // The directory entry and its loaded descendant are reparented under
        // `sub`, and the mark follows to the moved directory.
        assert!(Tree::find(&tree.entries, &dest.join("dir")).is_some());
        assert!(Tree::find(&tree.entries, &dest.join("dir").join("nested").join("x.rs")).is_some());
        assert!(tree.marked.contains(&dest.join("dir")));
    }

    #[test]
    fn refusing_to_move_the_root_or_an_ancestor_of_destination() {
        let tmp = TempDir::new("move-refuse");
        let dest = tmp.path().join("sub");
        fs::create_dir(&dest).unwrap();
        write_file(&tmp.path().join("a.txt"));
        let mut tree = Tree::new(tmp.path().to_path_buf(), permissive_config());
        // Cannot move the root into itself or a descendant.
        let (moved, errors) = tree.move_paths(&[tmp.path().to_path_buf()], &dest);
        assert_eq!(moved, 0);
        assert_eq!(errors.len(), 1);
        // Cannot move a directory into its own descendant.
        let (moved, errors) = tree.move_paths(&[dest.clone()], &dest.join("inner"));
        assert_eq!(moved, 0);
        assert_eq!(errors.len(), 1);
    }
}
