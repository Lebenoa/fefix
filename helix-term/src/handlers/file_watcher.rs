//! Watches the files of open documents and reloads unmodified buffers when
//! they change on disk.
//!
//! A watcher thread owns the platform watcher (`notify`) plus a command
//! channel; raw fs events are forwarded into this hook, which debounces
//! bursts and reloads affected buffers that have no unsaved changes and
//! are not being edited in insert mode.
//! Watch-set maintenance rides on the existing `DocumentDidOpen` /
//! `DocumentDidClose` hooks.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

use helix_event::{register_hook, send_blocking, AsyncHook};
use helix_view::{
    document::Mode,
    events::{DocumentDidClose, DocumentDidOpen},
    Editor,
};
use notify::Watcher;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;

use crate::job;

/// Debounce window: editors and build tools often emit several fs events per
/// write; reload once per burst.
const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Debug)]
pub enum FileWatchEvent {
    /// A document with this file was opened: watch its directory.
    Watch(PathBuf),
    /// A document with this file was closed: unwatch its directory when the
    /// last document living in it closes.
    Unwatch(PathBuf),
    /// A file changed on disk (event from the notify thread).
    Changed(PathBuf),
}

fn fs_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

#[derive(Debug)]
pub(super) enum WatchCmd {
    Watch(PathBuf),
    Unwatch(PathBuf),
}

/// Matches raw fs events against the set of open document paths; surviving
/// paths are debounced and reloaded.
#[derive(Debug)]
pub(super) struct FileWatcher {
    /// Canonicalized paths of currently open documents.
    watched: HashSet<PathBuf>,
    /// Files changed since the current debounce window started.
    pending: HashSet<PathBuf>,
    /// Mtime of the last reload per path; events not newer than it are
    /// echoes of our own work on platforms that report them (e.g. PRoot).
    reloaded: HashMap<PathBuf, SystemTime>,
    /// Directory watch/unwatch commands for the watcher thread.
    cmd_tx: mpsc::Sender<WatchCmd>,
}

impl FileWatcher {
    pub(super) fn new(cmd_tx: mpsc::Sender<super::file_watcher::WatchCmd>) -> Self {
        Self {
            watched: HashSet::new(),
            pending: HashSet::new(),
            reloaded: HashMap::new(),
            cmd_tx,
        }
    }

    fn track(&mut self, path: &Path) {
        let canonical = helix_stdx::path::canonicalize(path);
        if self.watched.insert(canonical.clone()) {
            if let Some(dir) = canonical.parent().map(Path::to_path_buf) {
                let _ = self.cmd_tx.send(WatchCmd::Watch(dir));
            }
        }
    }

    fn untrack(&mut self, path: &Path) {
        let canonical = helix_stdx::path::canonicalize(path);
        self.reloaded.remove(&canonical);
        if self.watched.remove(&canonical) {
            if let Some(dir) = canonical.parent().map(Path::to_path_buf) {
                let _ = self.cmd_tx.send(WatchCmd::Unwatch(dir));
            }
        }
    }
}

impl AsyncHook for FileWatcher {
    type Event = FileWatchEvent;

    fn handle_event(&mut self, event: Self::Event, timeout: Option<Instant>) -> Option<Instant> {
        match event {
            FileWatchEvent::Changed(path) => {
                if self.watched.contains(&path) {
                    self.pending.insert(path);
                    Some(Instant::now() + DEBOUNCE)
                } else {
                    timeout
                }
            }
            FileWatchEvent::Watch(path) => {
                self.track(&path);
                timeout
            }
            FileWatchEvent::Unwatch(path) => {
                self.untrack(&path);
                timeout
            }
        }
    }

    fn finish_debounce(&mut self) {
        // Keep only paths whose mtime moved past our last reload of them.
        let paths: Vec<PathBuf> = std::mem::take(&mut self.pending)
            .into_iter()
            .filter(|path| match fs_mtime(path) {
                Some(mtime) => match self.reloaded.get(path) {
                    Some(last) => mtime > *last,
                    None => true,
                },
                None => false,
            })
            .collect();
        if paths.is_empty() {
            return;
        }
        let now = SystemTime::now();
        for path in &paths {
            self.reloaded.insert(path.clone(), now);
        }
        job::dispatch_blocking(move |editor, _| {
            reload_paths(editor, &paths);
        });
    }
}

/// Reload every open document whose file is in `paths` and has no unsaved
/// changes; buffers with modifications (or an in-progress insert) are left
/// alone.
fn reload_paths(editor: &mut Editor, paths: &[PathBuf]) {
    if !editor.config().auto_reload || editor.mode() == Mode::Insert {
        return;
    }

    for path in paths {
        let doc_ids: Vec<_> = editor
            .documents
            .values()
            .filter(|doc| doc.path().is_some_and(|p| p == path))
            .map(|doc| doc.id())
            .collect();

        for doc_id in doc_ids {
            let has_changes = editor.documents[&doc_id].is_modified();
            let workspace_root = editor.documents[&doc_id]
                .workspace_root()
                .to_path_buf();

            if has_changes {
                log::info!(
                    "file watcher: {path:?} has unsaved changes, not reloading"
                );
                continue;
            }

            let trust_full = editor
                .workspace_trust
                .query(
                    &workspace_root,
                    helix_loader::workspace_trust::TrustQuery::Git,
                )
                .is_trusted();

            // Every open document has at least one view (Editor::open calls
            // ensure_view_init), and reload commits history per view.
            let view_ids: Vec<_> = editor
                .documents
                .get(&doc_id)
                .unwrap()
                .selections()
                .keys()
                .copied()
                .collect();

            // sync_changes guarantees the view history matches the doc
            // before reload appends its transaction.
            let doc = editor.documents.get_mut(&doc_id).unwrap();
            let view = editor.tree.get_mut(view_ids[0]);
            if view.doc != doc_id {
                continue;
            }
            view.sync_changes(doc);
            let doc = editor.documents.get_mut(&doc_id).unwrap();
            let view = editor.tree.get_mut(view_ids[0]);
            if let Err(error) = doc.reload(view, &editor.diff_providers, trust_full) {
                log::warn!("file watcher: reload of {path:?} failed: {error}");
                continue;
            }

            editor
                .language_servers
                .file_event_handler
                .file_changed(path.clone());

            // Keep every other view onto this document consistent with the
            // reloaded history (mirrors :reload-all).
            for view_id in view_ids.iter().skip(1) {
                let view = editor.tree.get_mut(*view_id);
                if view.doc == doc_id {
                    let doc = editor.documents.get_mut(&doc_id).unwrap();
                    view.sync_changes(doc);
                }
            }
        }
    }
}

pub(super) fn register_hooks() {
    // The watcher thread owns the platform watcher; it applies watch/unwatch
    // commands and forwards raw events into the debounced hook.
    let (cmd_tx, cmd_rx) = mpsc::channel::<WatchCmd>();
    let (raw_tx, raw_rx) = mpsc::channel();

    let Ok(watcher) = notify::recommended_watcher(raw_tx) else {
        // Platform watcher creation failed; reload-on-change stays off but
        // the editor works without it.
        log::warn!("file watcher: could not create platform watcher");
        return;
    };

    let handler = FileWatcher::new(cmd_tx);
    let hook_tx = handler.spawn();
    let event_tx = hook_tx.clone();

    thread::Builder::new()
        .name("file-watcher".into())
        .spawn(move || watcher_loop(watcher, cmd_rx, raw_rx, event_tx))
        .expect("could not spawn file watcher thread");

    let tx = hook_tx.clone();
    register_hook!(move |event: &mut DocumentDidOpen<'_>| {
        let doc = &event.editor.documents[&event.doc];
        if let Some(path) = doc.path() {
            send_blocking(&tx, FileWatchEvent::Watch(path.to_owned()));
        }
        Ok(())
    });

    let tx = hook_tx;
    register_hook!(move |event: &mut DocumentDidClose<'_>| {
        if let Some(path) = event.doc.path() {
            send_blocking(&tx, FileWatchEvent::Unwatch(path.to_owned()));
        }
        Ok(())
    });
}

/// Watcher thread body: applies directory watch/unwatch commands from the
/// hook and forwards raw fs events into the hook channel. Both channels live
/// for the whole session.
fn watcher_loop(
    mut watcher: notify::RecommendedWatcher,
    cmd_rx: mpsc::Receiver<WatchCmd>,
    raw_rx: mpsc::Receiver<notify::Result<notify::Event>>,
    event_tx: Sender<FileWatchEvent>,
) {
    use std::sync::mpsc::RecvTimeoutError;

    loop {
        // Drain pending commands first.
        loop {
            match cmd_rx.try_recv() {
                Ok(WatchCmd::Watch(dir)) => {
                    if let Err(err) = watcher.watch(&dir, notify::RecursiveMode::NonRecursive) {
                        // Can legitimately fail for dirs that vanished.
                        log::debug!("file watcher: could not watch {dir:?}: {err}");
                    }
                }
                Ok(WatchCmd::Unwatch(dir)) => {
                    let _ = watcher.unwatch(&dir);
                }
                Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }

        match raw_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(event)) => {
                for path in event.paths {
                    // Never block here: bursts of fs events (and repeat
                    // events while a reload is pending) must not stall the
                    // watcher, and a dropped duplicate only delays a reload
                    // by one debounce window.
                    if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) =
                        event_tx.try_send(FileWatchEvent::Changed(path))
                    {
                        log::debug!("file watcher: event queue full, dropping");
                    }
                }
            }
            Ok(Err(err)) => log::warn!("file watcher error: {err}"),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
