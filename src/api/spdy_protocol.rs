// SPDY/3.1 protocol implementation for kubectl port-forward
use bytes::{BufMut, BytesMut};
use std::collections::HashMap;

// SPDY/3.1 Constants
const SPDY_VERSION: u16 = 3;
const CONTROL_FLAG: u16 = 0x8000;

// Control frame types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlFrameType {
    SynStream = 1,
    SynReply = 2,
    RstStream = 3,
    Settings = 4,
    Ping = 6,
    GoAway = 7,
    Headers = 8,
    WindowUpdate = 9,
}

// SPDY Flags
pub const FLAG_FIN: u8 = 0x01;
pub const FLAG_UNIDIRECTIONAL: u8 = 0x02;
pub const FLAG_CLEAR_SETTINGS: u8 = 0x01;

// Settings IDs
pub const SETTINGS_UPLOAD_BANDWIDTH: u32 = 1;
pub const SETTINGS_DOWNLOAD_BANDWIDTH: u32 = 2;
pub const SETTINGS_ROUND_TRIP_TIME: u32 = 3;
pub const SETTINGS_MAX_CONCURRENT_STREAMS: u32 = 4;
pub const SETTINGS_CURRENT_CWND: u32 = 5;
pub const SETTINGS_DOWNLOAD_RETRANS_RATE: u32 = 6;
pub const SETTINGS_INITIAL_WINDOW_SIZE: u32 = 7;
pub const SETTINGS_CLIENT_CERTIFICATE_VECTOR_SIZE: u32 = 8;

#[derive(Debug)]
pub struct SpdyFrame {
    pub is_control: bool,
    pub stream_id: u32,
    pub flags: u8,
    pub data: Vec<u8>,
    pub frame_type: Option<ControlFrameType>,
}

impl SpdyFrame {
    pub fn new_data_frame(stream_id: u32, flags: u8, data: Vec<u8>) -> Self {
        SpdyFrame {
            is_control: false,
            stream_id,
            flags,
            data,
            frame_type: None,
        }
    }

    pub fn new_control_frame(frame_type: ControlFrameType, flags: u8, stream_id: u32, data: Vec<u8>) -> Self {
        SpdyFrame {
            is_control: true,
            stream_id,
            flags,
            data,
            frame_type: Some(frame_type),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = BytesMut::new();
        
        if self.is_control {
            // Control frame format:
            // +----------------------------------+
            // |C| Version(15bits) | Type(16bits) |
            // +----------------------------------+
            // | Flags (8)  |  Length (24 bits)   |
            // +----------------------------------+
            // |               Data                |
            // +----------------------------------+
            
            // First 16 bits: control bit + version
            let control_version = CONTROL_FLAG | SPDY_VERSION;
            bytes.put_u16(control_version);
            
            // Next 16 bits: frame type
            let frame_type = self.frame_type.unwrap_or(ControlFrameType::SynReply) as u16;
            bytes.put_u16(frame_type);
            
            // Flags (8 bits) and length (24 bits)
            bytes.put_u8(self.flags);
            
            // Length is a 24-bit field
            let length = self.data.len() as u32;
            bytes.put_u8((length >> 16) as u8);
            bytes.put_u8((length >> 8) as u8);
            bytes.put_u8(length as u8);
            
            // Data
            bytes.extend_from_slice(&self.data);
        } else {
            // Data frame format:
            // +----------------------------------+
            // |C|       Stream-ID (31bits)       |
            // +----------------------------------+
            // | Flags (8)  |  Length (24 bits)   |
            // +----------------------------------+
            // |               Data                |
            // +----------------------------------+
            
            // Stream ID (31 bits, C bit is 0 for data frames)
            bytes.put_u32(self.stream_id & 0x7FFFFFFF);
            
            // Flags (8 bits) and length (24 bits)
            bytes.put_u8(self.flags);
            
            // Length is a 24-bit field
            let length = self.data.len() as u32;
            bytes.put_u8((length >> 16) as u8);
            bytes.put_u8((length >> 8) as u8);
            bytes.put_u8(length as u8);
            
            // Data
            bytes.extend_from_slice(&self.data);
        }
        
        bytes.to_vec()
    }
}

pub fn parse_spdy_frame(data: &[u8]) -> Option<SpdyFrame> {
    if data.len() < 8 {
        return None;
    }
    
    // Check if it's a control frame (first bit set)
    let first_word = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let is_control = (first_word & 0x80000000) != 0;
    
    if is_control {
        // Control frame
        let version = ((first_word >> 16) & 0x7FFF) as u16;
        let frame_type = (first_word & 0xFFFF) as u16;
        
        if version != SPDY_VERSION {
            return None;
        }
        
        let flags = data[4];
        let length = u32::from_be_bytes([0, data[5], data[6], data[7]]) as usize;
        
        if data.len() < 8 + length {
            return None;
        }
        
        let frame_type_enum = match frame_type {
            1 => Some(ControlFrameType::SynStream),
            2 => Some(ControlFrameType::SynReply),
            3 => Some(ControlFrameType::RstStream),
            4 => Some(ControlFrameType::Settings),
            6 => Some(ControlFrameType::Ping),
            7 => Some(ControlFrameType::GoAway),
            8 => Some(ControlFrameType::Headers),
            9 => Some(ControlFrameType::WindowUpdate),
            _ => None,
        };
        
        Some(SpdyFrame {
            is_control: true,
            stream_id: 0, // Control frames don't have a stream ID in the first word
            flags,
            data: data[8..8 + length].to_vec(),
            frame_type: frame_type_enum,
        })
    } else {
        // Data frame
        let stream_id = first_word & 0x7FFFFFFF;
        let flags = data[4];
        let length = u32::from_be_bytes([0, data[5], data[6], data[7]]) as usize;
        
        if data.len() < 8 + length {
            return None;
        }
        
        Some(SpdyFrame {
            is_control: false,
            stream_id,
            flags,
            data: data[8..8 + length].to_vec(),
            frame_type: None,
        })
    }
}

// Create a SYN_REPLY frame for stream initialization
pub fn create_syn_reply(stream_id: u32) -> Vec<u8> {
    let mut data = BytesMut::new();
    
    // Stream ID (32 bits)
    data.put_u32(stream_id);
    
    // Number of header pairs (32 bits) - 0 for empty headers
    data.put_u32(0);
    
    let frame = SpdyFrame::new_control_frame(
        ControlFrameType::SynReply,
        FLAG_FIN,
        stream_id,
        data.to_vec(),
    );
    
    frame.to_bytes()
}

// Create a SETTINGS frame
pub fn create_settings_frame(settings: &[(u32, u32)]) -> Vec<u8> {
    let mut data = BytesMut::new();
    
    // Number of entries (32 bits)
    data.put_u32(settings.len() as u32);
    
    for (id, value) in settings {
        // Settings format: ID (24 bits) | Flags (8 bits) | Value (32 bits)
        data.put_u8((*id >> 16) as u8);
        data.put_u8((*id >> 8) as u8);
        data.put_u8(*id as u8);
        data.put_u8(0); // No flags
        data.put_u32(*value);
    }
    
    let frame = SpdyFrame::new_control_frame(
        ControlFrameType::Settings,
        0,
        0,
        data.to_vec(),
    );
    
    frame.to_bytes()
}

// Parse kubectl's port-forward specific initialization
pub fn parse_kubectl_init_frame(data: &[u8]) -> Option<(u16, u16)> {
    // kubectl sends port information in a specific format
    // Usually: [local_port_high, local_port_low, remote_port_high, remote_port_low]
    if data.len() >= 4 {
        let local_port = u16::from_be_bytes([data[0], data[1]]);
        let remote_port = u16::from_be_bytes([data[2], data[3]]);
        Some((local_port, remote_port))
    } else {
        None
    }
}