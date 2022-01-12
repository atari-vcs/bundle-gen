#!/bin/bash
set -e

docker build . -f Dockerfile.bundle-gen -t bundle-gen
docker build . -f Dockerfile.bundle-run -t bundle-run
