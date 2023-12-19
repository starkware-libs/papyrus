use std::env;
use std::io::Result;

fn main() -> Result<()> {
    println!("Building");
    let dir = env::current_dir().unwrap();
    println!("{dir:?}");
    env::set_var("PROTOC", "./protoc");
    prost_build::compile_protos(
        &["src/messages/proto/p2p/proto/block.proto"],
        &["src/messages/proto/"],
    )?;
    Ok(())
}
