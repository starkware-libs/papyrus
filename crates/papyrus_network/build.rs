use std::io::Result;

fn main() -> Result<()> {
    println!("Building");
    prost_build::compile_protos(
        &["src/messages/proto/p2p/proto/block.proto"],
        &["src/messages/proto/"],
    )?;
    Ok(())
}
