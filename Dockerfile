
FROM rust:1.83 as builder

WORKDIR /usr/src/app

COPY . .

RUN cargo build --release


FROM ubuntu:22.04

# Create a non-root user for security (optional but recommended)
WORKDIR /app


RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*


COPY --from=builder /usr/src/app/target/release/your_project_name /usr/local/bin/app

CMD ["app"]