#!/usr/bin/env sh

sudo pacman -S devtools lld bc llvm cpio bindgen clang rust-bindgen rustc

git clone --depth 1 https://github.com/archlinux/linux.git
cd linux

git fetch --tags

# TODO
echo $(uname -r)
git checkout v6.18.9-arch1
echo "-2" > localversion-arch


