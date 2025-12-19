FROM rust:1.91 AS builder

WORKDIR /usr/src/app

# Copy dependency manifests first (changes rarely)
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer is cached unless Cargo.toml changes)
RUN cargo build --release

# Remove dummy artifacts
RUN rm -rf src

# Now copy actual source code (changes frequently)
COPY src ./src

# Build only your code (dependencies are cached!)
RUN touch src/main.rs && cargo build --release


FROM minizinc/mznc2025:latest

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
    libfontconfig1 \
    && rm -rf /var/lib/apt/lists/*

# Install mzn2feat
RUN git clone https://github.com/CP-Unibo/mzn2feat.git /opt/mzn2feat

RUN cd /opt/mzn2feat && bash install --no-xcsp

RUN ln -s /opt/mzn2feat/bin/mzn2feat /usr/local/bin/mzn2feat \
    && ln -s /opt/mzn2feat/bin/fzn2feat /usr/local/bin/fzn2feat

RUN mkdir -p /opt/minizinc/share/minizinc/solvers/

# Install Picat solver
RUN wget http://picat-lang.org/download/picat394_linux64.tar.gz \
    && tar -xzf picat394_linux64.tar.gz -C /opt \
    && ln -s /opt/Picat/picat /usr/local/bin/picat \
    && rm picat394_linux64.tar.gz

# Download Picat MiniZinc configuration
RUN echo '{"id": "org.picat-lang.picat", "name": "Picat", "version": "3.9.4", "executable": "/usr/local/bin/picat", "mznlib": "", "tags": ["cp", "int"], "supportsMzn": false, "supportsFzn": true, "needsSolns2Out": true, "needsMznExecutable": false, "isGUIApplication": false}' > /opt/minizinc/share/minizinc/solvers/picat.msc

# Install Yuck solver (requires Java)
RUN apt-get update && apt-get install -y unzip default-jre \
    && wget https://github.com/informarte/yuck/releases/download/20251106/yuck-20251106.zip \
    && unzip yuck-20251106.zip -d /opt \
    && mv /opt/yuck-20251106 /opt/yuck \
    && chmod +x /opt/yuck/bin/yuck \
    && cp /opt/yuck/mzn/yuck.msc /opt/minizinc/share/minizinc/solvers/ \
    && sed -i 's|"executable": "../bin/yuck"|"executable": "/opt/yuck/bin/yuck"|' /opt/minizinc/share/minizinc/solvers/yuck.msc \
    && sed -i 's|"mznlib": "lib"|"mznlib": "/opt/yuck/mzn/lib"|' /opt/minizinc/share/minizinc/solvers/yuck.msc \
    && rm yuck-20251106.zip \
    && apt-get remove -y unzip && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/*

COPY ./solver.msc.template .
RUN sed 's|\${EXECUTABLE_PATH}|/usr/local/bin/portfolio-solver-framework|g' ./solver.msc.template > /usr/local/share/minizinc/solvers/portfolio-solver-framework.msc

# Set our solver as the default
RUN echo '{"tagDefaults": [["", "org.psp.sunny"]]}' > $HOME/.minizinc/Preferences.json

COPY --from=builder /usr/src/app/target/release/portfolio-solver-framework /usr/local/bin/portfolio-solver-framework



ENTRYPOINT ["portfolio-solver-framework"]
