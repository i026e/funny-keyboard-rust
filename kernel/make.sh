#!/usr/bin/env sh
# Use your current system's config
zcat /proc/config.gz > .config
cp /lib/modules/$(uname -r)/build/Module.symvers .

# Update the config for this source tree
make LLVM=1 olddefconfig

# Verify Rust is ready
make LLVM=1 rustavailable

# Generate the Rust crates and target.json
make LLVM=1 prepare


echo $(make -s kernelrelease)