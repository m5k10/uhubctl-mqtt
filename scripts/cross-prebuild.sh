#!/bin/sh
set -e

# CMake FindOpenSSL cannot find vendored openssl-sys output under
# cross-compilation because the toolchain file restricts searching
# to the sysroot only. Relax this so OPENSSL_ROOT_DIR hints work.
echo 'set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY BOTH)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE BOTH)' >> /opt/toolchain.cmake
