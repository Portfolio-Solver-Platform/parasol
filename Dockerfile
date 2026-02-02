FROM rust:1.93 AS rust
FROM rust AS builder

# The number of make jobs used when `make` is called
ARG MAKE_JOBS=2

WORKDIR /usr/src/app

# Build dependencies only (so they are cached)
COPY Cargo.toml Cargo.lock ./
# Dummy main file
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked --quiet \
    && rm -rf src

# Now copy and build the actual source code
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked --quiet

FROM minizinc/mznc2025:latest AS base-small

WORKDIR /app

RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    ca-certificates \
    # Java is needed at runtime by Yuck
    default-jre \
    # Dynamic library needed by Gecode
    libegl1 \
    # Cleanup
    && apt-get clean -qq \
    && rm -rf /var/lib/apt/lists/*

FROM base-small AS base

# Fix paths for cargo
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH="${CARGO_HOME}/bin:${PATH}"

# Install system dependencies
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    software-properties-common \
    curl \
    wget \
    unzip \
    git \
    jq \
    cmake \
    flex \
    bison \
    build-essential \
    # Install rustup
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.91 \
    # Cleanup
    && apt-get clean -qq \
    && rm -rf /var/lib/apt/lists/*

FROM base AS huub

RUN git clone -q --depth 1 --branch pub/CP2025 https://github.com/huub-solver/huub.git /huub
WORKDIR /huub
RUN cargo build --release --quiet


FROM base AS yuck

ARG YUCK_SHA256=2c562fe76f7b25289dacf90a7688b8cdd2f7c7029676e1d32727f795ac653615
RUN wget -q https://github.com/informarte/yuck/releases/download/20251106/yuck-20251106.zip \
    && echo "${YUCK_SHA256}  yuck-20251106.zip" | sha256sum -c - \
    && unzip -q yuck-20251106.zip -d /opt \
    && mv /opt/yuck-20251106 /opt/yuck \
    && chmod +x /opt/yuck/bin/yuck \
    && rm yuck-20251106.zip

FROM base AS or-tools

WORKDIR /or-tools
ARG OR_TOOLS_SHA256=6f389320672cee00b78aacefb2bde33fef0bb988c3b2735573b9fffd1047fbda
RUN wget -q https://github.com/google/or-tools/releases/download/v9.15/or-tools_amd64_ubuntu-24.04_cpp_v9.15.6755.tar.gz -O or-tools.tar.gz \
    && echo "${OR_TOOLS_SHA256}  or-tools.tar.gz" | sha256sum -c - \
    && tar -xzf or-tools.tar.gz --strip-components=1 \
    && rm or-tools.tar.gz \
    && mkdir /opt/or-tools \
    && mv /or-tools/bin /opt/or-tools/bin \
    && mv /or-tools/lib /opt/or-tools/lib \
    && mv /or-tools/share /opt/or-tools/share \
    && jq '.executable = "/opt/or-tools/bin/fzn-cp-sat"' /opt/or-tools/share/minizinc/solvers/cp-sat.msc \
     | jq '.mznlib = "/opt/or-tools/share/minizinc/cp-sat"' > cp-sat.msc.temp \
    && mv cp-sat.msc.temp /opt/or-tools/share/minizinc/solvers/cp-sat.msc

FROM base AS choco

ARG CHOCO_SRC_SHA256=9a6d8c465cc73752c085281f49c45793135d8545e57bc3f4effd15bde6d03de5
ARG CHOCO_JAR_SHA256=767a8bdf872c3b9d2a3465bb37822e1f0a60904a54f0181dbf7c6a106415abdf
RUN wget -q https://github.com/chocoteam/choco-solver/archive/refs/tags/v4.10.18.tar.gz -O choco.tar.gz \
    && echo "${CHOCO_SRC_SHA256}  choco.tar.gz" | sha256sum -c - \
    && wget -q https://github.com/chocoteam/choco-solver/releases/download/v4.10.18/choco-solver-4.10.18-light.jar -O choco.jar \
    && echo "${CHOCO_JAR_SHA256}  choco.jar" | sha256sum -c - \
    && tar -xzf choco.tar.gz \
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
ARG PUMPKIN_SHA256=0abf3495945e31c7ebf731bcdc00bae978b6da0b59ff3a8830a0c9335e672ca3
RUN wget -q https://github.com/ConSol-Lab/Pumpkin/archive/62b2f09f4b28d0065e4a274d7346f34598b44898.tar.gz -O pumpkin.tar.gz \
    && echo "${PUMPKIN_SHA256}  pumpkin.tar.gz" | sha256sum -c - \
    && tar -xzf pumpkin.tar.gz \
    && rm pumpkin.tar.gz \
    && mv Pumpkin-62b2f09f4b28d0065e4a274d7346f34598b44898 /pumpkin
WORKDIR /pumpkin
RUN cargo build --release --quiet -p pumpkin-solver
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
ENV MINIZINC_SOURCE_VERSION=2.9.5
ARG MINIZINC_SHA256=591f70e49e49ddead9a4c091ad2450f972abde8de3acdff544d3bed50b279e44
RUN wget -qO minizinc.tgz https://github.com/MiniZinc/MiniZincIDE/releases/download/${MINIZINC_SOURCE_VERSION}/MiniZincIDE-${MINIZINC_SOURCE_VERSION}-bundle-linux-x86_64.tgz \
    && echo "${MINIZINC_SHA256}  minizinc.tgz" | sha256sum -c - \
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
ARG SCIP_SHA256=de64b7c9f109d0e83dc7d7a7e8e6eb2036254c536100a5866a58e5898b3b36b4
RUN wget -qO package.deb https://www.scipopt.org/download/release/SCIPOptSuite-9.2.4-Linux-ubuntu24.deb \
    && echo "${SCIP_SHA256}  package.deb" | sha256sum -c -

FROM base AS dexter

WORKDIR /source
ARG DEXTER_SHA256=583a5ef689e189a568bd4e691096156fdc1974a0beb9721703f02ba61515b75f
RUN wget -qO source.tar.gz https://github.com/ddxter/gecode-dexter/archive/b46a6f557977c7b1863dc6b5885b69ebf9edcc14.tar.gz \
    && echo "${DEXTER_SHA256}  source.tar.gz" | sha256sum -c - \
    && tar xf source.tar.gz --strip-components=1 \
    && rm source.tar.gz \
    && cmake . \
    && make -j${MAKE_JOBS}

WORKDIR /opt/dexter
RUN mkdir bin \
    && mkdir -p share/minizinc/solvers \
    && mv /source/bin/fzn-gecode bin/fzn-dexter \
    && mv /source/gecode/ share/minizinc/dexter_lib/ \
    && jq '.executable = "/opt/dexter/bin/fzn-dexter"' /source/tools/flatzinc/gecode.msc.in \
     | jq '.mznlib = "/opt/dexter/share/minizinc/dexter_lib"' > share/minizinc/solvers/dexter.msc

FROM base AS solver-configs

COPY ./minizinc/solvers/ /solvers/
WORKDIR /solvers
RUN jq '.executable[0] = "/usr/local/bin/parasol"' ./parasol.msc.template > ./parasol.msc
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
COPY --from=dexter /opt/dexter/share/minizinc/solvers/* .

FROM base AS mzn2feat

WORKDIR /opt/mzn2feat

ARG MZN2FEAT_COMMIT=3f92db18a88ba73403238e0ca6be4e9367f4773d
ARG MZN2FEAT_SHA256=c5a07a8d4e3d266735302220268bb6e41f136a68e8c3d0bc5c6ee9ec02c8ec2b
RUN wget -qO source.tar.gz https://github.com/CP-Unibo/mzn2feat/archive/${MZN2FEAT_COMMIT}.tar.gz \
    && echo "${MZN2FEAT_SHA256}  source.tar.gz" | sha256sum -c - \
    && tar -xzf source.tar.gz --strip-components=1 \
    && rm source.tar.gz
RUN bash install --no-xcsp

FROM base AS picat

ARG PICAT_SHA256=938f994ab94c95d308a1abcade0ea04229171304ae2a64ddcea56a49cdd4faa0
RUN wget -qO picat.tar.gz https://picat-lang.org/download/picat394_linux64.tar.gz \
    && echo "${PICAT_SHA256}  picat.tar.gz" | sha256sum -c - \
    && tar -xzf picat.tar.gz -C /opt \
    && ln -s /opt/Picat/picat /usr/local/bin/picat \
    && rm picat.tar.gz

WORKDIR /opt/fzn_picat
ARG FZN_PICAT_COMMIT=8b6ba4517669bbf856f8b2661b2e8e52d5ad081d
ARG FZN_PICAT_SHA256=0ed8995177bd1251ad0433f8c5e7806e0a5a82d96ecc2e20d10b840c4f330b9e
RUN wget -qO source.tar.gz https://github.com/nfzhou/fzn_picat/archive/${FZN_PICAT_COMMIT}.tar.gz \
    && echo "${FZN_PICAT_SHA256}  source.tar.gz" | sha256sum -c - \
    && tar -xzf source.tar.gz --strip-components=1 \
    && rm source.tar.gz

FROM base-small AS final

# Install Python
RUN apt-get update -qq && apt-get install -qq -y --no-install-recommends \
    software-properties-common \
    && add-apt-repository universe \
    && add-apt-repository ppa:deadsnakes/ppa \
    && apt-get install -qq -y \
        python3.13 \
        python3.13-venv \
    && apt-get clean -qq && rm -rf /var/lib/apt/lists/*
RUN python3.13 -m ensurepip --upgrade

COPY command-line-ai/requirements.txt ./requirements.txt
RUN python3.13 -m pip install --quiet -r ./requirements.txt

COPY --from=scip /opt/scip/package.deb ./scip-package.deb
RUN apt-get update -qq && apt-get install -qq -y ./scip-package.deb \
    && rm ./scip-package.deb \
    && apt-get clean -qq && rm -rf /var/lib/apt/lists/*

COPY --from=mzn2feat /opt/mzn2feat/bin/mzn2feat /usr/local/bin/mzn2feat
COPY --from=mzn2feat /opt/mzn2feat/bin/fzn2feat /usr/local/bin/fzn2feat

# Install solver configurations
COPY --from=solver-configs /solvers/*.msc /usr/local/share/minizinc/solvers/

# Copy solver files
COPY ./solvers/picat/wrapper.sh /usr/local/bin/fzn-picat

COPY --from=huub /huub/target/release/fzn-huub /usr/local/bin/fzn-huub
COPY --from=huub /huub/share/minizinc/huub/ /usr/local/share/minizinc/huub/

COPY --from=picat /opt/Picat/picat /usr/local/bin/picat
COPY --from=picat /opt/fzn_picat/ /opt/fzn_picat/
COPY --from=yuck /opt/yuck/ /opt/yuck/
COPY --from=or-tools /opt/or-tools/ /opt/or-tools/
COPY --from=choco /opt/choco/ /opt/choco/
COPY --from=pumpkin /opt/pumpkin/ /opt/pumpkin/
COPY --from=gecode /opt/gecode/ /opt/gecode/
COPY --from=chuffed /opt/chuffed/ /opt/chuffed/
COPY --from=dexter /opt/dexter/ /opt/dexter/

COPY ./minizinc/Preferences.json /root/.minizinc/
COPY --from=builder /usr/src/app/target/release/parasol /usr/local/bin/parasol
COPY command-line-ai ./command-line-ai

# Gecode also uses dynamically linked libraries (DLL), so register these with the system.
# Note that Chuffed may be dependent on these same linked libraries, but I'm not sure.
# This is done at the very end to make sure it doesn't mess with other commands.
RUN echo "/opt/gecode/lib" > /etc/ld.so.conf.d/gecode.conf \
    && ldconfig

# NOTE: For CPLEX support:
#       1. Copy the cplex/bin/libcplexXXXX.so file from your CPLEX installation into the root of this repository and rename it to libcplex.so (this requires the Linux installation of CPLEX).
#       2. Uncomment the following line of code:
# COPY ./libcplex.so .


# NOTE: For FICO Xpress support:
#       1. Copy the entire xpressmp folder (the entire Xpress installation) into the root of this repository.
#       2. Uncomment the following line of code:
# COPY ./xpressmp/ /opt/xpressmp/

RUN parasol build-solver-cache

FROM builder AS ci

FROM final AS ci-end-to-end

    # Undo Gecode DLL modifications
RUN rm /etc/ld.so.conf.d/gecode.conf && ldconfig \
    # Remove Python apt-get repository
    && rm -f /etc/apt/sources.list.d/deadsnakes* \
    # Install build tools (because the CI builds the application)
    && apt-get update -qq \
    && apt-get install -y -qq --no-install-recommends \
    gcc \
    libc6-dev \
    # Cleanup
    && rm -rf /var/lib/apt/lists/* \
    # Redo Gecode DLL modifications (because they are needed at runtime)
    && echo "/opt/gecode/lib" > /etc/ld.so.conf.d/gecode.conf && ldconfig

# Fix paths for cargo
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH="${CARGO_HOME}/bin:${PATH}"

COPY --from=base /usr/local/cargo /usr/local/cargo
COPY --from=base /usr/local/rustup /usr/local/rustup

COPY Cargo.toml Cargo.lock ./
COPY ./src ./src
COPY ./tests ./tests

# Make the 'final' image the default image
FROM final

