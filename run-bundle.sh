#!/bin/bash
#
# Copyright 2022 Collabora, Ltd.
#
# SPDX-License-Identifier: MIT
#
set -e
set -x

if [ $# -ne 1 ]; then
    echo >&2 "Usage: $0 <file>"
    exit 1
fi

BUNDLE="$(realpath "$1")"
NAME="$(basename "${BUNDLE}")"

docker run --rm -it \
       -e DISPLAY="${DISPLAY}" \
       -v "${BUNDLE}:/tmp/${NAME}" \
       -v "/tmp/.X11-unix:/tmp/.X11-unix" \
       bundle-run:latest \
       "/tmp/${NAME}"
