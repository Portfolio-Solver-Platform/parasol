FROM rust:1.91 AS rust
FROM rust AS builder

WORKDIR /usr/src/app

# Build dependencies only (so they are cached)
COPY Cargo.toml Cargo.lock ./
# Dummy main file
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Now copy and build the actual source code
COPY src ./src
RUN touch src/main.rs && cargo build --release


FROM minizinc/mznc2025:latest AS base

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    wget \
    default-jre \
    unzip \
    git \
    jq \
    flex \
    bison \
    libxml++2.6-dev \
    build-essential \
    libgl1 \
    libglu1-mesa \
    libegl1 \
    libfontconfig1 \
    && rm -rf /var/lib/apt/lists/*


FROM rust AS huub

RUN git clone --branch pub/CP2025 https://github.com/huub-solver/huub.git /huub
WORKDIR /huub
RUN cargo build --release


FROM base AS yuck

RUN wget https://github.com/informarte/yuck/releases/download/20251106/yuck-20251106.zip \
    && unzip yuck-20251106.zip -d /opt \
    && mv /opt/yuck-20251106 /opt/yuck \
    && chmod +x /opt/yuck/bin/yuck \
    && rm yuck-20251106.zip \
    && apt-get remove -y unzip


FROM base AS solver-configs

COPY ./minizinc/solvers/ /solvers/
WORKDIR /solvers
RUN jq '.executable = "/usr/local/bin/portfolio-solver-framework"' ./framework.msc.template > ./framework.msc
RUN jq '.executable = "/usr/local/bin/fzn-picat"' ./picat.msc.template > picat.msc.temp
RUN jq '.mznlib = "/opt/fzn_picat/mznlib"' picat.msc.temp > ./picat.msc
COPY --from=huub /huub/share/minizinc/solvers/huub.msc ./huub.msc.template
RUN jq '.executable = "/usr/local/bin/fzn-huub"' ./huub.msc.template > huub.msc.temp
RUN jq '.mznlib = "/usr/local/share/minizinc/huub/"' ./huub.msc.temp > ./huub.msc
COPY --from=yuck /opt/yuck/mzn/yuck.msc ./yuck.msc.template
RUN jq '.executable = "/opt/yuck/bin/yuck"' ./yuck.msc.template > yuck.msc.temp
RUN jq '.mznlib = "/opt/yuck/mzn/lib/"' ./yuck.msc.temp > ./yuck.msc
# Gecode should only be used for compilation, not actually run, so don't correct its executable path
RUN cp ./gecode.msc.template ./gecode.msc


FROM base

# Install mzn2feat
# TODO: Move it into its own image (to improve caching)
RUN git clone https://github.com/CP-Unibo/mzn2feat.git /opt/mzn2feat

RUN cd /opt/mzn2feat && bash install --no-xcsp

RUN ln -s /opt/mzn2feat/bin/mzn2feat /usr/local/bin/mzn2feat \
    && ln -s /opt/mzn2feat/bin/fzn2feat /usr/local/bin/fzn2feat

# Install Picat solver
# TODO: Move it into its own image (to improve caching)
RUN wget http://picat-lang.org/download/picat394_linux64.tar.gz \
    && tar -xzf picat394_linux64.tar.gz -C /opt \
    && ln -s /opt/Picat/picat /usr/local/bin/picat \
    && rm picat394_linux64.tar.gz

RUN git clone https://github.com/nfzhou/fzn_picat.git /opt/fzn_picat

# Install solver configurations
COPY --from=solver-configs /solvers/*.msc /usr/local/share/minizinc/solvers/

# Copy solver files
COPY ./solvers/picat/wrapper.sh /usr/local/bin/fzn-picat

COPY --from=huub /huub/target/release/fzn-huub /usr/local/bin/fzn-huub
COPY --from=huub /huub/share/minizinc/huub/ /usr/local/share/minizinc/huub/

COPY --from=yuck /opt/yuck/ /opt/yuck/

# Set our solver as the default
RUN echo '{"tagDefaults": [["", "org.psp.sunny"]]}' > $HOME/.minizinc/Preferences.json

COPY --from=builder /usr/src/app/target/release/portfolio-solver-framework /usr/local/bin/portfolio-solver-framework

