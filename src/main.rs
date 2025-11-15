use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use synclite::{
    cli::{
        self,
        types::{Command, ParseErrorCode},
    },
    models::PeersConfig,
    network::{PeerMessage, generate_peer_id, receive_message_from_peer, send_message_to_peer},
    storage::{initialise_storage, read_peers_config, write_peers_config},
    sync::initialise_state,
    utils::output::CliOutput,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

use colored::Colorize;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    CliOutput::banner();

    let args = match cli::parse_args() {
        Ok(args) => args,
        Err(e) => {
            CliOutput::error(&e.message, None);
            match e.code {
                // If user doesn't provide any arguments, he doesn't know the commands, so we show the usage
                ParseErrorCode::NoCommand | ParseErrorCode::NoPath => {
                    CliOutput::usage();
                }
                _ => (),
            }
            std::process::exit(e.code as i32);
        }
    };

    CliOutput::info(
        &format!("Starting {} mode for: {}\n", args.command, args.path),
        None,
    );

    initialise_storage(&args.path);
    initialise_state(&args.path);

    print!("\n");

    let storage_path = Path::new(&args.path).join(".synclite");

    // Shared state for peer connections
    let connected_peers: Arc<Mutex<HashMap<String, SocketAddr>>> =
        Arc::new(Mutex::new(HashMap::new()));

    match args.command {
        Command::Serve => {
            let addr: SocketAddr = format!("127.0.0.1:{}", args.port).parse().unwrap();
            let listener = TcpListener::bind(addr).await?;

            // Set this peer as the leader
            let mut peers_config = read_peers_config(&storage_path).unwrap();
            let leader_id = generate_peer_id();
            peers_config.set_leader(leader_id.clone());
            write_peers_config(&storage_path, &peers_config).unwrap();

            let peers_config: Arc<Mutex<PeersConfig>> = Arc::new(Mutex::new(peers_config));

            println!("\n{}\n", "-=".repeat(40).black().bold());
            CliOutput::wrench(&format!("Listening on: {}", addr.to_string()), None);
            CliOutput::info(&format!("Leader ID: {}", generate_peer_id()), None);
            println!("\n{}\n", "-=".repeat(40).black().bold());

            while let Ok((stream, peer_addr)) = listener.accept().await {
                let peers_clone = Arc::clone(&connected_peers);
                let storage_path_clone = storage_path.clone();
                let peers_config_clone = Arc::clone(&peers_config);

                tokio::spawn(async move {
                    let peer_id = generate_peer_id();
                    CliOutput::log(
                        &format!("New peer connecting: {} ({})", peer_id, peer_addr),
                        None,
                    );

                    let (mut reader, mut writer) = stream.into_split();

                    // Send connection acknowledgment with peer ID
                    if let Err(e) = send_message_to_peer(
                        &mut writer,
                        &PeerMessage::ConnectionAck {
                            peer_id: peer_id.clone(),
                        },
                    )
                    .await
                    {
                        CliOutput::error(
                            &format!("Failed to send ack to peer {}: {}", peer_id, e),
                            None,
                        );
                        return;
                    }

                    // Add peer to connected peers list
                    {
                        let mut peers = peers_clone.lock().await;
                        peers.insert(peer_id.clone(), peer_addr);
                    }

                    // Update peers config and save to disk
                    {
                        let mut config = peers_config_clone.lock().await;
                        config.add_peer(format!("{}:{}", peer_addr.ip(), peer_addr.port()));
                        if let Err(e) = write_peers_config(&storage_path_clone, &config) {
                            CliOutput::error(&format!("Failed to save peers config: {}", e), None);
                        }
                    }

                    // Notify all other peers about the new peer
                    let _new_peer_message = PeerMessage::NewPeerJoined {
                        peer_id: peer_id.clone(),
                        peer_addr: peer_addr.to_string(),
                    };

                    // Send notification to all existing peers
                    {
                        let peers = peers_clone.lock().await;
                        let peer_count = peers.len();
                        if peer_count > 1 {
                            CliOutput::log(
                                &format!(
                                    "Notifying {} existing peers about new peer {}",
                                    peer_count - 1,
                                    peer_id
                                ),
                                None,
                            );
                            // Note: In a full implementation, you would maintain active connections
                            // to all peers and send the _new_peer_message to each of them
                        }
                    }

                    CliOutput::log(
                        &format!("Peer {} successfully connected and registered", peer_id),
                        None,
                    );

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
                                CliOutput::error(
                                    &format!("Error receiving message from {}: {}", peer_id, e),
                                    None,
                                );
                                break;
                            }
                        }
                    }

                    // Remove peer when connection is lost
                    {
                        let mut peers = peers_clone.lock().await;
                        peers.remove(&peer_id);
                    }
                    CliOutput::log(&format!("Peer {} disconnected", peer_id), None);
                });
            }
        }
        Command::Connect => {
            let addr: SocketAddr = format!("127.0.0.1:{}", args.port).parse().unwrap();
            let stream = match TcpStream::connect(addr).await {
                Ok(stream) => stream,
                Err(_) => {
                    CliOutput::error(&format!("Failed to connect to: {}", addr.to_string()), None);
                    std::process::exit(1);
                }
            };

            println!("{}\n", "-=".repeat(40).black().bold());
            CliOutput::wrench(&format!("Connected to: {}", addr.to_string()), None);
            println!("\n{}\n", "-=".repeat(40).black().bold());

            let (mut reader, _writer) = stream.into_split();

            // Wait for connection acknowledgment from leader
            match receive_message_from_peer(&mut reader).await {
                Ok(PeerMessage::ConnectionAck { peer_id }) => {
                    CliOutput::success(
                        &format!("Connection acknowledged! Assigned peer ID: {}", peer_id),
                        None,
                    );

                    // Update peers config to set the leader
                    let mut config = read_peers_config(&storage_path).unwrap();
                    config.set_leader(addr.to_string());
                    if let Err(e) = write_peers_config(&storage_path, &config) {
                        CliOutput::error(&format!("Failed to save peers config: {}", e), None);
                    }

                    CliOutput::info(
                        &format!("Successfully joined network as peer: {}", peer_id),
                        None,
                    );

                    // Listen for messages from the leader
                    loop {
                        match receive_message_from_peer(&mut reader).await {
                            Ok(message) => {
                                match message {
                                    PeerMessage::NewPeerJoined {
                                        peer_id: new_peer_id,
                                        peer_addr: new_peer_addr,
                                    } => {
                                        CliOutput::info(
                                            &format!(
                                                "New peer joined network: {} ({})",
                                                new_peer_id, new_peer_addr
                                            ),
                                            None,
                                        );

                                        // Update local peers config
                                        let mut config = read_peers_config(&storage_path).unwrap();
                                        config.add_peer(new_peer_addr);
                                        if let Err(e) = write_peers_config(&storage_path, &config) {
                                            CliOutput::error(
                                                &format!("Failed to update peers config: {}", e),
                                                None,
                                            );
                                        }
                                    }
                                    PeerMessage::PeerListUpdate { peers } => {
                                        CliOutput::info(
                                            &format!("Received peer list update: {:?}", peers),
                                            None,
                                        );

                                        // Update local peers config with full peer list
                                        let mut config = read_peers_config(&storage_path).unwrap();
                                        config.peers = peers;
                                        if let Err(e) = write_peers_config(&storage_path, &config) {
                                            CliOutput::error(
                                                &format!("Failed to update peers config: {}", e),
                                                None,
                                            );
                                        }
                                    }
                                    _ => {
                                        CliOutput::info(
                                            &format!("Received message: {:?}", message),
                                            None,
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                CliOutput::error(
                                    &format!("Connection to leader lost: {}", e),
                                    None,
                                );
                                break;
                            }
                        }
                    }
                }
                Ok(other_message) => {
                    CliOutput::error(
                        &format!("Unexpected message from leader: {:?}", other_message),
                        None,
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    CliOutput::error(
                        &format!("Failed to receive acknowledgment from leader: {}", e),
                        None,
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
