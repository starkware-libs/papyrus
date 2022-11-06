FROM rust:1.63

# Copy all Cargo.toml files
COPY . /app/
RUN find /app \! -name "Cargo.toml" -type f -delete 
RUN find /app -empty -type d -delete 

FROM rust:1.63
WORKDIR /app/

RUN apt update && apt install -y clang

COPY --from=0 /app .

# Create empty lib.rs files.
# In order for cargo init to work, we need to not have a Cargo.toml file. We'll rename Cargo.toml
# to another name and after running `cargo init` we'll override the auto-generated Cargo.toml with
# the original.
RUN mv Cargo.toml _Cargo.toml
RUN for dir in crates/*; do \
    mv $dir/Cargo.toml $dir/_Cargo.toml \
    && cargo init --lib --vcs none $dir \
    && mv -f $dir/_Cargo.toml $dir/Cargo.toml; \
done
RUN mv _Cargo.toml Cargo.toml

RUN CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node

COPY . .

RUN touch crates/*/src/lib.rs

RUN CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node --bin papyrus_node

ENTRYPOINT ["/app/target/release/papyrus_node"]
