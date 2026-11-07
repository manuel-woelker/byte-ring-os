FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --yes --no-install-recommends qemu-system-x86 seabios \
    && rm -rf /var/lib/apt/lists/*
