#!/usr/bin/env bash
set -euo pipefail
workspace=${1:-$(cd "$(dirname "$0")/../../.." && pwd)}
case ${2:-x86_64} in
  x86_64) docker_arch=amd64; sdk=linux ;;
  aarch64) docker_arch=arm64; sdk=linux-arm64 ;;
  *) echo "Unsupported Linux architecture: $2" >&2; exit 1 ;;
esac
mkdir -p "$workspace/target/cross-sdk/$sdk"
docker run --rm --platform "linux/$docker_arch" -v "$workspace/target/cross-sdk/$sdk:/out" -w /out ubuntu:22.04 bash -c 'apt-get update -qq && apt-get download libstdc++-12-dev libgcc-12-dev libstdc++6 libgcc-s1 libc6-dev && for file in ./*.deb; do dpkg-deb -x "$file" sysroot; done'
