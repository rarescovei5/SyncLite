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

use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use colored::Colorize;

use synclite::{
    cli::{Args, Command},
    handlers::{handle_initialise_storage, handle_parse_args},
    models::{FileEntry, PeersConfig, SyncConfig},
    network::{
        PeerConnectionManager, PeerMessage, ServerMessage, acknowledge_peer, broadcast_peer_list,
        generate_peer_id, receive_message_from_peer, send_message_to_peer,
    },
    sync::{calculate_file_hash, determine_winning_files},
    utils::{Log, read_json},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    Log::banner();

    let Args {
        command,
        abs_workspace_path,
        port,
    } = handle_parse_args();

    let abs_storage_path = abs_workspace_path.join(".synclite");

    // Initialise storage directory with peers and sync state files
    handle_initialise_storage(&abs_workspace_path);

    // Initialize sync_config, load the state from the storage directory
    // and patch with the the state of the physical filesystem
    let sync_config = Arc::new(SyncConfig::new(&abs_storage_path));
    sync_config.load().await;
    sync_config.patch().await;
    sync_config.save().await;

    // Initialize peers_config, load the state from the storage directory
    let peers_config = Arc::new(PeersConfig::new(
        &abs_storage_path,
        read_json(&abs_storage_path.join("peers.json")).unwrap(),
    ));

    match command {
        Command::Serve => {
            let leader_id = generate_peer_id();
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
            let listener = TcpListener::bind(addr).await?;

            // Initialize connection manager
            let connection_manager = Arc::new(PeerConnectionManager::new());

            // Flag to ignore file watcher events when program is making changes
            let ignore_file_events = Arc::new(AtomicBool::new(false));

            // Set this peer as the leader
            if let Err(e) = peers_config.set_leader(leader_id.clone()).await {
                Log::error(&format!("Failed to set leader: {}", e), None);
            }

            // Separate connection logs from the rest of the logs for clarity
            println!("\n{}\n", "-=".repeat(40).black().bold());
            Log::wrench(&format!("Listening on: {}", addr.to_string()), None);
            Log::info(&format!("Leader ID: {}", leader_id), None);
            println!("\n{}\n", "-=".repeat(40).black().bold());

            // ===== FILE WATCHER TASK (SERVER) =====
            // Spawn a background task to watch the workspace directory for changes
            // This runs independently of the connection handling
            {
                let sync_config = Arc::clone(&sync_config);
                let connection_manager = Arc::clone(&connection_manager);
                let abs_workspace_path = abs_workspace_path.clone();
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
                        .watch(&abs_workspace_path, RecursiveMode::Recursive)
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

                        let mut files_to_update: HashMap<String, String> = HashMap::new();
                        let mut files_to_delete: Vec<String> = Vec::new();

                        // Now handle each file **once** based on event history and current state
                        for (path, event_kinds) in grouped {
                            let path_buf = PathBuf::from(&path);

                            // Skip .synclite directory
                            let is_synclite_dir = path.contains(".synclite");
                            if is_synclite_dir {
                                continue;
                            }

                            // Calculate relative path - skip if path is not within workspace
                            let relative_path = match path_buf.strip_prefix(&abs_workspace_path) {
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
                                            Log::log(
                                                &format!(
                                                    "Failed to update file {}: {}",
                                                    relative_path, e
                                                )
                                                .red(),
                                                None,
                                            );
                                        }
                                    }
                                    files_to_update.insert(
                                        relative_path.clone(),
                                        fs::read_to_string(&path_buf).unwrap(),
                                    );
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
                                        Log::log(
                                            &format!("Failed to add file {}: {}", relative_path, e)
                                                .red(),
                                            None,
                                        );
                                    }
                                    files_to_update.insert(
                                        relative_path.clone(),
                                        fs::read_to_string(&path_buf).unwrap(),
                                    );
                                }
                                // File exists, no Create event -> modification
                                (true, false, _, true) => {
                                    if let Ok(hash) = calculate_file_hash(&path_buf) {
                                        if let Err(e) =
                                            sync_config.update_file(&relative_path, hash).await
                                        {
                                            Log::log(
                                                &format!(
                                                    "Failed to update file {}: {}",
                                                    relative_path, e
                                                )
                                                .red(),
                                                None,
                                            );
                                        }
                                    }
                                    files_to_update.insert(
                                        relative_path.clone(),
                                        fs::read_to_string(&path_buf).unwrap(),
                                    );
                                }
                                // File doesn't exist, saw Remove -> delete (includes temp files)
                                (false, _, true, _) => {
                                    if let Err(e) = sync_config.delete_file(&relative_path).await {
                                        Log::log(
                                            &format!(
                                                "Failed to delete file {}: {}",
                                                relative_path, e
                                            )
                                            .red(),
                                            None,
                                        );
                                    }
                                    files_to_delete.push(relative_path.clone());
                                }
                                // Any other case -> no action needed
                                _ => {}
                            }
                        }
                        // Broadcast the file updates to all peers
                        connection_manager
                            .broadcast_message(&ServerMessage::FileUpdatePush {
                                files_to_write: files_to_update,
                                files_to_delete: files_to_delete,
                            })
                            .await;
                    }
                });
            }

            // ===== CONNECTION HANDLER (SERVER) =====
            while let Ok((stream, peer_addr)) = listener.accept().await {
                let peers_config = Arc::clone(&peers_config);
                let sync_config = Arc::clone(&sync_config);
                let connection_manager = Arc::clone(&connection_manager);
                let leader_id = leader_id.clone();
                let abs_workspace_path = abs_workspace_path.clone();
                let ignore_file_events = Arc::clone(&ignore_file_events);

                tokio::spawn(async move {
                    let (mut reader, writer) = stream.into_split();

                    // Generate peer ID
                    let peer_id = generate_peer_id();
                    Log::log(
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
                        Log::log(&format!("Failed to acknowledge peer: {}", e).red(), None);
                    }

                    // Add peer to peers config
                    if let Err(e) = peers_config.add_peer(peer_id.clone()).await {
                        Log::log(&format!("Failed to add peer to config: {}", e).red(), None);
                    }

                    // Broadcast peer config to all other peers
                    if let Err(failed_peers) = broadcast_peer_list(
                        Arc::clone(&connection_manager),
                        peers_config.state().await.peers.clone(),
                    )
                    .await
                    {
                        Log::log(
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
                                    PeerMessage::InitialSyncPush {
                                        sync_state: peer_sync_state,
                                    } => {
                                        Log::log(
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
                                                Log::log(
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
                                                    &abs_workspace_path,
                                                    &files_to_delete_from_server,
                                                    Some(&peer_sync_state),
                                                )
                                                .await
                                            {
                                                Log::log(
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
                                                let full_path = abs_workspace_path.join(file_path);
                                                match fs::read_to_string(&full_path) {
                                                    Ok(content) => {
                                                        // Store with relative path as key
                                                        my_winning_files_with_content
                                                            .insert(file_path.clone(), content);
                                                    }
                                                    Err(e) => {
                                                        Log::log(
                                                            &format!("Failed to read workspace file {}: {}", file_path, e).red(),
                                                            None,
                                                        );
                                                    }
                                                }
                                            }

                                            if !my_winning_files_with_content.is_empty() {
                                                Log::log(
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
                                                Log::log(
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
                                                    &ServerMessage::InitialSyncPushResponse {
                                                        files_to_update:
                                                            my_winning_files_with_content,
                                                        files_to_delete: files_to_delete_from_peer,
                                                        files_to_send_back: their_winning_files,
                                                    },
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!("Failed to send initial sync push response to peer {}: {}", peer_id, e).red(),
                                                    None,
                                                );
                                            }
                                        }
                                    }
                                    PeerMessage::FileUpdatePush {
                                        files_to_write,
                                        files_to_delete,
                                    } => {
                                        Log::log(
                                            &format!(
                                                "Received file updates from peer: {}",
                                                peer_id
                                            )
                                            .blue(),
                                            None,
                                        );

                                        if !files_to_write.is_empty() {
                                            ignore_file_events.store(true, Ordering::Relaxed);
                                            for (path, _) in &files_to_write {
                                                Log::log(
                                                    &format!("Writing workspace file: {}", path)
                                                        .green(),
                                                    None,
                                                );
                                            }

                                            if let Err(e) = sync_config
                                                .sync_batch_write_files(
                                                    &abs_workspace_path,
                                                    &files_to_write,
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!("Failed to write files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }
                                        if !files_to_delete.is_empty() {
                                            ignore_file_events.store(true, Ordering::Relaxed);
                                            for path in &files_to_delete {
                                                Log::log(
                                                    &format!("Deleting workspace file: {}", path)
                                                        .bright_red(),
                                                    None,
                                                );
                                            }

                                            if let Err(e) = sync_config
                                                .sync_batch_delete_files(
                                                    &abs_workspace_path,
                                                    &files_to_delete,
                                                    None,
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!("Failed to delete files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        // Small delay to ensure file watcher events are processed
                                        tokio::time::sleep(Duration::from_millis(100)).await;
                                        ignore_file_events.store(false, Ordering::Relaxed);

                                        let failed_peers = connection_manager
                                            .broadcast_except(
                                                &ServerMessage::FileUpdatePush {
                                                    files_to_write: files_to_write.clone(),
                                                    files_to_delete: files_to_delete.clone(),
                                                },
                                                vec![peer_id.clone()],
                                            )
                                            .await;

                                        if !failed_peers.is_empty() {
                                            Log::log(
                                                &format!(
                                                    "Failed to broadcast file updates to {} peers",
                                                    failed_peers.len()
                                                )
                                                .red(),
                                                None,
                                            );
                                        }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    // Remove peer from connection manager and peers config when connection is lost
                    Log::log(&format!("Peer {} disconnected", peer_id).red(), None);

                    // Remove peer from connection manager
                    connection_manager.remove_connection(&peer_id).await;

                    // Remove peer from peers config
                    if let Err(e) = peers_config.remove_peer(&peer_id).await {
                        Log::log(
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
                        Log::log(
                            &format!("Failed to broadcast peer list to peers: {:?}", failed_peers)
                                .red(),
                            None,
                        );
                    }
                });
            }
        }
        Command::Connect => {
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
            let Ok(stream) = TcpStream::connect(addr).await else {
                Log::error(&format!("Failed to connect to: {}", addr.to_string()), None);
                std::process::exit(1);
            };

            let (mut reader, mut writer) = stream.into_split();

            // Wait for connection acknowledgment from leader
            match receive_message_from_peer::<ServerMessage>(&mut reader).await {
                Ok(ServerMessage::ConnectionAck { peer_id, leader_id }) => {
                    // Separate connection logs from the rest of the logs for clarity
                    println!("{}\n", "-=".repeat(40).black().bold());
                    Log::wrench(&format!("Connected to: {}", addr.to_string()), None);
                    Log::info(&format!("Peer ID: {}", peer_id), None);
                    println!("\n{}\n", "-=".repeat(40).black().bold());

                    // Update peers config to set the leader
                    if let Err(e) = peers_config.set_leader(leader_id).await {
                        Log::error(&format!("Failed to set leader: {}", e), None);
                    }

                    // ===== INITIAL SYNC (PEER) =====
                    // Send our current sync state to the server immediately after connecting
                    {
                        Log::log(&format!("Sending version to server").bright_cyan(), None);
                        let sync_state = sync_config.state().await;
                        let message = PeerMessage::InitialSyncPush { sync_state };

                        if let Err(e) = send_message_to_peer(&mut writer, &message).await {
                            Log::log(
                                &format!("Failed to send initial sync state to server: {}", e)
                                    .red()
                                    .bold(),
                                None,
                            );

                            std::process::exit(1);
                        }
                    }

                    // ===== FILE WATCHER TASK (PEER) =====
                    // Spawn a background task to watch the workspace directory for changes
                    let ignore_file_events = Arc::new(AtomicBool::new(false));

                    // Create a channel for the file watcher to send messages to the main connection handler
                    let (file_change_tx, mut file_change_rx) = mpsc::channel::<PeerMessage>(100);

                    {
                        let sync_config = Arc::clone(&sync_config);
                        let abs_workspace_path = abs_workspace_path.clone();
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
                                .watch(&abs_workspace_path, RecursiveMode::Recursive)
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

                                let mut files_to_update: HashMap<String, String> = HashMap::new();
                                let mut files_to_delete: Vec<String> = Vec::new();

                                // Now handle each file **once** based on event history and current state
                                for (path, event_kinds) in grouped {
                                    let path_buf = PathBuf::from(&path);

                                    // Skip .synclite directory
                                    let is_synclite_dir = path.contains(".synclite");
                                    if is_synclite_dir {
                                        continue;
                                    }

                                    // Calculate relative path - skip if path is not within workspace
                                    let relative_path =
                                        match path_buf.strip_prefix(&abs_workspace_path) {
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
                                                if let Err(e) = sync_config
                                                    .update_file(&relative_path, hash)
                                                    .await
                                                {
                                                    Log::log(
                                                        &format!(
                                                            "Failed to update file {}: {}",
                                                            relative_path, e
                                                        )
                                                        .red(),
                                                        None,
                                                    );
                                                }
                                            }
                                            files_to_update.insert(
                                                relative_path.clone(),
                                                fs::read_to_string(&path_buf).unwrap(),
                                            );
                                        }
                                        // File exists, saw Create but no Remove -> new file
                                        (true, true, false, _) => {
                                            if let Err(e) = sync_config
                                                .add_file(
                                                    relative_path.clone(),
                                                    FileEntry {
                                                        hash: Some(
                                                            calculate_file_hash(&path_buf).unwrap(),
                                                        ),
                                                        is_deleted: false,
                                                        last_modified: Utc::now(),
                                                    },
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!(
                                                        "Failed to add file {}: {}",
                                                        relative_path, e
                                                    )
                                                    .red(),
                                                    None,
                                                );
                                            }
                                            files_to_update.insert(
                                                relative_path.clone(),
                                                fs::read_to_string(&path_buf).unwrap(),
                                            );
                                        }
                                        // File exists, no Create event -> modification
                                        (true, false, _, true) => {
                                            if let Ok(hash) = calculate_file_hash(&path_buf) {
                                                if let Err(e) = sync_config
                                                    .update_file(&relative_path, hash)
                                                    .await
                                                {
                                                    Log::log(
                                                        &format!(
                                                            "Failed to update file {}: {}",
                                                            relative_path, e
                                                        )
                                                        .red(),
                                                        None,
                                                    );
                                                }
                                            }
                                            files_to_update.insert(
                                                relative_path.clone(),
                                                fs::read_to_string(&path_buf).unwrap(),
                                            );
                                        }
                                        // File doesn't exist, saw Remove -> delete (includes temp files)
                                        (false, _, true, _) => {
                                            if let Err(e) =
                                                sync_config.delete_file(&relative_path).await
                                            {
                                                Log::log(
                                                    &format!(
                                                        "Failed to delete file {}: {}",
                                                        relative_path, e
                                                    )
                                                    .red(),
                                                    None,
                                                );
                                            }
                                            files_to_delete.push(relative_path.clone());
                                        }
                                        // Any other case -> no action needed
                                        _ => {}
                                    }
                                }

                                // Send the file updates to the main connection handler via channel
                                let _ = file_change_tx
                                    .send(PeerMessage::FileUpdatePush {
                                        files_to_write: files_to_update,
                                        files_to_delete: files_to_delete,
                                    })
                                    .await;
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
                                        Log::log("Updated connected peers list".yellow(), None);

                                        // Update local peers config with full peer list
                                        if let Err(e) = peers_config.set_peers(peers).await {
                                            Log::log(
                                                &format!("Failed to update peers config: {}", e)
                                                    .bright_red()
                                                    .bold(),
                                                None,
                                            );
                                        }
                                    }

                                    ServerMessage::InitialSyncPushResponse {
                                        files_to_update,
                                        files_to_delete,
                                        files_to_send_back,
                                    } => {
                                        // Handle file deletions first
                                        if !files_to_delete.is_empty() {
                                            ignore_file_events.store(true, Ordering::Relaxed);
                                            for path in &files_to_delete {
                                                Log::log(
                                                    &format!("Deleting workspace file: {}", path)
                                                        .bright_red(),
                                                    None,
                                                );
                                            }

                                            // Unified filesystem + state delete operation
                                            if let Err(e) = sync_config
                                                .sync_batch_delete_files(
                                                    &abs_workspace_path,
                                                    &files_to_delete,
                                                    None,
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!("Failed to delete files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        // Apply server's winning files to our file system
                                        if !files_to_update.is_empty() {
                                            ignore_file_events.store(true, Ordering::Relaxed);
                                            for (path, _) in &files_to_update {
                                                Log::log(
                                                    &format!("Creating workspace file: {}", path)
                                                        .green(),
                                                    None,
                                                );
                                            }

                                            // Unified filesystem + state write operation
                                            if let Err(e) = sync_config
                                                .sync_batch_write_files(
                                                    &abs_workspace_path,
                                                    &files_to_update,
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!("Failed to write files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        // Wait for file watcher events to be processed
                                        tokio::time::sleep(Duration::from_millis(100)).await;
                                        ignore_file_events.store(false, Ordering::Relaxed);

                                        let mut our_winning_files = HashMap::new();
                                        for path in files_to_send_back {
                                            // path is relative to workspace_path
                                            // Convert to absolute path for file operations
                                            let full_path = abs_workspace_path.join(&path);
                                            match fs::read_to_string(&full_path) {
                                                Ok(content) => {
                                                    // Store with relative path as key
                                                    our_winning_files.insert(path, content);
                                                }
                                                Err(e) => {
                                                    Log::log(
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
                                            let message = PeerMessage::FileUpdatePush {
                                                files_to_write: our_winning_files,
                                                files_to_delete: Vec::new(),
                                            };
                                            if let Err(e) =
                                                send_message_to_peer(&mut writer, &message).await
                                            {
                                                Log::log(
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
                                            ignore_file_events.store(true, Ordering::Relaxed);
                                            for (path, _) in &files_to_write {
                                                Log::log(
                                                    &format!("Writing workspace file: {}", path)
                                                        .green(),
                                                    None,
                                                );
                                            }

                                            if let Err(e) = sync_config
                                                .sync_batch_write_files(
                                                    &abs_workspace_path,
                                                    &files_to_write,
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!("Failed to write files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        if !files_to_delete.is_empty() {
                                            ignore_file_events.store(true, Ordering::Relaxed);
                                            for path in &files_to_delete {
                                                Log::log(
                                                    &format!("Deleting workspace file: {}", path)
                                                        .bright_red(),
                                                    None,
                                                );
                                            }
                                            if let Err(e) = sync_config
                                                .sync_batch_delete_files(
                                                    &abs_workspace_path,
                                                    &files_to_delete,
                                                    None,
                                                )
                                                .await
                                            {
                                                Log::log(
                                                    &format!("Failed to delete files: {}", e).red(),
                                                    None,
                                                );
                                            }
                                        }

                                        // Wait for file watcher events to be processed
                                        tokio::time::sleep(Duration::from_millis(100)).await;
                                        ignore_file_events.store(false, Ordering::Relaxed);
                                    }

                                            _ => {
                                                Log::log(&format!("Received message: {:?}", message), None);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        Log::log(
                                            &format!("Connection to leader lost: {}", e)
                                                .bright_red()
                                                .bold(),
                                            None,
                                        );
                                        break;
                                    }
                                }
                            }
                            // Handle file changes from the file watcher
                            Some(file_change_msg) = file_change_rx.recv() => {
                                // Send file change to server
                                if let Err(e) = send_message_to_peer(&mut writer, &file_change_msg).await {
                                    Log::log(
                                        &format!("Failed to send file changes to server: {}", e).red(),
                                        None,
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(other_message) => {
                    Log::log(
                        &format!("Unexpected message from leader: {:?}", other_message)
                            .bright_red()
                            .bold(),
                        None,
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    Log::error(
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
