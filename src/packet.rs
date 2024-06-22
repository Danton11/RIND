use std::error::Error;
use std::net::Ipv4Addr;
use log::{debug, warn, error};

/// Parses a DNS packet from the given byte slice.
///
/// # Parameters
/// - `packet`: A byte slice representing the DNS packet to parse.
///
/// # Returns
/// - `Result<DnsQuery, Box<dyn Error + Send + Sync>>`:
///   - `Ok(DnsQuery)`: If the packet was successfully parsed.
///   - `Err(Box<dyn Error + Send + Sync>)`: If there was an error during parsing. The error is boxed and can be any type that implements the `Error`, `Send`, and `Sync` traits.
pub fn parse(packet: &[u8]) -> Result<DnsQuery, Box<dyn Error + Send + Sync>> {
    debug!("Starting packet parsing");
    debug!("Packet size: {}", packet.len());
    debug!("Packet contents: {:?}", packet);

    // Simplified DNS packet parsing
    if packet.len() < 12 {
        error!("Packet too short: expected at least 12 bytes but got {}", packet.len());
        return Err(format!("Packet too short: expected at least 12 bytes but got {}", packet.len()).into());
    }

    // Read the header
    let id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let qd_count = u16::from_be_bytes([packet[4], packet[5]]);
    let an_count = u16::from_be_bytes([packet[6], packet[7]]);
    let ns_count = u16::from_be_bytes([packet[8], packet[9]]);
    let ar_count = u16::from_be_bytes([packet[10], packet[11]]);
    debug!("");
    debug!("ID: {}, Flags: {}, QD Count: {}, AN Count: {}, NS Count: {}, AR Count: {}", id, flags, qd_count, an_count, ns_count, ar_count);

    // Read the question section (only handling a single question)
    if qd_count != 1 {
        return Err("Multiple questions not supported".into());
    }

    let mut offset = 12;
    let (name, new_offset) = read_name(packet, offset)?;
    debug!("Parsed name: {}, new offset: {}", name, new_offset);
    offset = new_offset;

    // Ensure the packet is long enough to contain the query type and class
    if packet.len() < offset + 4 {
        return Err(format!(
            "Packet too short for query type/class, expected at least {} bytes but got {}",
            offset + 4,
            packet.len()
        ).into());
    }

    debug!("Reading query type and class at offset: {}", offset);
    let qtype = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
    let qclass = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
    offset += 4;
    debug!("Query Type: {}, Query Class: {}", qtype, qclass);

    let mut has_opt = false;
    let mut opt_payload_size = 512;

    // Check for OPT record (EDNS0) in the additional section
    if ar_count > 0 {
        debug!("Checking for OPT record in the additional section at offset: {}", offset);
        let (opt_name, new_offset) = read_name(packet, offset)?;
        debug!("Parsed OPT name: {}, new offset: {}", opt_name, new_offset);
        offset = new_offset;

        if packet.len() < offset + 10 {
            return Err(format!(
                "Packet too short for OPT record, expected at least {} bytes but got {}",
                offset + 10,
                packet.len()
            ).into());
        }

        let opt_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let opt_udp_payload_size = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
        let opt_extended_rcode = packet[offset + 4];
        let opt_edns_version = packet[offset + 5];
        let opt_z = u16::from_be_bytes([packet[offset + 6], packet[offset + 7]]);
        let opt_data_length = u16::from_be_bytes([packet[offset + 8], packet[offset + 9]]);
        offset += 10;

        if opt_type == 41 {
            has_opt = true;
            opt_payload_size = opt_udp_payload_size;
            debug!("Detected OPT record: UDP payload size: {}, extended RCODE: {}, EDNS version: {}, Z: {}, data length: {}", opt_udp_payload_size, opt_extended_rcode, opt_edns_version, opt_z, opt_data_length);
            offset += opt_data_length as usize;
        }
    }

    debug!("Final offset: {}, Packet length: {}", offset, packet.len());
    debug!("Remaining bytes in packet: {}", packet.len() - offset);
    debug!("Packet parsing completed");

    Ok(DnsQuery {
        id,
        flags,
        questions: vec![Question { name, qtype, qclass }],
        has_opt,
        opt_payload_size,
    })
}

/// Represents a DNS query.
#[derive(Debug)]
pub struct DnsQuery {
    pub id: u16,
    pub flags: u16,
    pub questions: Vec<Question>,
    pub has_opt: bool, // Indicates if the query had an OPT record
    pub opt_payload_size: u16, // The UDP payload size from the OPT record
}

/// Represents a DNS question.
#[derive(Debug)]
pub struct Question {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

/// Reads and decodes a domain name from a DNS packet starting at the given offset.
///
/// DNS names are encoded as a sequence of labels. Each label is prefixed with a length byte, and the name is terminated by a zero-length label.
///
/// # Parameters
/// - `packet`: A byte slice representing the DNS packet.
/// - `offset`: The starting position in the packet where the name begins.
///
/// # Returns
/// - `Result<(String, usize), Box<dyn Error + Send + Sync>>`:
///   - `Ok((String, usize))`: On success, returns a tuple containing the decoded domain name as a `String` and the new offset after the name.
///   - `Err(Box<dyn Error + Send + Sync>)`: On failure, returns an error wrapped in a `Box` that implements the `Error`, `Send`, and `Sync` traits.
///
/// # Example
/// ```
/// let packet = [
///     3, b'w', b'w', b'w', 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
///     3, b'c', b'o', b'm', 0
/// ];
/// let (name, new_offset) = read_name(&packet, 0).unwrap();
/// assert_eq!(name, "www.example.com");
/// assert_eq!(new_offset, 17);
/// ```
fn read_name(packet: &[u8], mut offset: usize) -> Result<(String, usize), Box<dyn Error + Send + Sync>> {
    let mut name = String::new();
    let start_offset = offset;
    loop {
        let len = *packet.get(offset).ok_or("Unexpected end of packet while reading name length")? as usize;
        debug!("Reading label length: {}, at offset: {}", len, offset);
        if len == 0 {
            offset += 1; // Move past the zero-length label
            debug!("End of name at offset: {}", offset);
            break;
        }
        if !name.is_empty() {
            name.push('.');
        }
        offset += 1;
        if packet.len() < offset + len {
            return Err(format!(
                "Unexpected end of packet while reading name, expected at least {} bytes but got {}",
                offset + len,
                packet.len()
            ).into());
        }
        let label = std::str::from_utf8(&packet[offset..offset + len])?;
        debug!("Read label: {}, at offset range: {}-{}", label, offset, offset + len);
        name.push_str(label);
        offset += len;
    }
    let name_len = offset - start_offset;
    debug!("Total name length: {}, final offset after name: {}", name_len, offset);
    Ok((name, offset))
}

/// Builds a DNS response packet based on the given query and IP address.
///
/// # Parameters
/// - `query`: A `DnsQuery` struct representing the DNS query to respond to.
/// - `ip`: An `Ipv4Addr` representing the IP address to include in the DNS response.
///
/// # Returns
/// - `Vec<u8>`: A vector of bytes representing the DNS response packet.
///
/// # Example
/// ```
/// let query = DnsQuery {
///     id: 1234,
///     flags: 0x0100,
///     questions: vec![
///         Question {
///             name: "example.com".to_string(),
///             qtype: 1,
///             qclass: 1,
///         }
///     ],
/// };
/// let ip = Ipv4Addr::new(93, 184, 216, 34);
/// let response = build_response(query, ip);
/// ```
pub fn build_response(query: DnsQuery, ip: Ipv4Addr) -> Vec<u8> {
    let mut response = Vec::new();

    // Header
    response.extend(&query.id.to_be_bytes());
    response.extend(&(query.flags | 0x8000).to_be_bytes()); // Set response flag
    response.extend(&1u16.to_be_bytes()); // QDCOUNT
    response.extend(&1u16.to_be_bytes()); // ANCOUNT
    response.extend(&0u16.to_be_bytes()); // NSCOUNT
    if query.has_opt {
        response.extend(&1u16.to_be_bytes()); // ARCOUNT, include OPT record
    } else {
        response.extend(&0u16.to_be_bytes()); // ARCOUNT
    }

    // Question
    for question in query.questions.iter() {
        response.extend(encode_name(&question.name));
        response.extend(&question.qtype.to_be_bytes());
        response.extend(&question.qclass.to_be_bytes());
    }

    // Answer
    for question in query.questions.iter() {
        response.extend(encode_name(&question.name));
        response.extend(&1u16.to_be_bytes()); // TYPE A
        response.extend(&1u16.to_be_bytes()); // CLASS IN
        response.extend(&60u32.to_be_bytes()); // TTL
        response.extend(&4u16.to_be_bytes()); // RDLENGTH
        response.extend(&ip.octets());
    }

    // Add OPT record to the response if it was present in the query
    if query.has_opt {
        response.extend(&[0u8][..]); // Name (root)
        response.extend(&41u16.to_be_bytes()); // Type (OPT)
        response.extend(&query.opt_payload_size.to_be_bytes()); // UDP payload size
        response.extend(&0u32.to_be_bytes()); // Extended RCODE and flags
        response.extend(&0u16.to_be_bytes()); // RDLENGTH
    }

    response
}

/// Encodes a domain name into the DNS wire format.
///
/// # Parameters
/// - `name`: A string slice representing the domain name to encode.
///
/// # Returns
/// - `Vec<u8>`: A vector of bytes representing the encoded domain name.
fn encode_name(name: &str) -> Vec<u8> {
    let mut encoded = Vec::new();
    for label in name.split('.') {
        encoded.push(label.len() as u8);
        encoded.extend(label.as_bytes());
    }
    encoded.push(0);
    encoded
}
