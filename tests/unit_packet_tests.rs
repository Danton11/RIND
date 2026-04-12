use rind::packet::{build_response, parse, DnsQuery, Question};
use rind::update::RecordData;
use std::net::{Ipv4Addr, Ipv6Addr};

fn make_query(name: &str, qtype: u16) -> DnsQuery {
    DnsQuery {
        id: 0x1234,
        flags: 0x0100,
        questions: vec![Question {
            name: name.to_string(),
            qtype,
            qclass: 1,
        }],
        has_opt: false,
        opt_payload_size: 512,
    }
}

#[test]
fn test_read_name_through_parse() {
    // Test read_name functionality through parse since it's private
    let packet = [
        0x12, 0x34, // ID
        0x01, 0x00, // Flags
        0x00, 0x01, // QDCOUNT
        0x00, 0x00, // ANCOUNT
        0x00, 0x00, // NSCOUNT
        0x00, 0x00, // ARCOUNT
        3, b'w', b'w', b'w', 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm',
        0, // www.example.com
        0x00, 0x01, // QTYPE: A
        0x00, 0x01, // QCLASS: IN
    ];
    let result = parse(&packet);
    assert!(result.is_ok());
    let query = result.unwrap();
    assert_eq!(query.questions[0].name, "www.example.com");
}

#[test]
fn test_parse_valid_packet() {
    let packet = [
        0x12, 0x34, // ID
        0x01, 0x00, // Flags
        0x00, 0x01, // QDCOUNT
        0x00, 0x00, // ANCOUNT
        0x00, 0x00, // NSCOUNT
        0x00, 0x00, // ARCOUNT
        1, b'a', 3, b'c', b'o', b'm', 0, // QNAME: a.com
        0x00, 0x01, // QTYPE: A
        0x00, 0x01, // QCLASS: IN
    ];
    let result = parse(&packet);
    assert!(result.is_ok());
    let query = result.unwrap();
    assert_eq!(query.id, 0x1234);
    assert_eq!(query.questions[0].name, "a.com");
    assert_eq!(query.questions[0].qtype, 1);
    assert_eq!(query.questions[0].qclass, 1);
}

#[test]
fn test_parse_too_short_packet() {
    let packet = [0x01, 0x02, 0x03];
    let result = parse(&packet);
    assert!(result.is_err());
}

#[test]
fn test_build_response_basic() {
    let query = DnsQuery {
        id: 0x1234,
        flags: 0x0100,
        questions: vec![Question {
            name: "a.com".to_string(),
            qtype: 1,
            qclass: 1,
        }],
        has_opt: false,
        opt_payload_size: 512,
    };
    let data = RecordData::A {
        ip: Ipv4Addr::new(1, 2, 3, 4),
    };
    let response = build_response(query, Some(&data), 0, 60);
    assert!(response.len() > 20);
    assert_eq!(response[0], 0x12);
    assert_eq!(response[1], 0x34);
}

#[test]
fn test_build_response_a_record_rdata() {
    let query = make_query("a.com", 1);
    let data = RecordData::A {
        ip: Ipv4Addr::new(10, 0, 0, 7),
    };
    let response = build_response(query, Some(&data), 0, 60);

    // QR=1 set in flags
    assert_eq!(response[2] & 0x80, 0x80);
    // ANCOUNT == 1
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 1);
    // Last 4 bytes before the OPT record are the A RDATA.
    // OPT tail is 11 bytes: name(1) + type(2) + udpsz(2) + ttl(4) + rdlen(2).
    let opt_len = 11;
    let rdata_end = response.len() - opt_len;
    let rdata = &response[rdata_end - 4..rdata_end];
    assert_eq!(rdata, &[10, 0, 0, 7]);
    // RDLENGTH right before that == 4
    let rdlen = u16::from_be_bytes([response[rdata_end - 6], response[rdata_end - 5]]);
    assert_eq!(rdlen, 4);
}

#[test]
fn test_build_response_aaaa_record_rdata() {
    let query = make_query("v6.example.com", 28);
    let ip = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
    let data = RecordData::Aaaa { ip };
    let response = build_response(query, Some(&data), 0, 60);

    // ANCOUNT == 1
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 1);

    // The answer TYPE field should be 28 (AAAA). Find it by walking the
    // answer section: after header(12) + question, the answer begins with
    // the encoded name then TYPE(2) CLASS(2) TTL(4) RDLENGTH(2) RDATA(16).
    let opt_tail = 11;
    let rdata_end = response.len() - opt_tail;
    let rdata = &response[rdata_end - 16..rdata_end];
    assert_eq!(rdata, &ip.octets());
    let rdlen = u16::from_be_bytes([response[rdata_end - 18], response[rdata_end - 17]]);
    assert_eq!(rdlen, 16);
}

#[test]
fn test_build_response_nodata_empty_answer() {
    // NODATA: NOERROR (rcode=0) but no answer record → ANCOUNT == 0.
    let query = make_query("a.com", 28);
    let response = build_response(query, None, 0, 60);
    // QR=1, rcode low nibble of byte 3 == 0
    assert_eq!(response[2] & 0x80, 0x80);
    assert_eq!(response[3] & 0x0f, 0);
    // ANCOUNT == 0
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 0);
}

#[test]
fn test_build_response_nxdomain_rcode() {
    let query = make_query("nope.example", 1);
    let response = build_response(query, None, 3, 60);
    // rcode low nibble == 3 (NXDOMAIN)
    assert_eq!(response[3] & 0x0f, 3);
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 0);
}
