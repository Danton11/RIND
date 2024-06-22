use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::error::Error;
use std::sync::Arc;
use log::{info, debug, error};

use crate::packet;
use crate::query;

/// Runs the DNS server, binding to the specified address and handling incoming packets.
///
/// # Parameters
/// - `addr`: A string slice that represents the address to bind the server to. For example, `"127.0.0.1:5353"`.
///
/// # Returns
/// - `Result<(), Box<dyn Error>>`: 
///   - `Ok(())`: Indicates the server started successfully and is running.
///   - `Err(Box<dyn Error>)`: Indicates an error occurred while starting or running the server. The error is boxed and can be any type that implements the `Error` trait.
pub async fn run(addr: &str) -> Result<(), Box<dyn Error>> {
    let socket = Arc::new(UdpSocket::bind(addr).await?);
    let (tx, mut rx) = mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(1024);

    // Log that the server has successfully started
    info!("DNS server listening on {}", addr);
    info!("Server has successfully started");

    // Spawn a task to handle incoming packets
    let socket_clone = Arc::clone(&socket);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 512];
        loop {
            match socket_clone.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    debug!("Received packet from {}: {:?}", addr, &buf[..len]);
                    if tx.send((buf[..len].to_vec(), addr)).await.is_err() {
                        error!("Receiver dropped");
                        break;
                    }
                },
                Err(e) => {
                    error!("Failed to receive packet: {}", e);
                }
            }
        }
    });

    // Handle packets
    while let Some((packet, addr)) = rx.recv().await {
        let socket_clone = Arc::clone(&socket);
        tokio::spawn(async move {
            debug!("Spawning task to handle packet from {}",addr);
            handle_packet(packet, addr, socket_clone).await;
        });
    }

    Ok(())
}

/// Handles a received DNS packet by parsing it, processing the query, and sending a response.
///
/// # Parameters
/// - `packet`: A vector of bytes representing the received DNS packet.
/// - `addr`: The socket address of the sender of the packet.
/// - `socket`: An atomic reference-counted pointer to a `UdpSocket`, allowing the socket to be shared across tasks.
///
/// # Returns
/// - This function does not return a value. It performs its work asynchronously.
async fn handle_packet(packet: Vec<u8>, addr: std::net::SocketAddr, socket: Arc<UdpSocket>) {
    debug!("Handling packet from {}", addr);
    match packet::parse(&packet) {
        Ok(query) => {
            debug!("Successfully parsed query from {}", addr);
            let response = query::handle_query(query);
            if let Err(e) = socket.send_to(&response, &addr).await {
                error!("Failed to send response to {}: {}", addr, e);
            } else {
                debug!("Successfully sent response to {}: response = {:?}", addr, response);
            }
        }
        Err(e) => error!("Failed to parse packet: {}", e),
    }
}

