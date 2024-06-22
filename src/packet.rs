use std::error::Error;
use std::net::Ipv4Addr;
use log::{debug,error};

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

    // Ensure the packet is at least 12 bytes long (minimum DNS header size)
    if packet.len() < 12 {
        error!("Packet too short: expected at least 12 bytes but got {}", packet.len());
        return Err(format!("Packet too short: expected at least 12 bytes but got {}", packet.len()).into());
    }

    // Read the header fields from the packet
    let id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let qd_count = u16::from_be_bytes([packet[4], packet[5]]);
    let an_count = u16::from_be_bytes([packet[6], packet[7]]);
    let ns_count = u16::from_be_bytes([packet[8], packet[9]]);
    let ar_count = u16::from_be_bytes([packet[10], packet[11]]);
    
    debug!("ID: {}, Flags: {}, QD Count: {}, AN Count: {}, NS Count: {}, AR Count: {}", id, flags, qd_count, an_count, ns_count, ar_count);

    // Only handle packets with a single question
    if qd_count != 1 {
        return Err("Multiple questions not supported".into());
    }

    // Parse the question section of the packet
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

    // Read the query type and class fields
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

        // Ensure the packet is long enough to contain the OPT record fields
        if packet.len() < offset + 10 {
            return Err(format!(
                "Packet too short for OPT record, expected at least {} bytes but got {}",
                offset + 10,
                packet.len()
            ).into());
        }

        // Read the OPT record fields
        let opt_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let opt_udp_payload_size = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
        let opt_extended_rcode = packet[offset + 4];
        let opt_edns_version = packet[offset + 5];
        let opt_z = u16::from_be_bytes([packet[offset + 6], packet[offset + 7]]);
        let opt_data_length = u16::from_be_bytes([packet[offset + 8], packet[offset + 9]]);
        offset += 10;

        // Check if the record is indeed an OPT record
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

    // Return the parsed DNS query
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
        // Read the length of the next label
        let len = *packet.get(offset).ok_or("Unexpected end of packet while reading name length")? as usize;
        debug!("Reading label length: {}, at offset: {}", len, offset);
        
        // Check for the end of the name (a zero-length label)
        if len == 0 {
            offset += 1; // Move past the zero-length label
            debug!("End of name at offset: {}", offset);
            break;
        }

        // Append a dot to separate labels (except for the first label)
        if !name.is_empty() {
            name.push('.');
        }

        offset += 1;

        // Ensure the packet is long enough to contain the label
        if packet.len() < offset + len {
            return Err(format!(
                "Unexpected end of packet while reading name, expected at least {} bytes but got {}",
                offset + len,
                packet.len()
            ).into());
        }

        // Read the label and append it to the name
        let label = std::str::from_utf8(&packet[offset..offset + len])?;
        debug!("Read label: {}, at offset range: {}-{}", label, offset, offset + len);
        name.push_str(label);
        offset += len;
    }

    // Calculate the total length of the name
    let name_len = offset - start_offset;
    debug!("Total name length: {}, final offset after name: {}", name_len, offset);
    Ok((name, offset))
}

/// Builds a DNS response packet based on the given query and IP address.
///
/// # Parameters
/// - `query`: A `DnsQuery` struct representing the DNS query to respond to.
/// - `ip`: An `Ipv4Addr` representing the IP address to include in the DNS response.
/// - `response_code`: An `u8` representing the DNS response code.
/// - `ttl`: A `u32` representing the TTL (Time To Live) of the DNS record.
/// - `record_type`: A `String` representing the DNS record type.
/// - `class`: A `String` representing the DNS record class.
///
/// # Returns
/// - `Vec<u8>`: A vector of bytes representing the DNS response packet.
pub fn build_response(query: DnsQuery, ip: Ipv4Addr, response_code: u8, ttl: u32, record_type: String, class: String) -> Vec<u8> {
    let mut response = Vec::new();

    // Header
    response.extend(&query.id.to_be_bytes());
    response.extend(&(query.flags | 0x8000).to_be_bytes()); // Set response flag
    response.extend(&1u16.to_be_bytes()); // QDCOUNT
    response.extend(&1u16.to_be_bytes()); // ANCOUNT
    response.extend(&0u16.to_be_bytes()); // NSCOUNT
    response.extend(&1u16.to_be_bytes()); // ARCOUNT

    // Question
    for question in query.questions.iter() {
        response.extend(encode_name(&question.name));
        response.extend(&question.qtype.to_be_bytes());
        response.extend(&question.qclass.to_be_bytes());
    }

    // Answer
    if response_code == 0 {
        for question in query.questions.iter() {
            response.extend(encode_name(&question.name));
            response.extend(&1u16.to_be_bytes()); // TYPE A
            response.extend(&1u16.to_be_bytes()); // CLASS IN
            response.extend(&ttl.to_be_bytes()); // TTL
            response.extend(&4u16.to_be_bytes()); // RDLENGTH
            response.extend(&ip.octets());
        }
    }

    // Add OPT record to the response
    response.extend(&[0u8][..]); // Name (root)
    response.extend(&41u16.to_be_bytes()); // Type (OPT)
    response.extend(&4096u16.to_be_bytes()); // UDP payload size
    response.extend(&0u32.to_be_bytes()); // Extended RCODE and flags
    response.extend(&0u16.to_be_bytes()); // RDLENGTH

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
