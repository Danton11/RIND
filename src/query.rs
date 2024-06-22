use std::net::Ipv4Addr;
use crate::packet::{DnsQuery, build_response};
use log::debug;

/// Handles a DNS query and generates a response packet.
///
/// # Parameters
/// - `query`: A `DnsQuery` representing the DNS query to handle.
///
/// # Returns
/// - `Vec<u8>`: A vector of bytes representing the DNS response packet.
pub fn handle_query(query: DnsQuery) -> Vec<u8> {
    debug!("Handling query {:?}", query);
    // Handle a simple fixed set of A record queries
    let ip = match query.questions[0].name.as_str() {
        "example.com" => Ipv4Addr::new(93, 184, 216, 34), // Example IP
        "localhost" => Ipv4Addr::new(127, 0, 0, 1),      // Localhost IP
        _ => Ipv4Addr::new(0, 0, 0, 0),                  // Default to 0.0.0.0
    };
    build_response(query, ip)
}
