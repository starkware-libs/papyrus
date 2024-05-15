use std::env;
use std::io::{Error, ErrorKind, Result};
use std::process::Command;

fn main() -> Result<()> {
    println!("Building");
    let protoc = env::var("PROTOC").unwrap_or("protoc".to_string());

    let protoc_version = String::from_utf8_lossy(
        &Command::new(protoc)
            .arg("--version")
            .output()
            .expect("Failed to get protoc version, check if it is installed.")
            .stdout,
    )
    .to_string();

    let parts: Vec<&str> = protoc_version.split_whitespace().collect();
    assert!(parts.len() >= 2, "Failed to determine protoc version");
    let mut version_parts = parts[1].split('.').map(|part| part.parse::<u32>().unwrap_or_default());

    match (version_parts.next(), version_parts.next()) {
        (Some(major), Some(minor)) => {
            if major < 3 || (major == 3 && minor < 15) {
                Err(Error::new(
                    ErrorKind::Other,
                    "protoc version is too old. version 3.15.x or greater is needed.",
                ))
            } else {
                prost_build::compile_protos(
                    &[
                        "src/protobuf_messages/proto/p2p/proto/header.proto",
                        "src/protobuf_messages/proto/p2p/proto/state.proto",
                    ],
                    &["src/protobuf_messages/proto/"],
                )?;
                Ok(())
            }
        }
        _ => Err(Error::new(ErrorKind::Other, "Error parsing protoc version")),
    }
}
