use std::env;
use std::io::{Error, ErrorKind, Result};
use std::process::Command;

fn main() -> Result<()> {
    println!("Building");
    let protoc = env::var("PROTOC").unwrap_or("protoc".to_string());

    let protoc_version = String::from_utf8_lossy(
        &Command::new(protoc).arg("--version").output().expect("Protoc is not installed.").stdout,
    )
    .to_string();

    let parts: Vec<&str> = protoc_version.split_whitespace().collect();
    let protoc_version_str = parts.get(1).expect("Failed to determine protoc version");
    let mut protoc_version_parts = protoc_version_str
        .split('.')
        .map(|part| part.parse::<u32>().expect("Error parsing protoc version"));
    let major = protoc_version_parts.next().expect("Protoc version did not have a major number");
    let minor = protoc_version_parts.next().unwrap_or_default();

    if major < 3 || (major == 3 && minor < 15) {
        Err(Error::new(
            ErrorKind::Other,
            "protoc version is too old. version 3.15.x or greater is needed.",
        ))
    } else {
        prost_build::compile_protos(
            &[
                "src/proto/p2p/proto/class.proto",
                "src/proto/p2p/proto/event.proto",
                "src/proto/p2p/proto/header.proto",
                "src/proto/p2p/proto/state.proto",
                "src/proto/p2p/proto/transaction.proto",
                "src/proto/p2p/proto/consensus.proto",
            ],
            &["src/proto/"],
        )?;
        Ok(())
    }
}
