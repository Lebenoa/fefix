//! A VS Code / Zed style file tree sidebar.
//!
//! The tree is a modal layer that renders as a fixed-width sidebar on the left
//! side of the screen, leaving the rest of the editor visible underneath. It
//! lazily loads directory contents on expansion, remembers which directories
//! are expanded while it is open and reveals (expands the ancestors of) the
//! current buffer when opened.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    error::Error,
    path::{Path, PathBuf},
};

use helix_view::{
    editor::{Action, FileTreeConfig},
    graphics::{CursorKind, Rect},
    input::{MouseButton, MouseEvent, MouseEventKind},
    Editor,
};
use tui::buffer::Buffer as Surface;

use crate::{
    compositor::{self, Component, Compositor, Context, Event, EventResult},
    ctrl, key,
};

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
/// Maximum width of the sidebar in columns.
const MAX_WIDTH: u16 = 40;
/// Number of columns of indentation per tree level.
const INDENT: usize = 2;
/// Number of rows scrolled per mouse wheel step.
const WHEEL_SCROLL: isize = 3;

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

    /// The index of the selected entry among the visible entries, or 0 if it
    /// is not visible (e.g. its ancestors got collapsed).
    fn selected_index(&self) -> usize {
        self.visible()
            .iter()
            .position(|(entry, _)| entry.path == self.selected)
            .unwrap_or(0)
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
}

fn flatten<'a>(entries: &'a [TreeEntry], depth: usize, out: &mut Vec<(&'a TreeEntry, usize)>) {
    for entry in entries {
        out.push((entry, depth));
        if entry.is_dir && entry.expanded {
            flatten(&entry.children, depth + 1, out);
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
        .add_custom_ignore_filename(".helix/ignore")
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

/// A VS Code / Zed style file tree sidebar component.
pub struct FileTree {
    tree: Tree,
    /// First visible row, for scrolling.
    offset: usize,
    /// The area the tree was last rendered into, used for mouse handling.
    area: Rect,
    /// Whether to render Nerd Font folder/file icons instead of ASCII arrows.
    icons: bool,
}

impl FileTree {
    /// Create a new file tree rooted at `root`, revealing the current buffer
    /// if it is located under the root.
    pub fn new(root: PathBuf, editor: &Editor) -> Self {
        let config = editor.config().file_tree.clone();
        let icons = config.icons.enabled();
        let root = helix_stdx::path::normalize(root);
        let mut tree = Tree::new(root, config);
        if let Some(path) = doc!(editor).path() {
            tree.reveal(&helix_stdx::path::normalize(path));
        }
        Self {
            tree,
            offset: 0,
            area: Rect::default(),
            icons,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let visible = self.tree.visible();
        let len = visible.len();
        if len == 0 {
            return;
        }
        let index = self.tree.selected_index();
        let new_index = (index as isize + delta).clamp(0, len as isize - 1) as usize;
        if let Some(path) = visible.get(new_index).map(|(entry, _)| entry.path.clone()) {
            self.tree.select(&path);
        }
    }

    fn move_to(&mut self, position: usize) {
        let visible = self.tree.visible();
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
    /// the file and close the tree.
    fn open_selected(&mut self, ctx: &mut Context, action: Action) -> EventResult {
        let Some(path) = self.tree.selected_path() else {
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
        close_callback()
    }

    fn handle_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let MouseEvent {
            kind, row, column, ..
        } = *event;
        match kind {
            MouseEventKind::ScrollDown => self.move_selection(WHEEL_SCROLL),
            MouseEventKind::ScrollUp => self.move_selection(-WHEEL_SCROLL),
            MouseEventKind::Down(MouseButton::Left) => {
                let area = self.area;
                if row < area.top()
                    || row >= area.bottom()
                    || column < area.left()
                    || column >= area.right()
                {
                    return EventResult::Consumed(None);
                }
                let index = self.offset + (row - area.top()) as usize;
                let hit = self
                    .tree
                    .visible()
                    .get(index)
                    .map(|(entry, depth)| (entry.path.clone(), entry.is_dir, *depth));
                if let Some((path, is_dir, depth)) = hit {
                    // Clicking on the expand/collapse arrow toggles the directory.
                    let arrow_column = area.left() + (depth * INDENT) as u16;
                    if is_dir && column == arrow_column {
                        self.tree.toggle(&path);
                    } else {
                        self.tree.select(&path);
                    }
                }
            }
            _ => {}
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

/// The glyph rendered before an entry's name: a Nerd Font folder/file icon
/// when icons are enabled, otherwise an ASCII expand/collapse arrow.
fn symbol_for(entry: &TreeEntry, icons: bool) -> &'static str {
    if icons {
        if entry.is_dir {
            if entry.expanded {
                ICON_FOLDER_OPEN
            } else {
                ICON_FOLDER
            }
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

fn close_callback() -> EventResult {
    let callback: compositor::Callback = Box::new(|compositor: &mut Compositor, _ctx| {
        compositor.pop();
    });
    EventResult::Consumed(Some(callback))
}

impl Component for FileTree {
    fn handle_event(&mut self, event: &Event, ctx: &mut Context) -> EventResult {
        let key_event = match event {
            Event::Key(event) => *event,
            // The file tree is modal, so consume all other input events.
            Event::Paste(..) | Event::Resize(..) => return EventResult::Consumed(None),
            Event::Mouse(event) => return self.handle_mouse(event),
            _ => return EventResult::Ignored(None),
        };

        match key_event {
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
                return close_callback();
            }
            key!('r') => {
                self.tree.refresh(&self.tree.selected.clone());
            }
            _ => {}
        }
        EventResult::Consumed(None)
    }

    fn render(&mut self, area: Rect, surface: &mut Surface, ctx: &mut Context) {
        let theme = &ctx.editor.theme;
        let background = theme.get("ui.background");
        let directory_style = theme.get("ui.text.directory");
        let file_style = theme.get("ui.text");
        let selected_style = theme.get("ui.cursorline.primary");
        let window_style = theme.get("ui.window");

        // Leave the statusline and commandline rows to the editor underneath.
        let area = area.clip_bottom(2);
        let width = area.width.min(MAX_WIDTH);
        let tree_area = Rect {
            x: area.x,
            y: area.y,
            width,
            height: area.height,
        };
        self.area = tree_area;

        surface.clear_with(tree_area, background);

        let visible = self.tree.visible();
        let selected_index = self.tree.selected_index();
        let height = tree_area.height as usize;
        if height == 0 {
            return;
        }

        // Scroll to keep the selection visible.
        if selected_index < self.offset {
            self.offset = selected_index;
        } else if selected_index >= self.offset + height {
            self.offset = selected_index - height + 1;
        }

        let mut line = String::new();
        for (row, (entry, depth)) in visible.iter().enumerate().skip(self.offset).take(height) {
            let y = tree_area.y + (row - self.offset) as u16;
            let symbol = symbol_for(entry, self.icons);

            line.clear();
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
            let style = if row == selected_index {
                base_style.patch(selected_style)
            } else {
                base_style
            };
            surface.set_stringn(tree_area.x, y, &line, tree_area.width as usize, style);
        }

        // Draw a vertical separator between the tree and the editor.
        if tree_area.right() < area.right() {
            for y in tree_area.top()..tree_area.bottom() {
                surface[(tree_area.right(), y)]
                    .set_symbol(tui::symbols::line::VERTICAL)
                    .set_style(window_style);
            }
        }
    }

    fn cursor(&self, _area: Rect, _ctx: &Editor) -> (Option<helix_core::Position>, CursorKind) {
        (None, CursorKind::Hidden)
    }

    fn id(&self) -> Option<&'static str> {
        Some(ID)
    }
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
                .join(format!("hx-file-tree-test-{}-{name}", std::process::id()));
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

    /// The paths of the visible entries, relative to the tree root.
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
            })
            .collect()
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
        assert!(visible
            .iter()
            .any(|(entry, depth)| { entry.path == file && *depth == 3 }));
        assert_eq!(tree.selected, file);
        assert_eq!(tree.selected_index(), visible.len() - 1);
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
        assert_eq!(tree.selected_index(), 1);
    }

    #[test]
    fn symbols_switch_between_icons_and_arrows() {
        let mut dir = TreeEntry::new(PathBuf::from("dir"), true);
        let file = TreeEntry::new(PathBuf::from("f.xyz"), false);

        // Without icons: ASCII arrows for directories, blank for files.
        assert_eq!(symbol_for(&dir, false), ARROW_COLLAPSED);
        dir.expanded = true;
        assert_eq!(symbol_for(&dir, false), ARROW_EXPANDED);
        assert_eq!(symbol_for(&file, false), " ");

        // With icons: VS Code style folder icons (open/closed) and a file icon.
        assert_eq!(symbol_for(&dir, true), ICON_FOLDER_OPEN);
        dir.expanded = false;
        assert_eq!(symbol_for(&dir, true), ICON_FOLDER);
        assert_eq!(symbol_for(&file, true), ICON_FILE);
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
}
