#!/bin/sh
# Renders icon.svg into the bundle icons the dev channel uses
# (package.metadata.bundle in crates/zed/Cargo.toml) and a 256 px PNG for
# Linux packages. Needs rsvg-convert.
set -e
cd "$(dirname "$0")"
rsvg-convert -w 512 -h 512 icon.svg -o ../app-icon-dev.png
rsvg-convert -w 1024 -h 1024 icon.svg -o ../app-icon-dev@2x.png
rsvg-convert -w 256 -h 256 icon.svg -o sinter-256.png
