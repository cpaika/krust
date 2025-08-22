use prost::Message;
use serde_json::Value;
use std::collections::HashMap;

// This is a simplified protobuf encoding for OpenAPI v2 spec
// The actual Kubernetes format is complex, but kubectl can also handle
// a simplified version as long as the basic structure is correct

#[derive(Clone, PartialEq, Message)]
pub struct OpenAPIV2Document {
    #[prost(string, tag = "1")]
    pub swagger: String,
    
    #[prost(message, optional, tag = "2")]
    pub info: Option<Info>,
    
    #[prost(string, tag = "3")]
    pub host: String,
    
    #[prost(string, tag = "4")]
    pub base_path: String,
    
    #[prost(string, repeated, tag = "5")]
    pub schemes: Vec<String>,
    
    #[prost(string, repeated, tag = "6")]
    pub consumes: Vec<String>,
    
    #[prost(string, repeated, tag = "7")]
    pub produces: Vec<String>,
    
    #[prost(bytes, tag = "8")]
    pub paths: Vec<u8>,  // Simplified: store paths as JSON bytes
    
    #[prost(bytes, tag = "9")]
    pub definitions: Vec<u8>,  // Simplified: store definitions as JSON bytes
}

#[derive(Clone, PartialEq, Message)]
pub struct Info {
    #[prost(string, tag = "1")]
    pub title: String,
    
    #[prost(string, tag = "2")]
    pub version: String,
}

pub fn json_to_protobuf(json: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let doc = OpenAPIV2Document {
        swagger: json["swagger"].as_str().unwrap_or("2.0").to_string(),
        info: Some(Info {
            title: json["info"]["title"].as_str().unwrap_or("Kubernetes").to_string(),
            version: json["info"]["version"].as_str().unwrap_or("v1.29.0").to_string(),
        }),
        host: "".to_string(),
        base_path: "".to_string(),
        schemes: vec!["http".to_string()],
        consumes: vec!["application/json".to_string()],
        produces: vec!["application/json".to_string()],
        paths: serde_json::to_vec(&json["paths"])?,
        definitions: serde_json::to_vec(&json["definitions"])?,
    };
    
    let mut buf = Vec::new();
    doc.encode(&mut buf)?;
    Ok(buf)
}