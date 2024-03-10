use std::io::Result;

fn main() -> Result<()> {
    println!("Building");
    prost_build::compile_protos(
        &["src/protobuf_messages/proto/p2p/proto/header.proto"],
        &["src/protobuf_messages/proto/"],
    )?;
    Ok(())
}
