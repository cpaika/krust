use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["openapi_v2.proto"], &["."])?;
    Ok(())
}