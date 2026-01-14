FROM rust:1.91 AS rust
FROM rust AS builder

WORKDIR /usr/src/app

# Build dependencies only (so they are cached)
COPY Cargo.toml Cargo.lock ./
# Dummy main file
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

# Now copy and build the actual source code
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

FROM minizinc/mznc2025:latest AS base

WORKDIR /app

# Fix paths for cargo
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH="${CARGO_HOME}/bin:${PATH}"

# Install system dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    wget \
    default-jre \
    unzip \
    git \
    curl \
    jq \
    flex \
    bison \
    libxml++2.6-dev \
    build-essential \
    libgl1 \
    libglu1-mesa \
    libegl1 \
    libfontconfig1 \
    # Install rustup
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    # Cleanup
    && rm -rf /var/lib/apt/lists/*

FROM base AS huub

RUN git clone --depth 1 --branch pub/CP2025 https://github.com/huub-solver/huub.git /huub
WORKDIR /huub
RUN cargo build --release


FROM base AS yuck

RUN wget https://github.com/informarte/yuck/releases/download/20251106/yuck-20251106.zip \
    && unzip yuck-20251106.zip -d /opt \
    && mv /opt/yuck-20251106 /opt/yuck \
    && chmod +x /opt/yuck/bin/yuck \
    && rm yuck-20251106.zip

FROM base AS or-tools

RUN wget https://github.com/google/or-tools/releases/download/v9.14/or-tools_amd64_ubuntu-24.04_cpp_v9.14.6206.tar.gz -O or-tools.tar.gz \
    && tar -xzvf or-tools.tar.gz \
    && rm or-tools.tar.gz \
    && mv or-tools_x86_64_Ubuntu-24.04_cpp_v9.14.6206 /or-tools \
    && mkdir /opt/or-tools \
    && mv /or-tools/bin /opt/or-tools/bin \
    && mv /or-tools/lib /opt/or-tools/lib \
    && mv /or-tools/share /opt/or-tools/share \
    && jq '.executable = "/opt/or-tools/bin/fzn-cp-sat"' /opt/or-tools/share/minizinc/solvers/cp-sat.msc \
     | jq '.mznlib = "/opt/or-tools/share/minizinc/cp-sat"' > cp-sat.msc.temp \
    && mv cp-sat.msc.temp /opt/or-tools/share/minizinc/solvers/cp-sat.msc

FROM base AS choco

RUN wget https://github.com/chocoteam/choco-solver/archive/refs/tags/v4.10.18.tar.gz -O choco.tar.gz \
    && wget https://github.com/chocoteam/choco-solver/releases/download/v4.10.18/choco-solver-4.10.18-light.jar -O choco.jar \
    && tar -xzvf choco.tar.gz \
    && rm choco.tar.gz \
    && mv choco-solver-4.10.18 /choco \
    && mkdir -p /opt/choco/bin \
    && mv choco.jar /opt/choco/bin \
    && mv /choco/parsers/src/main/minizinc/fzn-choco.py /opt/choco/bin \
    && mv /choco/parsers/src/main/minizinc/fzn-choco.sh /opt/choco/bin \
    && mkdir -p /opt/choco/share/minizinc/solvers \
    && mv /choco/parsers/src/main/minizinc/mzn_lib /opt/choco/share/minizinc/choco_lib \
    && jq '.executable = "/opt/choco/bin/fzn-choco.sh"' /choco/parsers/src/main/minizinc/choco.msc \
     | jq '.mznlib = "/opt/choco/share/minizinc/choco_lib"' > /opt/choco/share/minizinc/solvers/choco.msc \
    && sed -i 's&JAR_FILE=.*&JAR_FILE="/opt/choco/bin/choco.jar"&g' /opt/choco/bin/fzn-choco.py \
    && rm -rf /choco

FROM base AS pumpkin

# Version 0.2.2
RUN wget https://github.com/ConSol-Lab/Pumpkin/archive/62b2f09f4b28d0065e4a274d7346f34598b44898.tar.gz -O pumpkin.tar.gz \
    && tar -xzvf pumpkin.tar.gz \
    && rm pumpkin.tar.gz \
    && mv Pumpkin-62b2f09f4b28d0065e4a274d7346f34598b44898 /pumpkin
WORKDIR /pumpkin
RUN cargo build --release -p pumpkin-solver
# We can't use the .msc file from the repository because it is currently not valid JSON
COPY ./minizinc/solvers/pumpkin.msc.template /pumpkin.msc.template
RUN mkdir -p /opt/pumpkin/bin \
    && mv /pumpkin/target/release/pumpkin-solver /opt/pumpkin/bin \
    && mkdir -p /opt/pumpkin/share/minizinc/solvers \
    && mv /pumpkin/minizinc/lib /opt/pumpkin/share/minizinc/pumpkin_lib \
    && jq '.executable = "/opt/pumpkin/bin/pumpkin-solver"' /pumpkin.msc.template \
     | jq '.mznlib = "/opt/pumpkin/share/minizinc/pumpkin_lib"' > /opt/pumpkin/share/minizinc/solvers/pumpkin.msc \
    && rm -rf /pumpkin

FROM base AS minizinc-source

WORKDIR /source
ENV MINIZINC_SOURCE_VERSION=2.9.4
RUN wget -qO minizinc.tgz https://github.com/MiniZinc/MiniZincIDE/releases/download/${MINIZINC_SOURCE_VERSION}/MiniZincIDE-${MINIZINC_SOURCE_VERSION}-bundle-linux-x86_64.tgz \
    && tar xf minizinc.tgz --strip-components=1 \
    && rm minizinc.tgz \
    && rm bin/minizinc bin/mzn2doc bin/MiniZincIDE

FROM minizinc-source AS gecode

WORKDIR /opt/gecode
RUN mkdir bin \
    && mkdir -p share/minizinc/solvers \
    && mv /source/bin/fzn-gecode bin/ \
    && mv /source/lib lib/ \
    && mv /source/share/minizinc/gecode/ share/minizinc/gecode_lib/ \
    && jq '.executable = "/opt/gecode/bin/fzn-gecode"' /source/share/minizinc/solvers/gecode.msc \
     | jq '.mznlib = "/opt/gecode/share/minizinc/gecode_lib"' > share/minizinc/solvers/gecode.msc

FROM minizinc-source AS chuffed

WORKDIR /opt/chuffed
COPY ./minizinc/solvers/chuffed.msc.template .
RUN mkdir bin \
    && mkdir -p share/minizinc/solvers \
    && mv /source/bin/fzn-chuffed bin/ \
    && mv /source/share/minizinc/chuffed/ share/minizinc/chuffed_lib/ \
    && jq '.executable = "/opt/chuffed/bin/fzn-chuffed"' chuffed.msc.template \
     | jq '.mznlib = "/opt/chuffed/share/minizinc/chuffed_lib"' > share/minizinc/solvers/chuffed.msc

FROM base AS scip

WORKDIR /opt/scip
RUN wget -qO package.deb https://www.scipopt.org/download/release/SCIPOptSuite-9.2.4-Linux-ubuntu24.deb

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
COPY --from=or-tools /opt/or-tools/share/minizinc/solvers/* .
COPY --from=choco /opt/choco/share/minizinc/solvers/* .
COPY --from=pumpkin /opt/pumpkin/share/minizinc/solvers/* .
COPY --from=gecode /opt/gecode/share/minizinc/solvers/* .
COPY --from=chuffed /opt/chuffed/share/minizinc/solvers/* .

FROM base AS final

# Install mzn2feat
# TODO: Move it into its own image (to improve caching)
RUN git clone https://github.com/CP-Unibo/mzn2feat.git /opt/mzn2feat

RUN cd /opt/mzn2feat && bash install --no-xcsp

RUN ln -s /opt/mzn2feat/bin/mzn2feat /usr/local/bin/mzn2feat \
    && ln -s /opt/mzn2feat/bin/fzn2feat /usr/local/bin/fzn2feat

# Install Picat solver
# TODO: Move it into its own image (to improve caching)
RUN wget https://picat-lang.org/download/picat394_linux64.tar.gz \
    && tar -xzf picat394_linux64.tar.gz -C /opt \
    && ln -s /opt/Picat/picat /usr/local/bin/picat \
    && rm picat394_linux64.tar.gz

RUN git clone https://github.com/nfzhou/fzn_picat.git /opt/fzn_picat

# Install SCIP from a .deb package. This requires updating apt-get lists
COPY --from=scip /opt/scip/package.deb ./scip-package.deb
RUN apt-get update && apt-get install -y --no-install-recommends \
    software-properties-common \
    && add-apt-repository universe \
    && apt-get install -y ./scip-package.deb \
    && rm ./scip-package.deb \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# Install solver configurations
COPY --from=solver-configs /solvers/*.msc /usr/local/share/minizinc/solvers/

# Copy solver files
COPY ./solvers/picat/wrapper.sh /usr/local/bin/fzn-picat

COPY --from=huub /huub/target/release/fzn-huub /usr/local/bin/fzn-huub
COPY --from=huub /huub/share/minizinc/huub/ /usr/local/share/minizinc/huub/

COPY --from=yuck /opt/yuck/ /opt/yuck/
COPY --from=or-tools /opt/or-tools/ /opt/or-tools/
COPY --from=choco /opt/choco/ /opt/choco/
COPY --from=pumpkin /opt/pumpkin/ /opt/pumpkin/
COPY --from=gecode /opt/gecode/ /opt/gecode/
COPY --from=chuffed /opt/chuffed/ /opt/chuffed/

# Set our solver as the default
RUN echo '{"tagDefaults": [["", "org.psp.sunny"]]}' > $HOME/.minizinc/Preferences.json

COPY --from=builder /usr/src/app/target/release/portfolio-solver-framework /usr/local/bin/portfolio-solver-framework

# Gecode also uses dynamically linked libraries, so register these with the system
# Note that Chuffed may be dependent on these same linked libraries, but I'm not sure
# This is done at the very end to make sure it doesn't mess with other commands
RUN echo "/opt/gecode/lib" > /etc/ld.so.conf.d/gecode.conf \
    && ldconfig

FROM builder AS ci

FROM final AS ci-integration

COPY Cargo.toml Cargo.lock ./
COPY ./src ./src
COPY ./tests ./tests

# Make the 'final' image the default image
FROM final

