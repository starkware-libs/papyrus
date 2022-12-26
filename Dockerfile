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
FROM rust:1.63 AS copy_toml

COPY crates/ /app/crates/
COPY Cargo.toml /app/

WORKDIR /app/

# Erase all non-Cargo.toml files.
RUN find /app \! -name "Cargo.toml" -type f -delete ; \
    find /app -empty -type d -delete; \
# Create empty lib.rs files.
# In order for cargo init to work, we need to not have a Cargo.toml file. In each crate, we rename
# Cargo.toml to another name and after running `cargo init` we override the auto-generated
# Cargo.toml with the original.
    mv Cargo.toml _Cargo.toml && for dir in crates/*; do \
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
COPY --from=copy_toml /app .

RUN CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node

# Copy the rest of the files.
COPY crates/ /app/crates

# Touching the lib.rs files to mark them for re-compilation. Then re-compile now that all the source
# code is available
RUN touch crates/*/src/lib.rs; \
    CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node --bin papyrus_node

# Starting a new stage so that the final image will contain only the executable.
FROM rust:1.63
WORKDIR /app
COPY --from=builder /app/target/release/papyrus_node /app/target/release/papyrus_node
COPY config/ /app/config

RUN mkdir data

RUN ls -R /app

EXPOSE 8080 8081

ENTRYPOINT ["/app/target/release/papyrus_node"]
