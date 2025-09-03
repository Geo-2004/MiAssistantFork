#!/bin/bash
# Compile with pkgconf for accurate library linking
gcc -Wall -O2 miasst_patched.c tiny-json/tiny-json.c -I. \
    -o miasst_windows64.exe \
    $(pkgconf --cflags --libs libusb-1.0 libcurl openssl) \
    2>&1 || echo "Compilation failed"
