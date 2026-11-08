#!/usr/bin/env bash
# Builds the Linux engine the app downloads on first run: stable-diffusion.cpp's sd-server
# with CUDA 12, from the same tag and with the same options as upstream's Windows
# "cuda12" release build, plus the CUDA runtime libraries it needs.
#
# Output (in $OUT_DIR, default ./dist-engine):
#   sd-master-f5df5f8-bin-linux-cuda12-x64.tar.gz   sd-server, sd-cli, libstable-diffusion.so, ggml backends
#   cudart-sd-bin-linux-cu12-x64.tar.gz             libcudart.so.12, libcublas.so.12, libcublasLt.so.12
# Both archives are flat (no top folder) and every binary has RUNPATH=$ORIGIN, so the app
# extracts them into runtime/ and runs runtime/sd-server without LD_LIBRARY_PATH tricks.
#
# Needs: git, cmake, g++, and the CUDA 12 toolkit (nvcc, cudart, cuBLAS). No GPU needed.
# Environment:
#   CUDA_HOME      toolkit root (default /usr/local/cuda)
#   CUDA_ARCHS     CMAKE_CUDA_ARCHITECTURES (default: upstream's list, Pascal to Blackwell)
#   JOBS           parallel build jobs (default: one per 4 GB of RAM, at most nproc)
#   OUT_DIR        where the archives go
set -euo pipefail

SD_TAG="master-1052-f5df5f8"
SD_SHORT="f5df5f8"
CUDA_HOME="${CUDA_HOME:-/usr/local/cuda}"
CUDA_ARCHS="${CUDA_ARCHS:-61;70;75;80;86;89;90;100;120}"
# The CUTLASS INT8/attention kernels take a few GB of RAM per nvcc job: by default one job
# per 4 GB of RAM (at most one per core), so a 16 GB runner doesn't run out of memory.
if [ -z "${JOBS:-}" ]; then
  mem_gb=$(awk '/MemTotal/ {print int($2 / 1048576)}' /proc/meminfo)
  JOBS=$(( mem_gb / 4 )); [ "$JOBS" -lt 1 ] && JOBS=1
  [ "$JOBS" -gt "$(nproc)" ] && JOBS=$(nproc)
fi
OUT_DIR="$(realpath -m "${OUT_DIR:-dist-engine}")"
WORK="$(realpath -m "${WORK_DIR:-build-linux-engine}")"

ENGINE_TAR="sd-master-${SD_SHORT}-bin-linux-cuda12-x64.tar.gz"
CUDART_TAR="cudart-sd-bin-linux-cu12-x64.tar.gz"

export PATH="$CUDA_HOME/bin:$PATH"
nvcc --version | tail -n 2

LIB="$CUDA_HOME/lib64"
[ -d "$LIB" ] || LIB="$CUDA_HOME/targets/x86_64-linux/lib"

mkdir -p "$WORK" "$OUT_DIR"
if [ ! -d "$WORK/sd/.git" ]; then
  git clone --depth 1 --branch "$SD_TAG" --recursive --shallow-submodules \
    https://github.com/leejet/stable-diffusion.cpp "$WORK/sd"
fi

cmake -S "$WORK/sd" -B "$WORK/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_CUDA_COMPILER="$CUDA_HOME/bin/nvcc" \
  -DCUDAToolkit_ROOT="$CUDA_HOME" \
  -DSD_CUDA=ON \
  -DCMAKE_CUDA_ARCHITECTURES="$CUDA_ARCHS" \
  -DSD_BUILD_SHARED_LIBS=ON -DSD_BUILD_SHARED_GGML_LIB=ON \
  -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_NATIVE=OFF \
  -DGGML_OPENMP=OFF \
  -DSD_SERVER_BUILD_FRONTEND=OFF \
  -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON -DCMAKE_INSTALL_RPATH='$ORIGIN' \
  -DCMAKE_EXE_LINKER_FLAGS="-Wl,-rpath-link,$LIB" \
  -DCMAKE_SHARED_LINKER_FLAGS="-Wl,-rpath-link,$LIB"
# The build RPATH is $ORIGIN only (that is what ships), and libstable-diffusion.so links
# cudart and cuBLAS itself, so -rpath-link tells the linker where they are at build time.

# Everything, not just the sd-server target: with GGML_BACKEND_DL the CUDA and CPU
# backends are modules loaded at run time, not link dependencies of sd-server.
cmake --build "$WORK/build" --config Release -j "$JOBS"

BIN="$WORK/build/bin"
test -x "$BIN/sd-server"

# Engine archive: the built programs and libraries plus the licenses.
STAGE="$WORK/stage-engine"
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp -a "$BIN"/. "$STAGE"/
cp "$WORK/sd/LICENSE" "$STAGE/stable-diffusion.cpp-LICENSE.txt"
if [ -f "$WORK/sd/ggml/LICENSE" ]; then cp "$WORK/sd/ggml/LICENSE" "$STAGE/ggml-LICENSE.txt"; fi
tar -C "$STAGE" -czf "$OUT_DIR/$ENGINE_TAR" .

# CUDA runtime archive: only the sonames the engine links against (NVIDIA lists cudart,
# cuBLAS and cuBLASLt as redistributable in the CUDA Toolkit EULA, Attachment A).
STAGE="$WORK/stage-cudart"
rm -rf "$STAGE" && mkdir -p "$STAGE"
for so in libcudart.so.12 libcublas.so.12 libcublasLt.so.12; do
  cp -L "$LIB/$so" "$STAGE/$so"
done
tar -C "$STAGE" -czf "$OUT_DIR/$CUDART_TAR" .

# Report what the engine still needs from the system (should be glibc, libstdc++, libgcc
# and the NVIDIA driver's libcuda.so.1 only; OpenMP is off, so no libgomp).
echo "--- run-time dependencies outside the archives:"
for f in "$BIN"/sd-server "$BIN"/*.so; do
  ldd "$f" 2>/dev/null | awk '/=>/ {print $1}'
done | sort -u | grep -v -E '^(libstable-diffusion|libggml|libcudart|libcublas|libcublasLt|libsharpyuv|libwebp|libwebm)' || true
echo "--- newest glibc symbol used:"
for f in "$BIN"/sd-server "$BIN"/*.so; do objdump -T "$f" 2>/dev/null; done \
  | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -n 1
ls -l "$OUT_DIR"
