FROM rust:1.63

RUN apt update && apt install -y clang

# TODO(shahak): Copy first only .toml files, then build, then copy the rest and rebuild.
COPY . /app/

WORKDIR /app/
RUN cargo build --release --package papyrus_node --bin papyrus_node

ENTRYPOINT ["/app/target/release/papyrus_node"]
