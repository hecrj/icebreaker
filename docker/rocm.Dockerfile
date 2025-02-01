ARG UBUNTU_VERSION=24.04

ARG ROCM_VERSION=6.3
ARG AMDGPU_VERSION=6.3

ARG BASE_ROCM_DEV_CONTAINER=rocm/dev-ubuntu-${UBUNTU_VERSION}:${ROCM_VERSION}-complete

### Build image
FROM ${BASE_ROCM_DEV_CONTAINER} AS build

ARG ROCM_DOCKER_ARCH='gfx1010,gfx1030,gfx1032,gfx1100,gfx1101,gfx1102'

ENV AMDGPU_TARGETS=${ROCM_DOCKER_ARCH}

RUN apt-get update \
    && apt-get install -y \
    build-essential \
    cmake \
    git \
    libcurl4-openssl-dev \
    curl \
    libgomp1

WORKDIR /app

RUN git clone https://github.com/ggerganov/llama.cpp --depth=1 .

RUN HIPCXX="$(hipconfig -l)/clang" HIP_PATH="$(hipconfig -R)" \
    cmake -S . -B build -DLLAMA_HIPBLAS=ON -DGGML_HIP=ON -DAMDGPU_TARGETS=$ROCM_DOCKER_ARCH -DCMAKE_BUILD_TYPE=Release -DLLAMA_CURL=ON \
    && cmake --build build --config Release -j$(nproc)

RUN mkdir -p /app/lib \
    && find build -name "*.so" -exec cp {} /app/lib \;

RUN mkdir -p /app/full \
    && cp build/bin/* /app/full \
    && cp *.py /app/full \
    && cp -r gguf-py /app/full \
    && cp -r requirements /app/full \
    && cp requirements.txt /app/full \
    && cp .devops/tools.sh /app/full/tools.sh

# Server, Server only
FROM ${BASE_ROCM_DEV_CONTAINER} AS server

RUN apt-get update \
    && apt-get install -y libgomp1 curl\
    && apt autoremove -y \
    && apt clean -y \
    && rm -rf /tmp/* /var/tmp/* \
    && find /var/cache/apt/archives /var/lib/apt/lists -not -name lock -type f -delete \
    && find /var/cache -type f -delete

COPY --from=build /app/lib/ /app
COPY --from=build /app/full/llama-server /app

ENV LLAMA_ARG_HOST=0.0.0.0

WORKDIR /app

HEALTHCHECK CMD [ "curl", "-f", "http://localhost:8080/health" ]

ENTRYPOINT [ "/app/llama-server" ]
