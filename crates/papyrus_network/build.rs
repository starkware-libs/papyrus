use std::io::Result;

fn main() -> Result<()> {
    println!("Building");
    let mut prost_build = prost_build::Config::new();
    prost_build.protoc_arg("--experimental_allow_proto3_optional");
    prost_build.compile_protos(
        &["src/messages/proto/p2p/proto/block.proto", "src/messages/proto_test/util.proto"],
        &["src/messages/proto/", "src/messages/proto_test"],
    )?;
    Ok(())
}
