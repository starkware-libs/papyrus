# We split the Dockerfile into two stages:
# Stage 1: We copy all the Cargo.toml files and create empty lib.rs files.
# Stage 2:
#   * We copy the files from the first stage
#   * We compile all the crates.
#   * We copy the rest of the files and compile again.
# The reason we compile twice is to allow caching for the first compilation (that compiles all the
# dependency crates) if no Cargo.toml files were changed.
# The reason we split it into two stages is to first copy all the files and then erase all
# non-Cargo.toml files. This way, non-Cargo.toml files won't affect the cache of the second stage
# (For more on docker stages, read https://docs.docker.com/build/building/multi-stage/).
FROM rust:1.63

COPY crates/ /app/crates/
COPY Cargo.toml /app/
RUN find /app \! -name "Cargo.toml" -type f -delete 
RUN find /app -empty -type d -delete 

WORKDIR /app/

# Create empty lib.rs files.
# In order for cargo init to work, we need to not have a Cargo.toml file. In each crate, we rename
# Cargo.toml to another name and after running `cargo init` we override the auto-generated
# Cargo.toml with the original.
RUN mv Cargo.toml _Cargo.toml && for dir in crates/*; do \
    mv $dir/Cargo.toml $dir/_Cargo.toml \
    && cargo init --lib --vcs none $dir \
    && mv -f $dir/_Cargo.toml $dir/Cargo.toml; \
done && mv _Cargo.toml Cargo.toml

# Starting a new stage so that the next two layers will be cached if a non-Cargo.toml file was
# changed.
FROM rust:1.63
WORKDIR /app/

RUN apt update && apt install -y clang

# Copy all the files from the previous stage (which are Cargo.toml and empty lib.rs files).
COPY --from=0 /app .

RUN CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node

# Copy the rest of the files.
COPY crates/ /app/crates
COPY config/ /app/config

# Touching the lib.rs files to mark them for re-compilation.
RUN touch crates/*/src/lib.rs

RUN CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node --bin papyrus_node

WORKDIR /app/
RUN mkdir data

EXPOSE 8080 8081

ENTRYPOINT ["/app/target/release/papyrus_node"]
