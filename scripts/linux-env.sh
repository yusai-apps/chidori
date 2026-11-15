# Source this before cargo on Linux: `. scripts/linux-env.sh`.
#
# Linux is a test platform, not a release (docs/DECISIONS.md → Linux): CI and cloud sessions build,
# test and drive the app there. As on macOS, .cargo/config.toml's [env] carries MSVC flags for
# whisper.cpp; variables already in the environment win over [env].
#
# Packages (Ubuntu 24.04): build-essential cmake clang pkg-config libasound2-dev
# libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libfontconfig-dev
# libgtk-3-dev libxdo-dev libayatana-appindicator3-dev; to drive the app without a
# screen, also xvfb and mesa-vulkan-drivers (software Vulkan).
export CMAKE_C_FLAGS_RELEASE="-O3 -DNDEBUG"
export CMAKE_CXX_FLAGS_RELEASE="-O3 -DNDEBUG"
export CMAKE_C_FLAGS_RELWITHDEBINFO="-O2 -g -DNDEBUG"
export CMAKE_CXX_FLAGS_RELWITHDEBINFO="-O2 -g -DNDEBUG"
