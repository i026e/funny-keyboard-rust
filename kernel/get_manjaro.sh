#!/usr/bin/env sh

sudo pacman -Syyy lld bc llvm cpio rust-bindgen clang makepkg

git clone https://gitlab.manjaro.org/packages/core/linux618.git

cd linux618

BUILDDIR=. makepkg -o && cp -rn src/*/* . && rm -rf src