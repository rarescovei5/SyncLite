use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use synclite::{
    cli::{self, types::Command},
    models::{PeersConfig, SyncConfig},
    network::{
        PeerConnectionManager, PeerMessage, acknowledge_peer, broadcast_peer_list,
        generate_peer_id, receive_message_from_peer,
    },
    storage::{initialise_state, initialise_storage},
    utils::{output::CliOutput, read_peers_state, read_sync_state},
};
use tokio::net::{TcpListener, TcpStream};

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

    let workspace_path = Path::new(&args.path);
    let storage_path = workspace_path.join(".synclite");

    // Initialise storage directory with peers and sync state files
    if let Err(e) = initialise_storage(workspace_path).await {
        CliOutput::error(&format!("Failed to initialise storage: {}", e), None);
        std::process::exit(1);
    }
    print!("\n");

    // Initialise sync state by merging saved and computed states
    if let Err(e) = initialise_state(workspace_path) {
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

    match args.command {
        Command::Serve => {
            let leader_id = generate_peer_id();
            let addr: SocketAddr = format!("127.0.0.1:{}", args.port).parse().unwrap();
            let listener = TcpListener::bind(addr).await?;

            // Initialize connection manager
            let connection_manager = Arc::new(PeerConnectionManager::new());

            // Set this peer as the leader
            if let Err(e) = peers_config.set_leader(leader_id.clone()).await {
                CliOutput::error(&format!("Failed to set leader: {}", e), None);
            }

            // Separate connection logs from the rest of the logs for clarity
            println!("\n{}\n", "-=".repeat(40).black().bold());
            CliOutput::wrench(&format!("Listening on: {}", addr.to_string()), None);
            CliOutput::info(&format!("Leader ID: {}", leader_id), None);
            println!("\n{}\n", "-=".repeat(40).black().bold());

            while let Ok((stream, peer_addr)) = listener.accept().await {
                let peers_config = Arc::clone(&peers_config);
                let connection_manager = Arc::clone(&connection_manager);
                let leader_id = leader_id.clone();

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

                    // Handle incoming messages from this peer
                    loop {
                        match receive_message_from_peer(&mut reader).await {
                            Ok(message) => {
                                CliOutput::info(
                                    &format!("Received message from {}: {:?}", peer_id, message),
                                    None,
                                );
                                // Handle different message types here
                            }
                            Err(e) => {
                                CliOutput::log(
                                    &format!("Error receiving message from {}: {}", peer_id, e)
                                        .red(),
                                    None,
                                );
                                break;
                            }
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

            let (mut reader, _writer) = stream.into_split();

            // Wait for connection acknowledgment from leader
            match receive_message_from_peer(&mut reader).await {
                Ok(PeerMessage::ConnectionAck { peer_id, leader_id }) => {
                    // Separate connection logs from the rest of the logs for clarity
                    println!("{}\n", "-=".repeat(40).black().bold());
                    CliOutput::wrench(&format!("Connected to: {}", addr.to_string()), None);
                    CliOutput::info(&format!("Peer ID: {}", peer_id), None);
                    println!("\n{}\n", "-=".repeat(40).black().bold());

                    // Update peers config to set the leader
                    if let Err(e) = peers_config.set_leader(leader_id).await {
                        CliOutput::error(&format!("Failed to set leader: {}", e), None);
                    }

                    // Listen for messages from the leader
                    loop {
                        match receive_message_from_peer(&mut reader).await {
                            Ok(message) => {
                                match message {
                                    PeerMessage::PeerListUpdate { peers } => {
                                        CliOutput::log("Updated connected peers list", None);

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
