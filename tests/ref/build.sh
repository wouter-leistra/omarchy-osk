#!/bin/bash
# build the C reference harness to $1 (default /tmp/osk-ref)
set -e
cd "$(dirname "$0")/../.."
for x in proto/*.xml; do
  b=${x%.xml}
  [ -f "$b-client-protocol.h" ] || wayland-scanner client-header < "$x" > "$b-client-protocol.h"
done
cc -std=gnu99 -O1 -g -Wall -Wno-unused-function -I. \
  -DLAYOUT='"layout.omarchy.h"' -DKEYMAP='"keymap.omarchy.h"' \
  $(pkg-config --cflags wayland-client xkbcommon pangocairo) \
  -o "${1:-/tmp/osk-ref}" tests/ref/harness.c keyboard.c swipe.c langs.c drw.c \
  shm_open.c os-compatibility.c \
  $(pkg-config --libs xkbcommon pangocairo) -lm -lrt
