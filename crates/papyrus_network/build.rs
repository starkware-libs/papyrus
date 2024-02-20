use std::io::Result;

fn main() -> Result<()> {
    println!("Building");
    prost_build::compile_protos(
        &["src/messages/proto/p2p/proto/header.proto", "src/messages/proto_test/util.proto"],
        &["src/messages/proto/", "src/messages/proto_test"],
    )?;
    Ok(())
}
