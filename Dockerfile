FROM rust:1.91 AS builder

WORKDIR /usr/src/app

COPY . .

RUN cargo build --release


FROM debian:trixie-slim

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    wget \
    git \
    flex \
    bison \
    libxml++2.6-dev \
    build-essential \
    libgl1 \
    libglu1-mesa \
    libegl1 \
    && rm -rf /var/lib/apt/lists/*

# Install MiniZinc
RUN wget https://github.com/MiniZinc/MiniZincIDE/releases/download/2.8.7/MiniZincIDE-2.8.7-bundle-linux-x86_64.tgz \
    && tar -xzf MiniZincIDE-2.8.7-bundle-linux-x86_64.tgz -C /opt \
    && mv /opt/MiniZincIDE-2.8.7-bundle-linux-x86_64 /opt/minizinc \
    && ln -s /opt/minizinc/bin/minizinc /usr/local/bin/minizinc \
    && rm MiniZincIDE-2.8.7-bundle-linux-x86_64.tgz

# Install mzn2feat
RUN git clone https://github.com/CP-Unibo/mzn2feat.git /opt/mzn2feat

RUN cd /opt/mzn2feat && bash install --no-xcsp

RUN ln -s /opt/mzn2feat/bin/mzn2feat /usr/local/bin/mzn2feat \
    && ln -s /opt/mzn2feat/bin/fzn2feat /usr/local/bin/fzn2feat

COPY --from=builder /usr/src/app/target/release/portfolio-solver-framework /usr/local/bin/portfolio-solver-framework



ENTRYPOINT ["portfolio-solver-framework"]