fn main() {
    prost_build::compile_protos(&["src/common.proto", "src/sync.proto"], &["src"]).unwrap();
}
