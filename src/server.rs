use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use std::error::Error;
use std::sync::Arc;
use log::{info, debug, error};

use crate::packet;
use crate::query;
use crate::update::DnsRecords;

/// Runs the DNS server on the specified address
pub async fn run(addr: &str, records: Arc<RwLock<DnsRecords>>) -> Result<(), Box<dyn Error>> {
    let socket = Arc::new(UdpSocket::bind(addr).await?);
    let (tx, mut rx) = mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(1024);

    info!("DNS server listening on {}", addr);

    // Task to receive packets and send them to the channel
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

    // Handle packets from the channel
    while let Some((packet, addr)) = rx.recv().await {
        let socket_clone = Arc::clone(&socket);
        let records_clone = Arc::clone(&records);

        tokio::spawn(async move {
            debug!("Handling packet from {}", addr);
            handle_packet(packet, addr, socket_clone, records_clone).await;
        });
    }

    Ok(())
}

/// Parses DNS packet, processes query, and sends response
async fn handle_packet(packet: Vec<u8>, addr: std::net::SocketAddr, socket: Arc<UdpSocket>, records: Arc<RwLock<DnsRecords>>) {
    match packet::parse(&packet) {
        Ok(query) => {
            debug!("Parsed query from {}", addr);
            let response = query::handle_query(query, records).await;
            if let Err(e) = socket.send_to(&response, &addr).await {
                error!("Failed to send response to {}: {}", addr, e);
            } else {
                debug!("Sent response to {}", addr);
            }
        }
        Err(e) => error!("Failed to parse packet: {}", e),
    }
}

