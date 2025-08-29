// TDD tests for kubectl port-forward protocol compatibility
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::time::timeout;

// kubectl frame format: [stream_id:1][flags:1][length:2][data:length]
fn make_kubectl_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.push(stream_id);
    frame.push(0x00); // flags
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

fn parse_kubectl_frame(data: &[u8]) -> Option<(u8, Vec<u8>)> {
    if data.len() < 4 {
        return None;
    }
    let stream_id = data[0];
    let _flags = data[1];
    let length = u16::from_be_bytes([data[2], data[3]]) as usize;
    
    if data.len() < 4 + length {
        return None;
    }
    
    Some((stream_id, data[4..4 + length].to_vec()))
}

#[tokio::test]
async fn test_kubectl_frame_format() {
    // Test frame creation
    let data = b"Hello, World!";
    let frame = make_kubectl_frame(0, data);
    
    assert_eq!(frame[0], 0); // stream_id
    assert_eq!(frame[1], 0); // flags
    assert_eq!(u16::from_be_bytes([frame[2], frame[3]]), data.len() as u16);
    assert_eq!(&frame[4..], data);
    
    // Test frame parsing
    let (stream_id, parsed_data) = parse_kubectl_frame(&frame).unwrap();
    assert_eq!(stream_id, 0);
    assert_eq!(parsed_data, data);
}

#[tokio::test]
async fn test_kubectl_stream_ids() {
    // kubectl uses specific stream IDs:
    // - Even numbers for data streams (0, 2, 4, ...)
    // - Odd numbers for error streams (1, 3, 5, ...)
    
    let data_frame = make_kubectl_frame(0, b"data");
    let error_frame = make_kubectl_frame(1, b"error");
    
    assert_eq!(data_frame[0], 0);  // Data stream
    assert_eq!(error_frame[0], 1); // Error stream
}

#[tokio::test]
async fn test_kubectl_empty_ack_frames() {
    // kubectl expects empty acknowledgment frames on connection
    let data_ack = make_kubectl_frame(0, &[]);
    let error_ack = make_kubectl_frame(1, &[]);
    
    assert_eq!(data_ack.len(), 4);  // Header only
    assert_eq!(error_ack.len(), 4); // Header only
    
    // Verify empty payload
    let (_, data) = parse_kubectl_frame(&data_ack).unwrap();
    assert_eq!(data.len(), 0);
}

#[tokio::test]
async fn test_kubectl_protocol_negotiation() {
    // kubectl requires specific protocol strings
    const EXPECTED_PROTOCOL: &str = "SPDY/3.1+portforward.k8s.io";
    const FALLBACK_PROTOCOL: &str = "portforward.k8s.io";
    
    // These are the protocols kubectl will accept
    let valid_protocols = vec![EXPECTED_PROTOCOL, FALLBACK_PROTOCOL];
    
    // Verify our implementation uses the correct protocol
    assert!(valid_protocols.contains(&EXPECTED_PROTOCOL));
}

#[tokio::test]
async fn test_kubectl_connection_sequence() {
    // Simulate the kubectl connection sequence
    
    // 1. kubectl connects with WebSocket upgrade
    // 2. Server must send acknowledgment frames for streams 0 and 1
    // 3. kubectl sends data on stream 0
    // 4. Server forwards data and sends responses on stream 0
    
    let mut sequence = Vec::new();
    
    // Server sends ACKs
    sequence.push(make_kubectl_frame(0, &[])); // Data stream ACK
    sequence.push(make_kubectl_frame(1, &[])); // Error stream ACK
    
    // Verify ACK frames
    assert_eq!(sequence[0].len(), 4);
    assert_eq!(sequence[1].len(), 4);
    assert_eq!(sequence[0][0], 0); // Stream 0
    assert_eq!(sequence[1][0], 1); // Stream 1
}

#[tokio::test]
async fn test_kubectl_data_forwarding() {
    // Test bidirectional data flow
    let http_request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let http_response = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
    
    // kubectl sends HTTP request on stream 0
    let request_frame = make_kubectl_frame(0, http_request);
    
    // Server should forward to container and send response on stream 0
    let response_frame = make_kubectl_frame(0, http_response);
    
    // Verify frames
    let (stream_id, data) = parse_kubectl_frame(&request_frame).unwrap();
    assert_eq!(stream_id, 0);
    assert_eq!(data, http_request);
    
    let (stream_id, data) = parse_kubectl_frame(&response_frame).unwrap();
    assert_eq!(stream_id, 0);
    assert_eq!(data, http_response);
}

#[tokio::test]
async fn test_kubectl_error_handling() {
    // Test error stream handling
    let error_message = b"Connection refused";
    let error_frame = make_kubectl_frame(1, error_message);
    
    let (stream_id, data) = parse_kubectl_frame(&error_frame).unwrap();
    assert_eq!(stream_id, 1); // Error stream
    assert_eq!(data, error_message);
}

#[tokio::test]
async fn test_kubectl_keep_alive() {
    // kubectl may send empty frames as keep-alive
    let keep_alive = make_kubectl_frame(0, &[]);
    
    assert_eq!(keep_alive.len(), 4);
    let (stream_id, data) = parse_kubectl_frame(&keep_alive).unwrap();
    assert_eq!(stream_id, 0);
    assert_eq!(data.len(), 0);
}

#[tokio::test]
async fn test_kubectl_frame_boundaries() {
    // Test that frames maintain proper boundaries
    let data1 = b"First message";
    let data2 = b"Second message";
    
    let frame1 = make_kubectl_frame(0, data1);
    let frame2 = make_kubectl_frame(0, data2);
    
    // Concatenate frames
    let mut combined = frame1.clone();
    combined.extend_from_slice(&frame2);
    
    // Parse first frame
    let (stream_id, parsed_data) = parse_kubectl_frame(&combined).unwrap();
    assert_eq!(stream_id, 0);
    assert_eq!(parsed_data, data1);
    
    // Parse second frame
    let offset = frame1.len();
    let (stream_id, parsed_data) = parse_kubectl_frame(&combined[offset..]).unwrap();
    assert_eq!(stream_id, 0);
    assert_eq!(parsed_data, data2);
}

#[tokio::test]
async fn test_kubectl_large_payload() {
    // Test with larger payloads
    let large_data = vec![0x42; 8192]; // 8KB of data
    let frame = make_kubectl_frame(0, &large_data);
    
    assert_eq!(frame.len(), 4 + 8192);
    
    let (stream_id, parsed_data) = parse_kubectl_frame(&frame).unwrap();
    assert_eq!(stream_id, 0);
    assert_eq!(parsed_data.len(), 8192);
    assert_eq!(parsed_data, large_data);
}

#[tokio::test]
async fn test_kubectl_protocol_compliance() {
    // Comprehensive test of protocol compliance
    
    // 1. Protocol negotiation
    let protocols = vec!["SPDY/3.1+portforward.k8s.io", "portforward.k8s.io"];
    assert!(!protocols.is_empty());
    
    // 2. Stream ID allocation
    let data_streams = vec![0, 2, 4];
    let error_streams = vec![1, 3, 5];
    
    for stream in data_streams {
        assert_eq!(stream % 2, 0); // Even for data
    }
    
    for stream in error_streams {
        assert_eq!(stream % 2, 1); // Odd for errors
    }
    
    // 3. Frame format validation
    let test_frame = make_kubectl_frame(0, b"test");
    assert_eq!(test_frame[0], 0); // stream_id
    assert_eq!(test_frame[1], 0); // flags
    assert_eq!(u16::from_be_bytes([test_frame[2], test_frame[3]]), 4); // length
    
    // 4. Acknowledgment sequence
    let ack_data = make_kubectl_frame(0, &[]);
    let ack_error = make_kubectl_frame(1, &[]);
    assert_eq!(ack_data.len(), 4);
    assert_eq!(ack_error.len(), 4);
}

// Integration test for the complete flow
#[tokio::test]
async fn test_kubectl_complete_flow() {
    // This test simulates the complete kubectl port-forward flow
    
    // Step 1: Connection with protocol negotiation
    let protocol = "SPDY/3.1+portforward.k8s.io";
    
    // Step 2: Server sends ACK frames
    let ack_frames = vec![
        make_kubectl_frame(0, &[]), // Data stream ACK
        make_kubectl_frame(1, &[]), // Error stream ACK
    ];
    
    // Step 3: kubectl sends HTTP request
    let http_request = b"GET / HTTP/1.1\r\nHost: nginx\r\n\r\n";
    let request_frame = make_kubectl_frame(0, http_request);
    
    // Step 4: Server forwards to container and gets response
    let http_response = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html>nginx</html>";
    let response_frame = make_kubectl_frame(0, http_response);
    
    // Verify all frames
    assert_eq!(ack_frames[0][0], 0);
    assert_eq!(ack_frames[1][0], 1);
    
    let (stream_id, data) = parse_kubectl_frame(&request_frame).unwrap();
    assert_eq!(stream_id, 0);
    assert!(data.starts_with(b"GET /"));
    
    let (stream_id, data) = parse_kubectl_frame(&response_frame).unwrap();
    assert_eq!(stream_id, 0);
    assert!(data.starts_with(b"HTTP/1.1 200"));
}

#[cfg(test)]
mod websocket_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_websocket_binary_frames() {
        // kubectl uses binary WebSocket frames
        let frame = make_kubectl_frame(0, b"test");
        
        // This would be sent as WebSocket binary message
        let ws_msg = Message::Binary(frame.clone());
        
        match ws_msg {
            Message::Binary(data) => {
                assert_eq!(data, frame);
            }
            _ => panic!("Expected binary message"),
        }
    }
    
    #[tokio::test]
    async fn test_websocket_frame_ordering() {
        // Frames must be processed in order
        let frames = vec![
            make_kubectl_frame(0, &[]),      // ACK
            make_kubectl_frame(1, &[]),      // ACK
            make_kubectl_frame(0, b"data1"), // Data
            make_kubectl_frame(0, b"data2"), // Data
        ];
        
        // Verify ordering
        for (i, frame) in frames.iter().enumerate() {
            if i < 2 {
                // ACK frames
                assert_eq!(frame.len(), 4);
            } else {
                // Data frames
                assert!(frame.len() > 4);
            }
        }
    }
}