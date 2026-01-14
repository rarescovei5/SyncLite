use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use chrono::Utc;
use notify::{
    Event, EventKind, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use tokio::sync::mpsc;

use common::fs::calculate_file_hash;
use log::{elog, log};
use p2p::ConnectionManager;
use workspace::{
    WORKSPACE_DIR,
    sync::{FileEntry, SyncManager},
};

use synclite::network::messages::{PeerMessage, ServerMessage};

pub enum WatcherMode {
    /// We are the leader: broadcast changes to all connected peers.
    Server {
        connection_manager: Arc<ConnectionManager>,
    },
    /// We are a peer: send changes to the main connection loop via channel.
    Peer {
        file_change_tx: mpsc::Sender<PeerMessage>,
    },
}

#[derive(Default)]
struct FileDelta {
    files_to_write: HashMap<String, Vec<u8>>,
    paths_to_delete: Vec<String>,
}

#[derive(Debug, Clone)]
enum ClassifiedEvent {
    /// Move/rename (We treat this as delete(from) + create(to))
    Move { from: PathBuf, to: PathBuf },
    /// File created
    Create { path: PathBuf },
    /// File modified
    Modify { path: PathBuf },
    /// File or directory deleted
    Delete { path: PathBuf },
    /// Directory created/moved into place, we scan it and emit all its files
    CreateDir { path: PathBuf },
}

pub fn spawn_file_watcher(
    mode: WatcherMode,
    sync_manager: Arc<SyncManager>,
    ignore_file_events: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::channel::<notify::Event>(100);

        let event_handler = move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        };

        let mut watcher = match notify::recommended_watcher(event_handler) {
            Ok(w) => w,
            Err(e) => {
                elog!(log, "Failed to create watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&*WORKSPACE_DIR, RecursiveMode::Recursive) {
            elog!(log, "Failed to watch workspace dir: {}", e);
            return;
        }

        loop {
            let Some(first_event) = rx.recv().await else {
                continue;
            };

            if ignore_file_events.load(Ordering::Relaxed) {
                continue;
            }

            tokio::time::sleep(Duration::from_millis(150)).await;

            let mut events = vec![first_event];
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }

            let classified = classify_events(events);
            let delta = handle_classified(&sync_manager, classified).await;

            if delta.files_to_write.is_empty() && delta.paths_to_delete.is_empty() {
                continue;
            }

            match &mode {
                WatcherMode::Server { .. } => {
                    log!(
                        log,
                        "📡 Broadcasting {} files and {} deletions to peers",
                        delta.files_to_write.len(),
                        delta.paths_to_delete.len()
                    );
                }
                WatcherMode::Peer { .. } => {
                    log!(
                        log,
                        "📡 Sending to server: {} files, {} deletions",
                        delta.files_to_write.len(),
                        delta.paths_to_delete.len()
                    );
                }
            }

            match &mode {
                WatcherMode::Server { connection_manager } => {
                    connection_manager
                        .broadcast_message(&ServerMessage::FileUpdatePush {
                            files_to_write: delta.files_to_write,
                            paths_to_delete: delta.paths_to_delete,
                        })
                        .await;
                }
                WatcherMode::Peer { file_change_tx } => {
                    let _ = file_change_tx
                        .send(PeerMessage::FileUpdatePush {
                            files_to_write: delta.files_to_write,
                            paths_to_delete: delta.paths_to_delete,
                        })
                        .await;
                }
            }
        }
    })
}

fn classify_events(events: Vec<Event>) -> Vec<ClassifiedEvent> {
    // 1) Extract moves/renames.
    // - If we get a single event with 2 paths and a Name modify, treat it as Move(from,to).
    // - Otherwise, pair RenameMode::From with RenameMode::To in the order observed.
    let mut moves: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut move_paths: HashSet<PathBuf> = HashSet::new();

    let mut rename_from: Vec<PathBuf> = Vec::new();
    let mut rename_to: Vec<PathBuf> = Vec::new();

    for ev in &events {
        if matches!(ev.kind, EventKind::Modify(ModifyKind::Name(_))) {
            if ev.paths.len() == 2 {
                let from = ev.paths[0].clone();
                let to = ev.paths[1].clone();
                move_paths.insert(from.clone());
                move_paths.insert(to.clone());
                moves.push((from, to));
            } else if ev.paths.len() == 1 {
                match ev.kind {
                    EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                        rename_from.push(ev.paths[0].clone());
                    }
                    EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                        rename_to.push(ev.paths[0].clone());
                    }
                    _ => {}
                }
            }
        }
    }

    // Pair From/To by order (best-effort).
    let pair_count = std::cmp::min(rename_from.len(), rename_to.len());
    for i in 0..pair_count {
        let from = rename_from[i].clone();
        let to = rename_to[i].clone();
        move_paths.insert(from.clone());
        move_paths.insert(to.clone());
        moves.push((from, to));
    }

    // 2) Group remaining kinds per path to derive Create/Modify/Delete/CreateDir.
    let mut grouped: HashMap<PathBuf, Vec<EventKind>> = HashMap::new();
    for ev in events {
        for path in ev.paths {
            if move_paths.contains(&path) {
                continue;
            }
            grouped
                .entry(path)
                .or_insert_with(Vec::new)
                .push(ev.kind.clone());
        }
    }

    let mut out: Vec<ClassifiedEvent> = Vec::new();

    for (from, to) in moves {
        out.push(ClassifiedEvent::Move { from, to });
    }

    for (path, kinds) in grouped {
        // Determine action based on observed kinds + current filesystem state.
        let exists = path.exists();
        let is_dir = exists && path.is_dir();

        // Directory create/move into place: scan it and emit all contained files.
        if is_dir {
            let has_create = kinds.iter().any(|k| {
                matches!(k, EventKind::Create(_))
                    || matches!(k, EventKind::Modify(ModifyKind::Name(RenameMode::To)))
            });
            if has_create {
                out.push(ClassifiedEvent::CreateDir { path });
            }
            continue;
        }

        let has_create = kinds.iter().any(|k| matches!(k, EventKind::Create(_)));
        let has_remove = kinds.iter().any(|k| {
            matches!(k, EventKind::Remove(_))
                || matches!(k, EventKind::Modify(ModifyKind::Name(RenameMode::From)))
        });
        let has_modify = kinds.iter().any(|k| matches!(k, EventKind::Modify(_)));

        // These are the 4 buckets you asked for.
        if has_remove && !exists {
            out.push(ClassifiedEvent::Delete { path });
            continue;
        }

        // create+remove in same burst (atomic write) -> treat as Modify
        if exists && (has_modify || (has_create && has_remove)) {
            out.push(ClassifiedEvent::Modify { path });
            continue;
        }

        if exists && has_create {
            out.push(ClassifiedEvent::Create { path });
            continue;
        }

        if exists && has_modify {
            out.push(ClassifiedEvent::Modify { path });
            continue;
        }
    }

    out
}

async fn handle_classified(
    sync_manager: &SyncManager,
    classified: Vec<ClassifiedEvent>,
) -> FileDelta {
    let mut delta = FileDelta::default();

    for ev in classified {
        match ev {
            ClassifiedEvent::Move { from, to } => {
                handle_move(sync_manager, &mut delta, &from, &to).await;
            }
            ClassifiedEvent::Create { path } => {
                handle_create_or_modify(sync_manager, &mut delta, &path, true).await;
            }
            ClassifiedEvent::Modify { path } => {
                handle_create_or_modify(sync_manager, &mut delta, &path, false).await;
            }
            ClassifiedEvent::Delete { path } => {
                handle_delete(sync_manager, &mut delta, &path).await;
            }
            ClassifiedEvent::CreateDir { path } => {
                handle_create_dir(sync_manager, &mut delta, &path).await;
            }
        }
    }

    delta
}

async fn handle_move(
    sync_manager: &SyncManager,
    delta: &mut FileDelta,
    from_abs: &PathBuf,
    to_abs: &PathBuf,
) {
    let from_str = from_abs.to_string_lossy();
    let to_str = to_abs.to_string_lossy();
    if from_str.contains(".synclite") || to_str.contains(".synclite") {
        return;
    }

    let Ok(from_rel) = from_abs.strip_prefix(&*WORKSPACE_DIR) else {
        return;
    };
    let Ok(to_rel) = to_abs.strip_prefix(&*WORKSPACE_DIR) else {
        return;
    };

    let from_rel_str = from_rel.to_string_lossy().to_string();
    let to_rel_str = to_rel.to_string_lossy().to_string();

    let _ = sync_manager.delete_directory_recursive(&from_rel_str).await;
    delta.paths_to_delete.push(from_rel_str);

    if to_abs.exists() && to_abs.is_dir() {
        let new_files = sync_manager
            .scan_and_add_directory(&*WORKSPACE_DIR, &to_rel_str)
            .await;
        delta.files_to_write.extend(new_files);
        return;
    }

    if to_abs.exists() {
        if let Ok(hash) = calculate_file_hash(to_abs) {
            let _ = sync_manager
                .add_file(
                    to_rel_str.clone(),
                    FileEntry {
                        hash: Some(hash),
                        is_deleted: false,
                        last_modified: Utc::now(),
                    },
                )
                .await;
        }

        if let Ok(content) = fs::read(to_abs) {
            delta.files_to_write.insert(to_rel_str, content);
        }
    }
}

async fn handle_delete(sync_manager: &SyncManager, delta: &mut FileDelta, abs_path: &PathBuf) {
    let path_str = abs_path.to_string_lossy();
    if path_str.contains(".synclite") {
        return;
    }

    let Ok(relative_path) = abs_path.strip_prefix(&*WORKSPACE_DIR) else {
        return;
    };
    let relative_path = relative_path.to_string_lossy().to_string();

    let _ = sync_manager
        .delete_directory_recursive(&relative_path)
        .await;
    delta.paths_to_delete.push(relative_path);
}

async fn handle_create_dir(sync_manager: &SyncManager, delta: &mut FileDelta, abs_path: &PathBuf) {
    let path_str = abs_path.to_string_lossy();
    if path_str.contains(".synclite") {
        return;
    }

    let Ok(relative_path) = abs_path.strip_prefix(&*WORKSPACE_DIR) else {
        return;
    };
    let relative_path = relative_path.to_string_lossy().to_string();

    if abs_path.exists() && abs_path.is_dir() {
        let new_files = sync_manager
            .scan_and_add_directory(&*WORKSPACE_DIR, &relative_path)
            .await;
        delta.files_to_write.extend(new_files);
    }
}

async fn handle_create_or_modify(
    sync_manager: &SyncManager,
    delta: &mut FileDelta,
    abs_path: &PathBuf,
    is_create: bool,
) {
    let path_str = abs_path.to_string_lossy();
    if path_str.contains(".synclite") {
        return;
    }

    let Ok(relative_path) = abs_path.strip_prefix(&*WORKSPACE_DIR) else {
        return;
    };
    let relative_path = relative_path.to_string_lossy().to_string();

    if !abs_path.exists() || abs_path.is_dir() {
        return;
    }

    if let Ok(hash) = calculate_file_hash(abs_path) {
        if is_create {
            let _ = sync_manager
                .add_file(
                    relative_path.clone(),
                    FileEntry {
                        hash: Some(hash),
                        is_deleted: false,
                        last_modified: Utc::now(),
                    },
                )
                .await;
        } else {
            let _ = sync_manager.update_file(&relative_path, hash).await;
        }
    }

    if let Ok(content) = fs::read(abs_path) {
        delta.files_to_write.insert(relative_path, content);
    }
}
