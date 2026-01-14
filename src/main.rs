use std::{
    collections::HashMap,
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use chrono::Utc;

use notify::{EventKind, RecursiveMode, Watcher};

use p2p::{ConnectionManager, receive_message_from_peer, send_message_to_peer};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use colored::Colorize;
use common::fs::calculate_file_hash;
use log::{elog, log};

use synclite::network::{
    generate_peer_id, mdns,
    messages::{
        PeerMessage, ServerMessage,
        handlers::{peer, server},
    },
    peer_registry::{acknowledge_peer, broadcast_peer_list},
};
use workspace::{
    WORKSPACE_DIR,
    sync::{FileEntry, determine_winning_files},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ascii_art = r#"
                            .__  .__  __          
  _________.__. ____   ____ |  | |__|/  |_  ____  
 /  ___<   |  |/    \_/ ___\|  | |  \   __\/ __ \ 
 \___ \ \___  |   |  \  \___|  |_|  ||  | \  ___/ 
/____  >/ ____|___|  /\___  >____/__||__|  \___  >
     \/ \/         \/     \/                   \/                      
"#;
    println!("{}", ascii_art.bright_yellow().bold());

    dsl_cli::cli! {
        name "SyncLite",
        version "0.1.0",
        description "Real-time peer-to-peer folder sync for your local network (LAN) from the command line—sync changes instantly over TCP with multi-peer support, SHA-256 integrity checks, and proper delete tracking.",

        cmd serve "Start a server that watches a directory and allows peers to connect." {
            arg workspace "The workspace directory to serve.": Option<PathBuf> = ".",
            opt "-p, --port" "The port to serve on." {
                arg number: u16 = 8080,
            }
        }
        cmd connect "Connect to a server and keep your directory in sync." {
            arg workspace "The workspace directory to connect to.": Option<PathBuf> = ".",
            opt "-p, --port" "The port to connect to." {
                arg number: u16 = 8080,
            }
        }
    }

    // Logic
    match parsed {
        Command::Serve(parsed_args, parsed_opts) => {
            let (sync_manager, peers_config) = synclite::setup(parsed_args.workspace)?;
            let connection_manager = Arc::new(ConnectionManager::new());

            let port = parsed_opts.port;

            let leader_id = generate_peer_id();
            let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
            let listener = TcpListener::bind(addr).await?;

            // Flag to ignore file watcher events when program is making changes
            let ignore_file_events = Arc::new(AtomicBool::new(false));

            // Set this peer as the leader
            if let Err(e) = peers_config.set_leader(leader_id.clone()).await {
                elog!(error, "Failed to set leader: {}", e);
            }

            // Publish mDNS service
            if let Err(e) = mdns::advertise(port) {
                elog!(error, "Failed to advertise mDNS service: {}", e);
            }

            // Separate connection logs from the rest of the logs for clarity
            println!("\n{}\n", "-=".repeat(40).black().bold());
            log!(wrench, "Listening on: {}", addr.to_string());
            log!(info, "Leader ID: {}", leader_id);
            println!("\n{}\n", "-=".repeat(40).black().bold());

            // ===== FILE WATCHER TASK (SERVER) =====
            // Spawn a background task to watch the workspace directory for changes
            // This runs independently of the connection handling
            {
                let sync_manager = Arc::clone(&sync_manager);
                let connection_manager = Arc::clone(&connection_manager);
                let ignore_file_events = Arc::clone(&ignore_file_events);

                tokio::spawn(async move {
                    // Create a tokio channel for async communication
                    let (tx, mut rx) = mpsc::channel::<notify::Event>(100);

                    // Create a custom event handler that sends to the tokio channel
                    let event_handler = move |res: notify::Result<notify::Event>| {
                        if let Ok(event) = res {
                            // Use blocking_send since notify runs in sync context
                            let _ = tx.blocking_send(event);
                        }
                    };

                    // Create watcher with our custom event handler
                    let mut watcher = notify::recommended_watcher(event_handler).unwrap();
                    watcher
                        .watch(&*WORKSPACE_DIR, RecursiveMode::Recursive)
                        .unwrap();

                    // Loop to handle file system events
                    loop {
                        // First event in a burst
                        let Some(first_event) = rx.recv().await else {
                            continue;
                        };

                        // Skip processing if we're currently making programmatic changes
                        if ignore_file_events.load(Ordering::Relaxed) {
                            continue;
                        }

                        // Wait to absorb additional events
                        tokio::time::sleep(Duration::from_millis(150)).await;

                        // Collect all events that arrived during/after the sleep
                        let mut events = vec![first_event];
                        while let Ok(event) = rx.try_recv() {
                            events.push(event);
                        }

                        // Group by file path, collecting ALL event kinds for each path
                        let mut grouped: HashMap<String, Vec<EventKind>> = HashMap::new();

                        for event in events {
                            for path in event.paths {
                                if let Some(p) = path.to_str() {
                                    grouped
                                        .entry(p.to_string())
                                        .or_insert_with(Vec::new)
                                        .push(event.kind.clone());
                                }
                            }
                        }

                        let mut files_to_update: HashMap<String, Vec<u8>> = HashMap::new();
                        let mut paths_to_delete: Vec<String> = Vec::new();

                        // Now handle each file **once** based on event history and current state
                        for (path, event_kinds) in grouped {
                            let path_buf = PathBuf::from(&path);

                            // Skip .synclite directory
                            let is_synclite_dir = path.contains(".synclite");
                            if is_synclite_dir {
                                continue;
                            }

                            // Calculate relative path - skip if path is not within workspace
                            let relative_path = match path_buf.strip_prefix(&*WORKSPACE_DIR) {
                                Ok(rel) => rel.to_str().unwrap().to_string(),
                                Err(_) => continue, // Path not within workspace
                            };

                            // Check actual file system state
                            let file_exists = path_buf.exists();

                            // Handle Directory Logic
                            if file_exists && path_buf.is_dir() {
                                // Check if it's a Create event (which happens on directory move/copy)
                                let has_create = event_kinds.iter().any(|k| {
                                    matches!(k, EventKind::Create(_))
                                        || matches!(
                                            k,
                                            EventKind::Modify(notify::event::ModifyKind::Name(
                                                notify::event::RenameMode::To
                                            ))
                                        )
                                });

                                if has_create {
                                    // It's a directory creation/move! Scan it recursively.
                                    let new_files = sync_manager
                                        .scan_and_add_directory(&*WORKSPACE_DIR, &relative_path)
                                        .await;
                                    files_to_update.extend(new_files);
                                }
                                // Skip regular processing for directories
                                continue;
                            }

                            // Analyze event history
                            let has_create = event_kinds
                                .iter()
                                .any(|k| matches!(k, EventKind::Create(_)));
                            let has_remove = event_kinds.iter().any(|k| {
                                matches!(k, EventKind::Remove(_))
                                    || matches!(
                                        k,
                                        EventKind::Modify(notify::event::ModifyKind::Name(
                                            notify::event::RenameMode::From
                                        ))
                                    )
                            });
                            let has_modify = event_kinds
                                .iter()
                                .any(|k| matches!(k, EventKind::Modify(_)));

                            // Determine action based on event history and current state
                            match (file_exists, has_create, has_remove, has_modify) {
                                // File exists, saw both Create and Remove -> atomic write, treat as modify
                                (true, true, true, _) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) =
                                            sync_manager.update_file(&relative_path, hash).await
                                        {
                                            elog!(
                                                log,
                                                "Failed to update file {}: {}",
                                                relative_path,
                                                e
                                            );
                                        }
                                    }
                                    files_to_update.insert(
                                        relative_path.clone(),
                                        fs::read(&path_buf).unwrap(),
                                    );
                                }
                                // File exists, saw Create but no Remove -> new file
                                (true, true, false, _) => {
                                    if let Err(e) = sync_manager
                                        .add_file(
                                            relative_path.clone(),
                                            FileEntry {
                                                hash: Some(calculate_file_hash(&path_buf).unwrap()),
                                                is_deleted: false,
                                                last_modified: Utc::now(),
                                            },
                                        )
                                        .await
                                    {
                                        elog!(log, "Failed to add file {}: {}", relative_path, e);
                                    }
                                    files_to_update.insert(
                                        relative_path.clone(),
                                        fs::read(&path_buf).unwrap(),
                                    );
                                }
                                // File exists, no Create event -> modification
                                (true, false, _, true) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) =
                                            sync_manager.update_file(&relative_path, hash).await
                                        {
                                            elog!(
                                                log,
                                                "Failed to update file {}: {}",
                                                relative_path,
                                                e
                                            );
                                        }
                                    }
                                    files_to_update.insert(
                                        relative_path.clone(),
                                        fs::read(&path_buf).unwrap(),
                                    );
                                }
                                // File doesn't exist, saw Remove -> delete (could be file or directory)
                                (false, _, true, _) => {
                                    // Try recursive delete (handles both files and directories)
                                    let _ = sync_manager
                                        .delete_directory_recursive(&relative_path)
                                        .await;

                                    paths_to_delete.push(relative_path.clone());
                                }
                                // Any other case -> no action needed
                                _ => {}
                            }
                        }
                        // Broadcast the file updates to all peers
                        if !files_to_update.is_empty() || !paths_to_delete.is_empty() {
                            log!(
                                log,
                                "📡 Broadcasting {} files and {} deletions to peers",
                                files_to_update.len(),
                                paths_to_delete.len()
                            );
                        }

                        if !files_to_update.is_empty() || !paths_to_delete.is_empty() {
                            connection_manager
                                .broadcast_message(&ServerMessage::FileUpdatePush {
                                    files_to_write: files_to_update,
                                    paths_to_delete,
                                })
                                .await;
                        }
                    }
                });
            }

            // ===== CONNECTION HANDLER (SERVER) =====
            while let Ok((stream, peer_addr)) = listener.accept().await {
                let peers_config = Arc::clone(&peers_config);
                let sync_manager = Arc::clone(&sync_manager);
                let connection_manager = Arc::clone(&connection_manager);
                let leader_id = leader_id.clone();
                let ignore_file_events = Arc::clone(&ignore_file_events);

                tokio::spawn(async move {
                    let (mut reader, writer) = stream.into_split();

                    // Generate peer ID
                    let peer_id = generate_peer_id();
                    log!(log, "New peer connecting: {} ({})", peer_id, peer_addr);

                    // Add peer to connection manager
                    connection_manager
                        .add_connection(peer_id.clone(), writer)
                        .await;

                    // Acknowledge peer connection
                    if let Err(e) = acknowledge_peer(
                        Arc::clone(&connection_manager),
                        peer_id.clone(),
                        leader_id.clone(),
                    )
                    .await
                    {
                        elog!(log, "Failed to acknowledge peer: {}", e);
                    }

                    // Add peer to peers config
                    if let Err(e) = peers_config.add_peer(peer_id.clone()).await {
                        elog!(log, "Failed to add peer to config: {}", e);
                    }

                    // Broadcast peer config to all other peers
                    if let Err(failed_peers) = broadcast_peer_list(
                        Arc::clone(&connection_manager),
                        peers_config.state().await.peers.clone(),
                    )
                    .await
                    {
                        elog!(
                            log,
                            "Failed to broadcast peer list to peers: {:?}",
                            failed_peers
                        );
                    }

                    // ===== MESSAGE HANDLER (SERVER) =====
                    // Handle incoming messages from this peer
                    loop {
                        match receive_message_from_peer::<PeerMessage>(&mut reader).await {
                            Ok(message) => {
                                match message {
                                    // STEP 1: Peer sends their version
                                    PeerMessage::InitialSyncPush {
                                        sync_state: peer_sync_state,
                                    } => {
                                        if let Err(e) = server::handle_initial_sync_push(
                                            &connection_manager,
                                            &sync_manager,
                                            &ignore_file_events,
                                            &peer_id,
                                            &peer_sync_state,
                                        )
                                        .await
                                        {
                                            elog!(
                                                log,
                                                "Failed to handle initial sync from {}: {}",
                                                peer_id,
                                                e
                                            );
                                        }
                                    }
                                    PeerMessage::FileUpdatePush {
                                        files_to_write,
                                        paths_to_delete,
                                    } => {
                                        if let Err(e) = server::handle_file_update_push(
                                            &connection_manager,
                                            &sync_manager,
                                            &ignore_file_events,
                                            &peer_id,
                                            &files_to_write,
                                            &paths_to_delete,
                                        )
                                        .await
                                        {
                                            elog!(
                                                log,
                                                "Failed to handle file updates from {}: {}",
                                                peer_id,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    // Remove peer from connection manager and peers config when connection is lost
                    log!(log, "Peer {} disconnected", peer_id);

                    // Remove peer from connection manager
                    connection_manager.remove_connection(&peer_id).await;

                    // Remove peer from peers config
                    if let Err(e) = peers_config.remove_peer(&peer_id).await {
                        elog!(log, "Failed to remove peer {} from config: {}", peer_id, e);
                    }

                    // Broadcast peer config to all other peers
                    if let Err(failed_peers) = broadcast_peer_list(
                        connection_manager,
                        peers_config.state().await.peers.clone(),
                    )
                    .await
                    {
                        elog!(
                            log,
                            "Failed to broadcast peer list to peers: {:?}",
                            failed_peers
                        );
                    }
                });
            }
        }
        Command::Connect(parsed_args, parsed_opts) => {
            let (sync_manager, peers_config) = synclite::setup(parsed_args.workspace)?;

            let addr = mdns::browse(parsed_opts.port).await?;

            let Ok(stream) = TcpStream::connect(addr).await else {
                elog!(error, "Failed to connect to: {}", addr.to_string());
                std::process::exit(1);
            };

            let (mut reader, mut writer) = stream.into_split();

            let ServerMessage::ConnectionAck { peer_id, leader_id } =
                receive_message_from_peer::<ServerMessage>(&mut reader).await?
            else {
                elog!(
                    error,
                    "Failed to receive connection acknowledgment from leader"
                );
                std::process::exit(1);
            };

            // Separate connection logs from the rest of the logs for clarity
            println!("\n{}\n", "-=".repeat(40).black().bold());
            log!(wrench, "Connected to: {}", addr.to_string());
            log!(info, "Peer ID: {}", peer_id);
            println!("\n{}\n", "-=".repeat(40).black().bold());

            // Update peers config to set the leader
            if let Err(e) = peers_config.set_leader(leader_id).await {
                elog!(error, "Failed to set leader: {}", e);
            }

            // ===== INITIAL SYNC (PEER) =====
            // Send our current sync state to the server immediately after connecting
            {
                log!(log, "Sending version to server");
                let sync_state = sync_manager.state().await;
                let message = PeerMessage::InitialSyncPush { sync_state };

                if let Err(e) = send_message_to_peer(&mut writer, &message).await {
                    elog!(log, "Failed to send initial sync state to server: {}", e);
                    std::process::exit(1);
                }
            }

            // ===== FILE WATCHER TASK (PEER) =====
            // Spawn a background task to watch the workspace directory for changes
            let ignore_file_events = Arc::new(AtomicBool::new(false));

            // Create a channel for the file watcher to send messages to the main connection handler
            let (file_change_tx, mut file_change_rx) = mpsc::channel::<PeerMessage>(100);

            {
                let sync_manager = Arc::clone(&sync_manager);
                let ignore_file_events = Arc::clone(&ignore_file_events);

                tokio::spawn(async move {
                    // Create a tokio channel for async communication
                    let (tx, mut rx) = mpsc::channel::<notify::Event>(100);

                    // Create a custom event handler that sends to the tokio channel
                    let event_handler = move |res: notify::Result<notify::Event>| {
                        if let Ok(event) = res {
                            // Use blocking_send since notify runs in sync context
                            let _ = tx.blocking_send(event);
                        }
                    };

                    // Create watcher with our custom event handler
                    let mut watcher = notify::recommended_watcher(event_handler).unwrap();
                    watcher
                        .watch(&*WORKSPACE_DIR, RecursiveMode::Recursive)
                        .unwrap();

                    // Loop to handle file system events
                    loop {
                        // First event in a burst
                        let Some(first_event) = rx.recv().await else {
                            continue;
                        };

                        // Skip processing if we're currently making programmatic changes
                        if ignore_file_events.load(Ordering::Relaxed) {
                            continue;
                        }

                        // Wait to absorb additional events
                        tokio::time::sleep(Duration::from_millis(150)).await;

                        // Collect all events that arrived during/after the sleep
                        let mut events = vec![first_event];
                        while let Ok(event) = rx.try_recv() {
                            events.push(event);
                        }

                        // Group by file path, collecting ALL event kinds for each path
                        let mut grouped: HashMap<String, Vec<EventKind>> = HashMap::new();

                        for event in events {
                            for path in event.paths {
                                if let Some(p) = path.to_str() {
                                    grouped
                                        .entry(p.to_string())
                                        .or_insert_with(Vec::new)
                                        .push(event.kind.clone());
                                }
                            }
                        }

                        let mut files_to_update: HashMap<String, Vec<u8>> = HashMap::new();
                        let mut paths_to_delete: Vec<String> = Vec::new();

                        // Now handle each file **once** based on event history and current state
                        for (path, event_kinds) in grouped {
                            let path_buf = PathBuf::from(&path);

                            // Skip .synclite directory
                            let is_synclite_dir = path.contains(".synclite");
                            if is_synclite_dir {
                                continue;
                            }

                            // Skip directories
                            // Calculate relative path - skip if path is not within workspace
                            let relative_path = match path_buf.strip_prefix(&*WORKSPACE_DIR) {
                                Ok(rel) => rel.to_str().unwrap().to_string(),
                                Err(_) => continue, // Path not within workspace
                            };

                            // Check actual file system state
                            let file_exists = path_buf.exists();

                            // Handle Directory Logic
                            if file_exists && path_buf.is_dir() {
                                // Check if it's a Create event
                                let has_create = event_kinds.iter().any(|k| {
                                    matches!(k, EventKind::Create(_))
                                        || matches!(
                                            k,
                                            EventKind::Modify(notify::event::ModifyKind::Name(
                                                notify::event::RenameMode::To
                                            ))
                                        )
                                });

                                if has_create {
                                    // It's a directory creation/move! Scan it recursively.
                                    let new_files = sync_manager
                                        .scan_and_add_directory(&*WORKSPACE_DIR, &relative_path)
                                        .await;
                                    files_to_update.extend(new_files);
                                }
                                // Skip regular processing for directories
                                continue;
                            }

                            // Analyze event history
                            let has_create = event_kinds
                                .iter()
                                .any(|k| matches!(k, EventKind::Create(_)));
                            let has_remove = event_kinds.iter().any(|k| {
                                matches!(k, EventKind::Remove(_))
                                    || matches!(
                                        k,
                                        EventKind::Modify(notify::event::ModifyKind::Name(
                                            notify::event::RenameMode::From
                                        ))
                                    )
                            });
                            let has_modify = event_kinds
                                .iter()
                                .any(|k| matches!(k, EventKind::Modify(_)));

                            // Determine action based on event history and current state
                            match (file_exists, has_create, has_remove, has_modify) {
                                // File exists, saw both Create and Remove -> atomic write, treat as modify
                                (true, true, true, _) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) =
                                            sync_manager.update_file(&relative_path, hash).await
                                        {
                                            elog!(
                                                log,
                                                "Failed to update file {}: {}",
                                                relative_path,
                                                e
                                            );
                                        }
                                    }
                                    if let Ok(content) = fs::read(&path_buf) {
                                        files_to_update.insert(relative_path.clone(), content);
                                    }
                                }
                                // File exists, saw Create but no Remove -> new file
                                (true, true, false, _) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) = sync_manager
                                            .add_file(
                                                relative_path.clone(),
                                                FileEntry {
                                                    hash: Some(hash),
                                                    is_deleted: false,
                                                    last_modified: Utc::now(),
                                                },
                                            )
                                            .await
                                        {
                                            elog!(
                                                log,
                                                "Failed to add file {}: {}",
                                                relative_path,
                                                e
                                            );
                                        }
                                    }
                                    if let Ok(content) = fs::read(&path_buf) {
                                        files_to_update.insert(relative_path.clone(), content);
                                    }
                                }
                                // File exists, no Create event -> modification
                                (true, false, _, true) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) =
                                            sync_manager.update_file(&relative_path, hash).await
                                        {
                                            elog!(
                                                log,
                                                "Failed to update file {}: {}",
                                                relative_path,
                                                e
                                            );
                                        }
                                    }
                                    if let Ok(content) = fs::read(&path_buf) {
                                        files_to_update.insert(relative_path.clone(), content);
                                    }
                                }
                                // File doesn't exist, saw Remove -> delete (could be file or directory)
                                (false, _, true, _) => {
                                    // Try recursive delete (handles both files and directories)
                                    let _ = sync_manager
                                        .delete_directory_recursive(&relative_path)
                                        .await;

                                    paths_to_delete.push(relative_path.clone());
                                }
                                // Any other case -> no action needed
                                _ => {}
                            }
                        }

                        // Send the file updates to the main connection handler via channel
                        if !files_to_update.is_empty() || !paths_to_delete.is_empty() {
                            log!(
                                log,
                                "📡 Sending to server: {} files, {} deletions",
                                files_to_update.len(),
                                paths_to_delete.len()
                            );
                        }

                        if !files_to_update.is_empty() || !paths_to_delete.is_empty() {
                            let _ = file_change_tx
                                .send(PeerMessage::FileUpdatePush {
                                    files_to_write: files_to_update,
                                    paths_to_delete,
                                })
                                .await;
                        }
                    }
                });
            }

            // ===== MESSAGE HANDLER (PEER) =====
            // Listen for messages from the server AND file watcher changes
            loop {
                tokio::select! {
                    // Handle incoming messages from server
                    server_msg = receive_message_from_peer::<ServerMessage>(&mut reader) => {
                        match server_msg {
                            Ok(message) => {
                                match message {
                                    ServerMessage::PeerListUpdate { peers } => {
                                        if let Err(e) =
                                            peer::handle_peer_list_update(&peers_config, peers)
                                                .await
                                        {
                                            elog!(log, "Failed to handle peer list update: {}", e);
                                        }
                                    }

                                    ServerMessage::InitialSyncPushResponse {
                                        files_to_update,
                                        files_to_delete,
                                        files_to_send_back,
                                    } => {
                                        if let Err(e) = peer::handle_initial_sync_push_response(
                                            &sync_manager,
                                            &ignore_file_events,
                                            &mut writer,
                                            files_to_update,
                                            files_to_delete,
                                            files_to_send_back,
                                        )
                                        .await
                                        {
                                            elog!(
                                                log,
                                                "Failed to handle initial sync response: {}",
                                                e
                                            );
                                        }
                                    }

                                    ServerMessage::FileUpdatePush {
                                        files_to_write,
                                        paths_to_delete,
                                    } => {
                                        if let Err(e) = peer::handle_file_update_push(
                                            &sync_manager,
                                            &ignore_file_events,
                                            files_to_write,
                                            paths_to_delete,
                                        )
                                        .await
                                        {
                                            elog!(log, "Failed to handle file update push: {}", e);
                                        }
                                    }

                                    _ => {
                                        log!(log, "Received message: {:?}", message);
                                    }
                                }
                            }
                            Err(e) => {
                                elog!(log, "Connection to leader lost: {}", e);
                                break;
                            }
                        }
                    }
                    // Handle file changes from the file watcher
                    Some(file_change_msg) = file_change_rx.recv() => {
                        // Send file change to server
                        if let Err(e) = send_message_to_peer(&mut writer, &file_change_msg).await {
                            elog!(log, "Failed to send file changes to server: {}", e);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
