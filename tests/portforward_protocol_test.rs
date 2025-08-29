// Comprehensive tests for kubectl port-forward protocol implementation
use krust::api::kubectl_portforward::*;

#[test]
fn test_kubectl_stream_ids() {
    // kubectl uses specific stream IDs for port forwarding
    // Based on Kubernetes source, streams are numbered:
    // - Stream 0: data for port 1
    // - Stream 1: error for port 1  
    // - Stream 2: data for port 2
    // - Stream 3: error for port 2
    // etc.
    
    assert_eq!(get_data_stream_id(0), 0);
    assert_eq!(get_error_stream_id(0), 1);
    assert_eq!(get_data_stream_id(1), 2);
    assert_eq!(get_error_stream_id(1), 3);
}

#[test]
fn test_kubectl_initial_frames() {
    // kubectl sends specific initialization frames
    // These are not standard SPDY but kubectl's own format
    
    // First frame is always [0x80, 0x01] for protocol version
    let init_frame = create_kubectl_init_frame();
    assert_eq!(init_frame[0], 0x80);
    assert_eq!(init_frame[1], 0x01);
}

#[test]
fn test_kubectl_port_header() {
    // kubectl sends port information in a specific format
    // The port is sent as 2 bytes in network byte order
    
    let port_frame = create_port_frame(8080, 80);
    // 8080 = 0x1F90, 80 = 0x0050
    assert_eq!(port_frame.len(), 4);
    assert_eq!(port_frame[0], 0x1F);
    assert_eq!(port_frame[1], 0x90);
    assert_eq!(port_frame[2], 0x00);
    assert_eq!(port_frame[3], 0x50);
}

#[test]
fn test_kubectl_data_frame() {
    // kubectl data frames have a specific format:
    // [stream_id:1][flags:1][length:2][data:n]
    
    let data = b"Hello, World!";
    let frame = create_kubectl_data_frame(0, data);
    
    assert_eq!(frame[0], 0); // stream_id
    assert_eq!(frame[1], 0); // flags
    assert_eq!(u16::from_be_bytes([frame[2], frame[3]]), data.len() as u16);
    assert_eq!(&frame[4..], data);
}

#[test]
fn test_parse_kubectl_frame() {
    // Test parsing kubectl frames
    let data = b"Test data";
    let frame = create_kubectl_data_frame(2, data);
    
    let parsed = parse_kubectl_frame(&frame).unwrap();
    assert_eq!(parsed.stream_id, 2);
    assert_eq!(parsed.flags, 0);
    assert_eq!(parsed.data, data);
}

#[test]
fn test_kubectl_error_frame() {
    // Error frames use odd stream IDs
    let error_msg = b"Connection refused";
    let frame = create_kubectl_error_frame(0, error_msg);
    
    assert_eq!(frame[0], 1); // error stream for port 0
    assert_eq!(&frame[4..], error_msg);
}

#[test]
fn test_kubectl_close_frame() {
    // kubectl sends specific frames to close streams
    let close_frame = create_kubectl_close_frame(0);
    
    assert_eq!(close_frame[0], 0); // stream_id
    assert_eq!(close_frame[1], 0x01); // FIN flag
    assert_eq!(u16::from_be_bytes([close_frame[2], close_frame[3]]), 0); // length = 0
}