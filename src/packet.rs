use crate::update::RecordData;
use log::debug;
use std::error::Error;

/// Parses a DNS packet from bytes
pub fn parse(packet: &[u8]) -> Result<DnsQuery, Box<dyn Error + Send + Sync>> {
    debug!("Parsing packet of {} bytes", packet.len());

    if packet.len() < 12 {
        return Err(format!("Packet too short: {} bytes", packet.len()).into());
    }

    // Parse DNS header
    let id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let qd_count = u16::from_be_bytes([packet[4], packet[5]]);
    let _an_count = u16::from_be_bytes([packet[6], packet[7]]);
    let _ns_count = u16::from_be_bytes([packet[8], packet[9]]);
    let ar_count = u16::from_be_bytes([packet[10], packet[11]]);

    debug!("Header - ID: {}, Questions: {}", id, qd_count);

    if qd_count != 1 {
        return Err("Multiple questions not supported".into());
    }

    // Parse question section
    let mut offset = 12;
    let (name, new_offset) = read_name(packet, offset)?;
    offset = new_offset;

    if packet.len() < offset + 4 {
        return Err("Packet too short for query type/class".into());
    }

    let qtype = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
    let qclass = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
    offset += 4;

    let mut has_opt = false;
    let mut opt_payload_size = 512;

    // Check for OPT record (EDNS0)
    if ar_count > 0 {
        let (_opt_name, new_offset) = read_name(packet, offset)?;
        offset = new_offset;

        if packet.len() >= offset + 10 {
            let opt_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
            let opt_udp_payload_size = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
            let _opt_data_length = u16::from_be_bytes([packet[offset + 8], packet[offset + 9]]);

            if opt_type == 41 {
                has_opt = true;
                opt_payload_size = opt_udp_payload_size;
                debug!(
                    "Found OPT record with payload size: {}",
                    opt_udp_payload_size
                );
            }
        }
    }

    Ok(DnsQuery {
        id,
        flags,
        questions: vec![Question {
            name,
            qtype,
            qclass,
        }],
        has_opt,
        opt_payload_size,
    })
}

/// Represents a DNS query.
#[derive(Debug, Clone)]
pub struct DnsQuery {
    pub id: u16,
    pub flags: u16,
    pub questions: Vec<Question>,
    #[allow(dead_code)]
    pub has_opt: bool, // Indicates if the query had an OPT record
    #[allow(dead_code)]
    pub opt_payload_size: u16, // The UDP payload size from the OPT record
}

/// Represents a DNS question.
#[derive(Debug, Clone)]
pub struct Question {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

/// Reads a domain name from DNS packet at given offset
fn read_name(
    packet: &[u8],
    mut offset: usize,
) -> Result<(String, usize), Box<dyn Error + Send + Sync>> {
    let mut name = String::new();

    loop {
        let len = *packet.get(offset).ok_or("Unexpected end of packet")? as usize;

        if len == 0 {
            offset += 1;
            break;
        }

        if !name.is_empty() {
            name.push('.');
        }

        offset += 1;

        if packet.len() < offset + len {
            return Err("Packet too short for label".into());
        }

        let label = std::str::from_utf8(&packet[offset..offset + len])?;
        name.push_str(label);
        offset += len;
    }

    Ok((name, offset))
}

/// Builds a DNS response packet.
///
/// `answers` is the answer section RRSet, each entry a `(&RecordData, ttl)`
/// tuple. Empty slice = error/NODATA/NXDOMAIN (ANCOUNT=0). Non-empty slice =
/// one answer RR per entry, ANCOUNT=N. Per RFC 2181 §5.2 all entries in a
/// single RRSet should share a TTL — the caller (`query.rs`) is responsible
/// for clamping to the min TTL before calling here.
///
/// `response_code` is the RCODE in the header flags (0=NOERROR, 3=NXDOMAIN, ...).
pub fn build_response(
    query: DnsQuery,
    answers: &[(&RecordData, u32)],
    response_code: u8,
) -> Vec<u8> {
    let mut response = Vec::new();

    // Header
    response.extend(&query.id.to_be_bytes());
    let flags_with_rcode = (query.flags | 0x8000) | (response_code as u16);
    response.extend(&flags_with_rcode.to_be_bytes());
    response.extend(&(query.questions.len() as u16).to_be_bytes()); // QDCOUNT
    let ancount = (query.questions.len() * answers.len()) as u16;
    response.extend(&ancount.to_be_bytes()); // ANCOUNT
    response.extend(&0u16.to_be_bytes()); // NSCOUNT
    response.extend(&1u16.to_be_bytes()); // ARCOUNT (for OPT)

    debug!(
        "Building response: response_code={}, ancount={}",
        response_code, ancount
    );

    // Question section (echo back)
    for question in query.questions.iter() {
        response.extend(encode_name(&question.name));
        response.extend(&question.qtype.to_be_bytes());
        response.extend(&question.qclass.to_be_bytes());
    }

    // Answer section: one RR per (question, answer) pair.
    for question in query.questions.iter() {
        for (data, ttl) in answers.iter() {
            response.extend(encode_name(&question.name));
            response.extend(&data.type_code().to_be_bytes());
            response.extend(&1u16.to_be_bytes()); // CLASS IN
            response.extend(&ttl.to_be_bytes());

            // RDLENGTH + RDATA depend on the variant. Exhaustive match so
            // adding a new RecordData variant is a compile-time reminder
            // to teach the encoder about it.
            match data {
                RecordData::A { ip } => {
                    response.extend(&4u16.to_be_bytes()); // RDLENGTH
                    response.extend(&ip.octets());
                }
                RecordData::Aaaa { ip } => {
                    response.extend(&16u16.to_be_bytes()); // RDLENGTH
                    response.extend(&ip.octets());
                }
                RecordData::Cname { target }
                | RecordData::Ptr { target }
                | RecordData::Ns { target } => {
                    // RDATA is the target encoded as an uncompressed domain
                    // name (RFC 1035 §3.3.1 CNAME, §3.3.11 NS, §3.3.12 PTR).
                    // No pointer compression — encoder doesn't track offsets.
                    let encoded = encode_name(target);
                    response.extend(&(encoded.len() as u16).to_be_bytes()); // RDLENGTH
                    response.extend(&encoded);
                }
                RecordData::Mx {
                    preference,
                    exchange,
                } => {
                    // RFC 1035 §3.3.9: 16-bit preference then uncompressed
                    // exchange name. Clients sort by preference; we don't.
                    let encoded = encode_name(exchange);
                    let rdlen = 2 + encoded.len();
                    response.extend(&(rdlen as u16).to_be_bytes()); // RDLENGTH
                    response.extend(&preference.to_be_bytes());
                    response.extend(&encoded);
                }
                RecordData::Txt { strings } => {
                    // RFC 1035 §3.3.14: RDATA is one or more <character-string>.
                    // Each character-string is a 1-octet length followed by
                    // that many bytes. RDLENGTH = sum of (1 + len) across all.
                    // Per-string ≤255-byte limit is already enforced at write
                    // time via `validate_rdata`, so the `as u8` cast is safe.
                    let rdlen: usize = strings.iter().map(|s| 1 + s.len()).sum();
                    response.extend(&(rdlen as u16).to_be_bytes()); // RDLENGTH
                    for s in strings.iter() {
                        response.push(s.len() as u8);
                        response.extend(s.as_bytes());
                    }
                }
            }
        }
    }

    // OPT record (EDNS0) in additional section
    response.extend(&[0u8][..]); // Name (root)
    response.extend(&41u16.to_be_bytes()); // Type (OPT)
    response.extend(&4096u16.to_be_bytes()); // UDP payload size
    response.extend(&0u32.to_be_bytes()); // Extended RCODE and flags
    response.extend(&0u16.to_be_bytes()); // RDLENGTH

    response
}

/// Encodes domain name into DNS wire format
fn encode_name(name: &str) -> Vec<u8> {
    let mut encoded = Vec::new();
    for label in name.split('.') {
        encoded.push(label.len() as u8);
        encoded.extend(label.as_bytes());
    }
    encoded.push(0);
    encoded
}
