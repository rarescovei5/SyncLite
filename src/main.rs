use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{net::SocketAddr, time::Duration};

use chrono::Utc;
use notify::{EventKind, RecursiveMode, Watcher};

use synclite::{
    cli::{self, types::Command},
    models::{FileEntry, PeersConfig, SyncConfig},
    network::{
        PeerConnectionManager, PeerMessage, ServerMessage, acknowledge_peer, broadcast_peer_list,
        generate_peer_id, receive_message_from_peer, send_message_to_peer,
    },
    sandboxed::FileSystem,
    storage::{initialise_state, initialise_storage},
    sync::{calculate_file_hash, determine_winning_files},
    utils::{output::CliOutput, read_peers_state, read_sync_state},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use colored::Colorize;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    CliOutput::banner();

    let Ok(args) = cli::parse_args() else {
        std::process::exit(0);
    };

    CliOutput::info(
        &format!(
            "Selected \"{:?}\" mode for: \"{}\"\n",
            args.command, args.path
        ),
        None,
    );

    let workspace_path = match Path::new(&args.path).canonicalize() {
        Ok(mut canonical_workspace) => {
            let canonical_str = canonical_workspace.to_string_lossy();
            if canonical_str.starts_with(r"\\?\") {
                canonical_workspace = PathBuf::from(&canonical_str[4..]);
            }
            canonical_workspace
        }
        Err(e) => {
            CliOutput::error(
                &format!("Failed to resolve workspace path '{}': {}", args.path, e),
                None,
            );
            std::process::exit(1);
        }
    };

    CliOutput::info(
        &format!("Resolved workspace path: {}", workspace_path.display()),
        None,
    );

    let storage_path = workspace_path.join(".synclite");

    // Initialise storage directory with peers and sync state files
    if let Err(e) = initialise_storage(&workspace_path).await {
        CliOutput::error(&format!("Failed to initialise storage: {}", e), None);
        std::process::exit(1);
    }
    print!("\n");

    // Initialise sync state by merging saved and computed states
    if let Err(e) = initialise_state(&workspace_path) {
        CliOutput::error(&format!("Failed to initialise state: {}", e), None);
        std::process::exit(1);
    }
    print!("\n");

    // Create sync and peers configs
    let sync_config = Arc::new(SyncConfig::new(
        &storage_path,
        read_sync_state(&storage_path),
    ));
    let peers_config = Arc::new(PeersConfig::new(
        &storage_path,
        read_peers_state(&storage_path),
    ));

    let workspace_file_system = Arc::new(FileSystem::new(workspace_path.clone()));

    match args.command {
        Command::Serve => {
            let leader_id = generate_peer_id();
            let addr: SocketAddr = format!("127.0.0.1:{}", args.port).parse().unwrap();
            let listener = TcpListener::bind(addr).await?;

            // Initialize connection manager
            let connection_manager = Arc::new(PeerConnectionManager::new());

            // Flag to ignore file watcher events when program is making changes
            let ignore_file_events = Arc::new(AtomicBool::new(false));

            // Set this peer as the leader
            if let Err(e) = peers_config.set_leader(leader_id.clone()).await {
                CliOutput::error(&format!("Failed to set leader: {}", e), None);
            }

            // Separate connection logs from the rest of the logs for clarity
            println!("\n{}\n", "-=".repeat(40).black().bold());
            CliOutput::wrench(&format!("Listening on: {}", addr.to_string()), None);
            CliOutput::info(&format!("Leader ID: {}", leader_id), None);
            println!("\n{}\n", "-=".repeat(40).black().bold());

            // ===== FILE WATCHER TASK (SERVER) =====
            // Spawn a background task to watch the workspace directory for changes
            // This runs independently of the connection handling
            {
                let sync_config = Arc::clone(&sync_config);
                let connection_manager = Arc::clone(&connection_manager);
                let workspace_path = workspace_path.clone();
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
                        .watch(&workspace_path, RecursiveMode::Recursive)
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

                        // Now handle each file **once** based on event history and current state
                        for (path, event_kinds) in grouped {
                            let path_buf = PathBuf::from(&path);

                            // Skip .synclite directory
                            let is_synclite_dir = path.contains(".synclite");
                            if is_synclite_dir {
                                continue;
                            }

                            // Calculate relative path - skip if path is not within workspace
                            let relative_path = match path_buf.strip_prefix(&workspace_path) {
                                Ok(rel) => rel.to_str().unwrap().to_string(),
                                Err(_) => continue, // Path not within workspace
                            };

                            // Check actual file system state
                            let file_exists = path_buf.exists();

                            // Analyze event history
                            let has_create = event_kinds
                                .iter()
                                .any(|k| matches!(k, EventKind::Create(_)));
                            let has_remove = event_kinds
                                .iter()
                                .any(|k| matches!(k, EventKind::Remove(_)));
                            let has_modify = event_kinds
                                .iter()
                                .any(|k| matches!(k, EventKind::Modify(_)));

                            // Determine action based on event history and current state
                            match (file_exists, has_create, has_remove, has_modify) {
                                // File exists, saw both Create and Remove -> atomic write, treat as modify
                                (true, true, true, _) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) =
                                            sync_config.update_file(&relative_path, hash).await
                                        {
                                            CliOutput::log(
                                                &format!(
                                                    "Failed to update file {}: {}",
                                                    relative_path, e
                                                )
                                                .red(),
                                                None,
                                            );
                                        }
                                    }
                                }
                                // File exists, saw Create but no Remove -> new file
                                (true, true, false, _) => {
                                    if let Err(e) = sync_config
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
                                        CliOutput::log(
                                            &format!("Failed to add file {}: {}", relative_path, e)
                                                .red(),
                                            None,
                                        );
                                    }
                                }
                                // File exists, no Create event -> modification
                                (true, false, _, true) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) =
                                            sync_config.update_file(&relative_path, hash).await
                                        {
                                            CliOutput::log(
                                                &format!(
                                                    "Failed to update file {}: {}",
                                                    relative_path, e
                                                )
                                                .red(),
                                                None,
                                            );
                                        }
                                    }
                                }
                                // File doesn't exist, saw Remove -> delete (includes temp files)
                                (false, _, true, _) => {
                                    if let Err(e) = sync_config.delete_file(&relative_path).await {
                                        CliOutput::log(
                                            &format!(
                                                "Failed to delete file {}: {}",
                                                relative_path, e
                                            )
                                            .red(),
                                            None,
                                        );
                                    }
                                }
                                // Any other case -> no action needed
                                _ => {}
                            }
                        }
                    }
                });
            }

            // ===== CONNECTION HANDLER (SERVER) =====
            while let Ok((stream, peer_addr)) = listener.accept().await {
                let peers_config = Arc::clone(&peers_config);
                let sync_config = Arc::clone(&sync_config);
                let connection_manager = Arc::clone(&connection_manager);
                let leader_id = leader_id.clone();
                let workspace_path = workspace_path.clone();
                let ignore_file_events = Arc::clone(&ignore_file_events);
                let workspace_file_system = Arc::clone(&workspace_file_system);

                tokio::spawn(async move {
                    let (mut reader, writer) = stream.into_split();

                    // Generate peer ID
                    let peer_id = generate_peer_id();
                    CliOutput::log(
                        &format!("New peer connecting: {} ({})", peer_id, peer_addr).bright_cyan(),
                        None,
                    );

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
                        CliOutput::log(&format!("Failed to acknowledge peer: {}", e).red(), None);
                    }

                    // Add peer to peers config
                    if let Err(e) = peers_config.add_peer(peer_id.clone()).await {
                        CliOutput::log(&format!("Failed to add peer to config: {}", e).red(), None);
                    }

                    // Broadcast peer config to all other peers
                    if let Err(failed_peers) = broadcast_peer_list(
                        Arc::clone(&connection_manager),
                        peers_config.state().await.peers.clone(),
                    )
                    .await
                    {
                        CliOutput::log(
                            &format!("Failed to broadcast peer list to peers: {:?}", failed_peers)
                                .red(),
                            None,
                        );
                    }

                    // ===== MESSAGE HANDLER (SERVER) =====
                    // Handle incoming messages from this peer
                    loop {
                        match receive_message_from_peer::<PeerMessage>(&mut reader).await {
                            Ok(message) => {
                                match message {
                                    // STEP 1: Peer sends their version
                                    PeerMessage::VersionPush {
                                        sync_state: peer_sync_state,
                                    } => {
                                        CliOutput::log(
                                            &format!("Received version from {}", peer_id),
                                            None,
                                        );

                                        // STEP 2: Check winning files for both sides
                                        let server_sync_state = sync_config.state().await;
                                        let (
                                            our_winning_files,
                                            their_winning_files,
                                            files_to_delete_from_server,
                                            files_to_delete_from_peer,
                                        ) = determine_winning_files(
                                            &server_sync_state,
                                            &peer_sync_state,
                                        );

                                        // Handle file deletions first
                                        if !files_to_delete_from_server.is_empty() {
                                            ignore_file_events.store(true, Ordering::Relaxed);

                                            for path in &files_to_delete_from_server {
                                                CliOutput::log(
                                                    &format!(
                                                        "Deleting workspace file: {} ({})",
                                                        path, peer_id
                                                    )
                                                    .bright_red(),
                                                    None,
                                                );
                                            }

                                            // Unified filesystem + state delete operation
                                            if let Err(e) = sync_config
                                                .sync_batch_delete_files(
                                                    &workspace_file_system,
                                                    &workspace_path,
                                                    &files_to_delete_from_server,
                                                    Some(&peer_sync_state),
                                                )
                                                .await
                                            {
                                                CliOutput::log(
                                                    &format!("Failed to delete files: {}", e).red(),
                                                    None,
                                                );
                                            }

                                            // Small delay to ensure file watcher events are processed
                                            tokio::time::sleep(Duration::from_millis(100)).await;
                                            ignore_file_events.store(false, Ordering::Relaxed);
                                        }

                                        // STEP 3: Send our winning files with content and request their winning files
                                        if !our_winning_files.is_empty()
                                            || !their_winning_files.is_empty()
                                        {
                                            // Read content of our winning files (paths are relative to workspace)
                                            let mut my_winning_files_with_content = HashMap::new();
                                            for file_path in &our_winning_files {
                                                // Convert relative path to absolute path for file operations
                                                let full_path = workspace_path.join(file_path);
                                                match workspace_file_system
                                                    .read_file(&full_path)
                                                    .await
                                                {
                                                    Ok(content) => {
                                                        // Store with relative path as key
                                                        my_winning_files_with_content
                                                            .insert(file_path.clone(), content);
                                                    }
                                                    Err(e) => {
                                                        CliOutput::log(
                                                            &format!("Failed to read workspace file {}: {}", file_path, e).red(),
                                                            None,
                                                        );
                                                    }
                                                }
                                            }

                                            if !my_winning_files_with_content.is_empty() {
                                                CliOutput::log(
                                                    &format!(
                                                        "Sending {} winning files to peer: {}",
                                                        my_winning_files_with_content.len(),
                                                        peer_id
                                                    )
                                                    .blue(),
                                                    None,
                                                );
                                            }

                                            if !their_winning_files.is_empty() {
                                                CliOutput::log(
                                                    &format!(
                                                        "Requesting {} winning files from peer: {}",
                                                        their_winning_files.len(),
                                                        peer_id
                                                    )
                                                    .blue(),
                                                    None,
                                                );
                                            }

                                            if let Err(e) = connection_manager
                                                .send_to_peer(
                                                    &peer_id,
                                                    &ServerMessage::FileContentRequestWithVersion {
                                                        my_winning_files:
                                                            my_winning_files_with_content,
                                                        files_to_delete: files_to_delete_from_peer,
                                                        request_files: their_winning_files,
                                                    },
                                                )
                                                .await
                                            {
                                                CliOutput::log(
                                                    &format!("Failed to send file content request to peer {}: {}", peer_id, e).red(),
                                                    None,
                                                );
                                            }
                                        }
                                    }

                                    // STEP 4: Peer sends their winning files
                                    PeerMessage::FileContentResponse { files } => {
                                        CliOutput::log(
                                            &format!(
                                                "Received {} winning files from peer: {}",
                                                files.len(),
                                                peer_id
                                            )
                                            .bright_cyan(),
                                            None,
                                        );

                                        // STEP 5: Apply their winning files and broadcast to everyone except this peer
                                        if !files.is_empty() {
                                            ignore_file_events.store(true, Ordering::Relaxed);

                                            for (path, _) in &files {
                                                CliOutput::log(
                                                    &format!(
                                                        "Creating workspace file: {} ({})",
                                                        path, peer_id
                                                    )
                                                    .green()
                                                    .bold(),
                                                    None,
                                                );
                                            }

                                            // Unified filesystem + state write operation
                                            if let Err(e) = sync_config
                                                .sync_batch_write_files(
                                                    &workspace_file_system,
                                                    &workspace_path,
                                                    &files,
                                                )
                                                .await
                                            {
                                                CliOutput::log(
                                                    &format!("Failed to write files: {}", e).red(),
                                                    None,
                                                );
                                            }

                                            // Small delay to ensure file watcher events are processed
                                            tokio::time::sleep(Duration::from_millis(100)).await;
                                            ignore_file_events.store(false, Ordering::Relaxed);

                                            // Broadcast winning files to all OTHER peers (excluding the one that sent them)
                                            let failed_peers = connection_manager
                                                .broadcast_except(
                                                    &ServerMessage::FileUpdatePush {
                                                        files_to_write: files.clone(),
                                                        files_to_delete: Vec::new(),
                                                    },
                                                    vec![peer_id.clone()],
                                                )
                                                .await;

                                            if !failed_peers.is_empty() {
                                                CliOutput::log(
                                                    &format!("Failed to broadcast file updates to {} peers", failed_peers.len()).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        CliOutput::log(
                                            &format!(
                                                "Received and processed {} winning files from {}",
                                                files.len(),
                                                peer_id
                                            ),
                                            None,
                                        );
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    // Remove peer from connection manager and peers config when connection is lost
                    CliOutput::log(&format!("Peer {} disconnected", peer_id).red(), None);

                    // Remove peer from connection manager
                    connection_manager.remove_connection(&peer_id).await;

                    // Remove peer from peers config
                    if let Err(e) = peers_config.remove_peer(&peer_id).await {
                        CliOutput::log(
                            &format!("Failed to remove peer {} from config: {}", peer_id, e)
                                .bright_red()
                                .bold(),
                            None,
                        );
                    }

                    // Broadcast peer config to all other peers
                    if let Err(failed_peers) = broadcast_peer_list(
                        connection_manager,
                        peers_config.state().await.peers.clone(),
                    )
                    .await
                    {
                        CliOutput::log(
                            &format!("Failed to broadcast peer list to peers: {:?}", failed_peers)
                                .red(),
                            None,
                        );
                    }
                });
            }
        }
        Command::Connect => {
            let addr: SocketAddr = format!("127.0.0.1:{}", args.port).parse().unwrap();
            let Ok(stream) = TcpStream::connect(addr).await else {
                CliOutput::error(&format!("Failed to connect to: {}", addr.to_string()), None);
                std::process::exit(1);
            };

            let (mut reader, mut writer) = stream.into_split();

            // Wait for connection acknowledgment from leader
            match receive_message_from_peer::<ServerMessage>(&mut reader).await {
                Ok(ServerMessage::ConnectionAck { peer_id, leader_id }) => {
                    // Separate connection logs from the rest of the logs for clarity
                    println!("{}\n", "-=".repeat(40).black().bold());
                    CliOutput::wrench(&format!("Connected to: {}", addr.to_string()), None);
                    CliOutput::info(&format!("Peer ID: {}", peer_id), None);
                    println!("\n{}\n", "-=".repeat(40).black().bold());

                    // Update peers config to set the leader
                    if let Err(e) = peers_config.set_leader(leader_id).await {
                        CliOutput::error(&format!("Failed to set leader: {}", e), None);
                    }

                    // ===== INITIAL SYNC (PEER) =====
                    // Send our current sync state to the server immediately after connecting
                    {
                        CliOutput::log(&format!("Sending version to server").bright_cyan(), None);
                        let sync_state = sync_config.state().await;
                        let message = PeerMessage::VersionPush { sync_state };

                        // Use the helper function to send the message
                        if let Err(e) = send_message_to_peer(&mut writer, &message).await {
                            CliOutput::log(
                                &format!("Failed to send version to server: {}", e)
                                    .red()
                                    .bold(),
                                None,
                            );

                            std::process::exit(1);
                        }
                    }

                    // ===== FILE WATCHER TASK (PEER) =====
                    // Spawn a background task to watch the workspace directory for changes
                    {
                        let _sync_config = Arc::clone(&sync_config);
                        let _workspace_path = workspace_path.to_path_buf();
                        // Clone writer for the file watcher task
                        // let writer = Arc::new(Mutex::new(writer));
                        // let writer_clone = Arc::clone(&writer);

                        tokio::spawn(async move {
                            // TODO: Use `notify` crate to watch workspace_path
                            // let (tx, rx) = channel();
                            // let mut watcher = notify::recommended_watcher(tx).unwrap();
                            // watcher.watch(&workspace_path, RecursiveMode::Recursive).unwrap();

                            // Loop to handle file system events
                            // loop {
                            //     match rx.recv() {
                            //         Ok(Ok(event)) => {
                            //             // Filter out .synclite directory events
                            //             // if event.paths contain ".synclite" -> skip
                            //
                            //             match event.kind {
                            //                 EventKind::Create(_) | EventKind::Modify(_) => {
                            //                     // File was created or modified locally
                            //                     for path in event.paths {
                            //                         // 1. Read file contents
                            //                         // let content = tokio::fs::read_to_string(&path).await.ok()?;
                            //                         //
                            //                         // 2. Calculate hash
                            //                         // let hash = calculate_hash(&content);
                            //                         //
                            //                         // 3. Update local sync_config
                            //                         // let relative_path = path.strip_prefix(&workspace_path)?;
                            //                         // sync_config.update_file(relative_path, hash).await.ok()?;
                            //                         //
                            //                         // 4. Send updated sync state to server
                            //                         // let sync_state = sync_config.state().await;
                            //                         // let message = PeerMessage::SyncStatePush { sync_state };
                            //                         //
                            //                         // let mut writer = writer_clone.lock().await;
                            //                         // send_message_to_peer(&mut writer, &message).await.ok()?;
                            //                     }
                            //                 }
                            //                 EventKind::Remove(_) => {
                            //                     // File was deleted locally
                            //                     for path in event.paths {
                            //                         // 1. Mark as deleted in sync_config
                            //                         // let relative_path = path.strip_prefix(&workspace_path)?;
                            //                         // sync_config.delete_file(relative_path).await.ok()?;
                            //                         //
                            //                         // 2. Send updated sync state to server
                            //                         // let sync_state = sync_config.state().await;
                            //                         // let message = PeerMessage::SyncStatePush { sync_state };
                            //                         //
                            //                         // let mut writer = writer_clone.lock().await;
                            //                         // send_message_to_peer(&mut writer, &message).await.ok()?;
                            //                     }
                            //                 }
                            //                 _ => {}
                            //             }
                            //         }
                            //         Err(e) => {
                            //             eprintln!("File watcher error: {:?}", e);
                            //         }
                            //     }
                            // }
                        });
                    }

                    // ===== MESSAGE HANDLER (PEER) =====
                    // Listen for messages from the server
                    loop {
                        match receive_message_from_peer::<ServerMessage>(&mut reader).await {
                            Ok(message) => {
                                match message {
                                    ServerMessage::PeerListUpdate { peers } => {
                                        CliOutput::log(
                                            "Updated connected peers list".yellow(),
                                            None,
                                        );

                                        // Update local peers config with full peer list
                                        if let Err(e) = peers_config.set_peers(peers).await {
                                            CliOutput::log(
                                                &format!("Failed to update peers config: {}", e)
                                                    .bright_red()
                                                    .bold(),
                                                None,
                                            );
                                        }
                                    }

                                    ServerMessage::FileContentRequestWithVersion {
                                        my_winning_files: server_winning_files,
                                        request_files,
                                        files_to_delete,
                                    } => {
                                        CliOutput::log(
                                            &format!(
                                                "Received {} winning files from server and request for {} files",
                                                server_winning_files.len(),
                                                request_files.len()
                                            ).bright_cyan(),
                                            None,
                                        );

                                        // Handle file deletions first
                                        if !files_to_delete.is_empty() {
                                            for path in &files_to_delete {
                                                CliOutput::log(
                                                    &format!("Deleting workspace file: {}", path)
                                                        .bright_red(),
                                                    None,
                                                );
                                            }

                                            // Unified filesystem + state delete operation
                                            if let Err(e) = sync_config
                                                .sync_batch_delete_files(
                                                    &workspace_file_system,
                                                    &workspace_path,
                                                    &files_to_delete,
                                                    None,
                                                )
                                                .await
                                            {
                                                CliOutput::log(
                                                    &format!("Failed to delete files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        // Apply server's winning files to our file system
                                        if !server_winning_files.is_empty() {
                                            for (path, _) in &server_winning_files {
                                                CliOutput::log(
                                                    &format!("Creating workspace file: {}", path)
                                                        .green(),
                                                    None,
                                                );
                                            }

                                            // Unified filesystem + state write operation
                                            if let Err(e) = sync_config
                                                .sync_batch_write_files(
                                                    &workspace_file_system,
                                                    &workspace_path,
                                                    &server_winning_files,
                                                )
                                                .await
                                            {
                                                CliOutput::log(
                                                    &format!("Failed to write files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        // Read our winning files and send them back
                                        let mut our_winning_files = HashMap::new();
                                        for path in request_files {
                                            // path is relative to workspace_path
                                            // Convert to absolute path for file operations
                                            let full_path = workspace_path.join(&path);
                                            match workspace_file_system.read_file(&full_path).await
                                            {
                                                Ok(content) => {
                                                    // Store with relative path as key
                                                    our_winning_files.insert(path, content);
                                                }
                                                Err(e) => {
                                                    CliOutput::log(
                                                        &format!(
                                                            "Failed to read requested file {}: {}",
                                                            path, e
                                                        )
                                                        .red(),
                                                        None,
                                                    );
                                                }
                                            }
                                        }

                                        // Send our winning files back to server
                                        if !our_winning_files.is_empty() {
                                            let message = PeerMessage::FileContentResponse {
                                                files: our_winning_files,
                                            };
                                            if let Err(e) =
                                                send_message_to_peer(&mut writer, &message).await
                                            {
                                                CliOutput::log(
                                                    &format!(
                                                        "Failed to send file content response: {}",
                                                        e
                                                    )
                                                    .red(),
                                                    None,
                                                );
                                            }
                                        }
                                    }

                                    ServerMessage::FileUpdatePush {
                                        files_to_write,
                                        files_to_delete,
                                    } => {
                                        // Server is pushing updated files to us
                                        if !files_to_write.is_empty() {
                                            for (path, _) in &files_to_write {
                                                CliOutput::log(
                                                    &format!("Writing workspace file: {}", path)
                                                        .green(),
                                                    None,
                                                );
                                            }

                                            if let Err(e) = sync_config
                                                .sync_batch_write_files(
                                                    &workspace_file_system,
                                                    &workspace_path,
                                                    &files_to_write,
                                                )
                                                .await
                                            {
                                                CliOutput::log(
                                                    &format!("Failed to write files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        if !files_to_delete.is_empty() {
                                            for path in &files_to_delete {
                                                CliOutput::log(
                                                    &format!("Deleting workspace file: {}", path)
                                                        .bright_red(),
                                                    None,
                                                );
                                            }
                                            if let Err(e) = sync_config
                                                .sync_batch_delete_files(
                                                    &workspace_file_system,
                                                    &workspace_path,
                                                    &files_to_delete,
                                                    None,
                                                )
                                                .await
                                            {
                                                CliOutput::log(
                                                    &format!("Failed to delete files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        CliOutput::log("Received file updates from server", None);
                                    }

                                    _ => {
                                        CliOutput::log(
                                            &format!("Received message: {:?}", message),
                                            None,
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                CliOutput::log(
                                    &format!("Connection to leader lost: {}", e)
                                        .bright_red()
                                        .bold(),
                                    None,
                                );
                                break;
                            }
                        }
                    }
                }
                Ok(other_message) => {
                    CliOutput::log(
                        &format!("Unexpected message from leader: {:?}", other_message)
                            .bright_red()
                            .bold(),
                        None,
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    CliOutput::error(
                        &format!("Failed to receive acknowledgment from leader: {}", e)
                            .bright_red()
                            .bold(),
                        None,
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
