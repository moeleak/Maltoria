#!/usr/bin/env bash
# Ubuntu 24.04 development and headless acceptance dependencies. Run as root.
# The Swift helper builds its file chooser against GTK 4 and reads covers
# through GdkPixbuf. The Material app loads X11, Wayland, xkbcommon and Vulkan
# at run time, so it needs their libraries but no development headers.
set -euo pipefail
apt-get update
apt-get install -y --no-install-recommends \
    build-essential clang curl ca-certificates pkg-config patchelf libssl-dev libsecret-tools gnome-keyring \
    libgdk-pixbuf-2.0-dev libgtk-4-dev webp-pixbuf-loader \
    libx11-6 libx11-xcb1 libxcursor1 libxi6 libxrandr2 libxkbcommon0 libxkbcommon-x11-0 \
    libwayland-client0 libwayland-cursor0 libwayland-egl1 libegl1 libvulkan1 mesa-vulkan-drivers \
    cmake ninja-build libwebp-dev \
    xvfb xauth dbus-x11 fonts-noto-cjk python3 desktop-file-utils xdg-utils
