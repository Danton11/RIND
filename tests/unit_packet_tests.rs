use std::net::Ipv4Addr;
use rind::packet::{parse, build_response, DnsQuery, Question};

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
        3, b'w', b'w', b'w', 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0, // www.example.com
        0x00, 0x01, // QTYPE: A
        0x00, 0x01  // QCLASS: IN
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
        0x00, 0x01  // QCLASS: IN
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
    let ip = Ipv4Addr::new(1, 2, 3, 4);
    let response = build_response(query, ip, 0, 60, "A".to_string(), "IN".to_string());
    assert!(response.len() > 20);
    assert_eq!(response[0], 0x12);
    assert_eq!(response[1], 0x34);
}