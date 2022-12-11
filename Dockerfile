FROM rust:1.63

RUN apt update && apt install -y clang vim

COPY . /app/

WORKDIR /app/

RUN cargo build --release --package papyrus_node --bin papyrus_node

RUN mkdir data

EXPOSE 8080 8081

ENTRYPOINT ["/app/target/release/papyrus_node"]
