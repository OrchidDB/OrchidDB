#!/usr/bin/env bash
set -euo pipefail
workspace=${1:-$(cd "$(dirname "$0")/../../.." && pwd)}
mkdir -p "$workspace/target/cross-sdk/linux"
docker run --rm --platform linux/amd64 -v "$workspace/target/cross-sdk/linux:/out" -w /out ubuntu:22.04 bash -c 'apt-get update -qq && apt-get download libstdc++-12-dev libgcc-12-dev libstdc++6 libgcc-s1 libc6-dev && for file in ./*.deb; do dpkg-deb -x "$file" sysroot; done'
