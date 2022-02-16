#!/bin/bash
#
# Copyright 2022 Collabora, Ltd.
#
# SPDX-License-Identifier: MIT
#
set -e

if [ $# -ne 1 ]; then
    echo >&2 "Usage: $0 <file>"
    exit 1
fi

# This is the current directory, which is assumed to be your preferred build directory
BUILD="$(realpath "$(pwd)")"
# This is the location of your spec, which is assumed to be the root of your repository
SOURCE="$(realpath "$(dirname "$1")")"
# This is the name of your spec, as it is mapped in the container
SPEC="/source/$(basename "$1")"

docker run --rm -v "${BUILD}:/build" -v "${SOURCE}:/source:ro" -w /build ghcr.io/atari-vcs/bundle-gen:builder "${SPEC}"
