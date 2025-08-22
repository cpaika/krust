use prost::Message;
use serde_json::Value;
use std::collections::HashMap;

// Include the generated protobuf code
pub mod openapi_v2 {
    include!(concat!(env!("OUT_DIR"), "/openapi.v2.rs"));
}

use openapi_v2::*;

/// Convert our JSON OpenAPI schema to protobuf format
pub fn json_to_protobuf_v2(json: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let doc = Document {
        swagger: json["swagger"].as_str().unwrap_or("2.0").to_string(),
        info: Some(Info {
            title: json["info"]["title"].as_str().unwrap_or("Kubernetes").to_string(),
            version: json["info"]["version"].as_str().unwrap_or("v1.29.0").to_string(),
            description: String::new(),
        }),
        host: String::new(),
        base_path: String::new(),
        schemes: vec!["http".to_string(), "https".to_string()],
        consumes: vec!["application/json".to_string()],
        produces: vec!["application/json".to_string()],
        paths: Some(convert_paths(&json["paths"])),
        definitions: Some(convert_definitions(&json["definitions"])),
    };
    
    // Encode to protobuf
    let mut buf = Vec::new();
    doc.encode(&mut buf)?;
    Ok(buf)
}

fn convert_paths(paths_json: &Value) -> Paths {
    let mut paths = Paths {
        path: Vec::new(),
    };
    
    if let Some(paths_obj) = paths_json.as_object() {
        for (path_name, path_value) in paths_obj {
            let path_item = convert_path_item(path_value);
            paths.path.push(NamedPathItem {
                name: path_name.clone(),
                value: Some(path_item),
            });
        }
    }
    
    paths
}

fn convert_path_item(path_json: &Value) -> PathItem {
    PathItem {
        get: path_json.get("get").and_then(|op| Some(convert_operation(op))),
        put: path_json.get("put").and_then(|op| Some(convert_operation(op))),
        post: path_json.get("post").and_then(|op| Some(convert_operation(op))),
        delete: path_json.get("delete").and_then(|op| Some(convert_operation(op))),
        options: None,
        head: None,
        patch: path_json.get("patch").and_then(|op| Some(convert_operation(op))),
        parameters: Vec::new(),
    }
}

fn convert_operation(op_json: &Value) -> Operation {
    let mut operation = Operation {
        tags: Vec::new(),
        summary: String::new(),
        description: op_json["description"].as_str().unwrap_or("").to_string(),
        operation_id: op_json["operationId"].as_str().unwrap_or("").to_string(),
        consumes: vec!["application/json".to_string()],
        produces: vec!["application/json".to_string()],
        parameters: Vec::new(),
        responses: Some(convert_responses(&op_json["responses"])),
        schemes: Vec::new(),
    };
    
    // Convert parameters
    if let Some(params) = op_json["parameters"].as_array() {
        for (i, param) in params.iter().enumerate() {
            operation.parameters.push(convert_parameter(i, param));
        }
    }
    
    // Convert tags
    if let Some(tags) = op_json["tags"].as_array() {
        for tag in tags {
            if let Some(tag_str) = tag.as_str() {
                operation.tags.push(tag_str.to_string());
            }
        }
    }
    
    // Convert produces/consumes
    if let Some(produces) = op_json["produces"].as_array() {
        operation.produces.clear();
        for produce in produces {
            if let Some(p) = produce.as_str() {
                operation.produces.push(p.to_string());
            }
        }
    }
    
    if let Some(consumes) = op_json["consumes"].as_array() {
        operation.consumes.clear();
        for consume in consumes {
            if let Some(c) = consume.as_str() {
                operation.consumes.push(c.to_string());
            }
        }
    }
    
    operation
}

fn convert_parameter(index: usize, param_json: &Value) -> NamedParameter {
    let name = param_json["name"].as_str().unwrap_or(&format!("param{}", index)).to_string();
    let in_value = param_json["in"].as_str().unwrap_or("query");
    
    let parameter = if in_value == "body" {
        Parameter {
            oneof: Some(parameter::Oneof::BodyParameter(BodyParameter {
                description: param_json["description"].as_str().unwrap_or("").to_string(),
                name: name.clone(),
                r#in: in_value.to_string(),
                required: param_json["required"].as_bool().unwrap_or(false),
                schema: param_json.get("schema").and_then(|s| Some(convert_schema(s))),
            })),
        }
    } else {
        Parameter {
            oneof: Some(parameter::Oneof::NonBodyParameter(NonBodyParameter {
                description: param_json["description"].as_str().unwrap_or("").to_string(),
                name: name.clone(),
                r#in: in_value.to_string(),
                required: param_json["required"].as_bool().unwrap_or(false),
                r#type: param_json["type"].as_str().unwrap_or("string").to_string(),
                format: param_json["format"].as_str().unwrap_or("").to_string(),
            })),
        }
    };
    
    NamedParameter {
        name: name.clone(),
        value: Some(parameter),
    }
}

fn convert_responses(responses_json: &Value) -> Responses {
    let mut responses = Responses {
        response: Vec::new(),
    };
    
    if let Some(resp_obj) = responses_json.as_object() {
        for (status_code, response) in resp_obj {
            responses.response.push(NamedResponse {
                name: status_code.clone(),
                value: Some(Response {
                    description: response["description"].as_str().unwrap_or("").to_string(),
                    schema: response.get("schema").and_then(|s| Some(convert_schema(s))),
                }),
            });
        }
    }
    
    responses
}

fn convert_definitions(defs_json: &Value) -> Definitions {
    let mut definitions = Definitions {
        additional_properties: Vec::new(),
    };
    
    if let Some(defs_obj) = defs_json.as_object() {
        for (def_name, def_value) in defs_obj {
            definitions.additional_properties.push(NamedSchema {
                name: def_name.clone(),
                value: Some(convert_schema(def_value)),
            });
        }
    }
    
    definitions
}

fn convert_schema(schema_json: &Value) -> Schema {
    let mut schema = Schema {
        r#ref: schema_json["$ref"].as_str().unwrap_or("").to_string(),
        r#type: schema_json["type"].as_str().unwrap_or("").to_string(),
        format: schema_json["format"].as_str().unwrap_or("").to_string(),
        title: schema_json["title"].as_str().unwrap_or("").to_string(),
        description: schema_json["description"].as_str().unwrap_or("").to_string(),
        properties: None,
        additional_properties: None,
        required: Vec::new(),
        items: None,
        all_of: Vec::new(),
    };
    
    // Convert properties
    if let Some(props) = schema_json.get("properties") {
        schema.properties = Some(convert_properties(props));
    }
    
    // Convert additionalProperties
    if let Some(add_props) = schema_json.get("additionalProperties") {
        if let Some(bool_val) = add_props.as_bool() {
            schema.additional_properties = Some(Box::new(AdditionalProperties {
                oneof: Some(additional_properties::Oneof::Boolean(bool_val)),
            }));
        } else {
            schema.additional_properties = Some(Box::new(AdditionalProperties {
                oneof: Some(additional_properties::Oneof::Schema(Box::new(convert_schema(add_props)))),
            }));
        }
    }
    
    // Convert required array
    if let Some(required) = schema_json["required"].as_array() {
        for req in required {
            if let Some(req_str) = req.as_str() {
                schema.required.push(req_str.to_string());
            }
        }
    }
    
    // Convert items
    if let Some(items) = schema_json.get("items") {
        schema.items = Some(Items {
            schema: vec![convert_schema(items)],
        });
    }
    
    schema
}

fn convert_properties(props_json: &Value) -> Properties {
    let mut properties = Properties {
        additional_properties: Vec::new(),
    };
    
    if let Some(props_obj) = props_json.as_object() {
        for (prop_name, prop_value) in props_obj {
            properties.additional_properties.push(NamedSchema {
                name: prop_name.clone(),
                value: Some(convert_schema(prop_value)),
            });
        }
    }
    
    properties
}