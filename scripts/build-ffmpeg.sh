#!/usr/bin/env bash
#
# Build a trimmed static FFmpeg for LivePorter (Windows, MSYS2 MinGW-w64).
#
# Run from an MSYS2 shell (the script sets up the MinGW64 toolchain paths itself):
#   bash scripts/build-ffmpeg.sh <ffmpeg-source-dir> <output-dir>
#
# Goal: ffmpeg.exe + ffprobe.exe around 15-25 MB, only the components the app uses:
#   - HEIC/JPG/PNG stills  -> deps: mov + image2 demuxers, hevc/h264/mjpeg/png/webp decoders
#   - MOV/MP4 video        -> mov demuxer, hevc/h264 decoders
#   - thumbnails           -> scale filter, libwebp encoder, webp muxer
#   - preview clips        -> scale filter, libx264 encoder, mp4 muxer (-an)
#   - content identifier   -> ffprobe on mov
set -euo pipefail

SRC="${1:?usage: build-ffmpeg.sh <ffmpeg-source-dir> <output-dir>}"
OUT="${2:?usage: build-ffmpeg.sh <ffmpeg-source-dir> <output-dir>}"

export PATH="/mingw64/bin:$PATH"
export PKG_CONFIG_PATH="/mingw64/lib/pkgconfig"
export PKG_CONFIG_LIBDIR="/mingw64/lib/pkgconfig"

mkdir -p "$OUT"
OUT_ABS="$(cd "$OUT" && pwd)"
cd "$SRC"

JOBS="${JOBS:-$(nproc)}"

make distclean >/dev/null 2>&1 || true

./configure \
  --prefix="$OUT_ABS" \
  --arch=x86_64 \
  --target-os=mingw32 \
  --enable-static \
  --disable-shared \
  --enable-small \
  --disable-debug \
  --disable-doc \
  --disable-network \
  --disable-autodetect \
  --disable-everything \
  --enable-gpl \
  --enable-demuxer=mov,image2 \
  --enable-decoder=hevc,h264,mjpeg,png,webp \
  --enable-parser=hevc,h264 \
  --enable-encoder=libx264,libwebp \
  --enable-muxer=mp4,webp \
  --enable-filter=scale,format,null,xstack \
  --enable-protocol=file,pipe \
  --enable-libx264 \
  --enable-libwebp \
  --enable-ffmpeg \
  --enable-ffprobe \
  --disable-ffplay \
  --pkg-config-flags=--static \
  --extra-ldflags=-static

make -j"$JOBS"
make install

echo "=== built binaries ==="
ls -la "$OUT_ABS/bin"
