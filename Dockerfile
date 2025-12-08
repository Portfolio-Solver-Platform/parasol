FROM ubuntu:24.04 AS nix

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Nix
RUN curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux --init none --no-confirm
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"

COPY ./flake.nix ./flake.lock ./
COPY ./nix/ ./nix/
COPY ./rust-toolchain.toml ./

RUN nix develop

FROM nix AS builder

# Copy dependency manifests first (changes rarely)
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer is cached unless Cargo.toml changes)
RUN nix develop -c cargo build --release

# Remove dummy artifacts
RUN rm -rf src

# Now copy actual source code (changes frequently)
COPY src ./src

# Build code (dependencies are cached)
RUN nix develop -c cargo build --release


FROM nix

# Install system dependencies
# RUN apt-get update && apt-get install -y \
#     curl \
#     xz-utils \
#     ca-certificates \
#     libssl-dev \
#     wget \
#     git \
#     build-essential \
#     libgl1 \
#     libglu1-mesa \
#     libegl1 \
#     libfontconfig1 \
#     && rm -rf /var/lib/apt/lists/*


# Insert Picat MiniZinc configuration
RUN mkdir -p /opt/minizinc/share/minizinc/solvers/
RUN echo '{"id": "org.picat-lang.picat", "name": "Picat", "version": "3.9.4", "executable": "/usr/local/bin/picat", "mznlib": "", "tags": ["cp", "int"], "supportsMzn": false, "supportsFzn": true, "needsSolns2Out": true, "needsMznExecutable": false, "isGUIApplication": false}' > /opt/minizinc/share/minizinc/solvers/picat.msc

# # Install Yuck solver (requires Java)
# RUN apt-get update && apt-get install -y unzip default-jre \
#     && wget https://github.com/informarte/yuck/releases/download/20251106/yuck-20251106.zip \
#     && unzip yuck-20251106.zip -d /opt \
#     && mv /opt/yuck-20251106 /opt/yuck \
#     && chmod +x /opt/yuck/bin/yuck \
#     && cp /opt/yuck/mzn/yuck.msc /opt/minizinc/share/minizinc/solvers/ \
#     && sed -i 's|"executable": "../bin/yuck"|"executable": "/opt/yuck/bin/yuck"|' /opt/minizinc/share/minizinc/solvers/yuck.msc \
#     && sed -i 's|"mznlib": "lib"|"mznlib": "/opt/yuck/mzn/lib"|' /opt/minizinc/share/minizinc/solvers/yuck.msc \
#     && rm yuck-20251106.zip \
#     && apt-get remove -y unzip && apt-get autoremove -y \
#     && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/portfolio-solver-framework /usr/local/bin/portfolio-solver-framework

# Create a small startup script which has the `nix develop` environment baked into it.
#   This is done to avoid running `nix develop -c` in the entrypoint.
#   Avoiding this is important because `nix develop` evaluates the
#   flake derivation every time it runs which is slow.
# RUN echo '#!/bin/bash' > /entrypoint.sh && \
#     nix print-dev-env >> /entrypoint.sh && \
#     echo 'exec "$@"' >> /entrypoint.sh && \
#     chmod +x /entrypoint.sh

ENTRYPOINT [ "portfolio-solver-framework" ]
