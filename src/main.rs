mod file_watcher;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use p2p::{ConnectionManager, receive_message_from_peer, send_message_to_peer};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use colored::Colorize;
use log::{elog, log};

use synclite::network::{
    generate_peer_id, mdns,
    messages::{
        PeerMessage, ServerMessage,
        handlers::{peer, server},
    },
    peer_registry::{acknowledge_peer, broadcast_peer_list},
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
            let _server_watcher = file_watcher::spawn_file_watcher(
                file_watcher::WatcherMode::Server {
                    connection_manager: Arc::clone(&connection_manager),
                },
                Arc::clone(&sync_manager),
                Arc::clone(&ignore_file_events),
            );

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

            let _peer_watcher = file_watcher::spawn_file_watcher(
                file_watcher::WatcherMode::Peer { file_change_tx },
                Arc::clone(&sync_manager),
                Arc::clone(&ignore_file_events),
            );

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
