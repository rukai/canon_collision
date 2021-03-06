#!/bin/bash

# use -j2 as travis VM's have 2 cores https://docs.travis-ci.com/user/reference/overview/

set -ev

# prevent timeouts
# this is not ideal... but we have looooong compile times.
# and travis_wait doesnt work in bash
(
    while :
    do
        sleep 5m
        echo "☃"
    done
) &

# setup blender
wget https://ftp.nluug.nl/pub/graphics/blender/release/Blender2.83/blender-2.83.0-linux64.tar.xz
tar -xvf blender-2.83.0-linux64.tar.xz
PATH="$PWD/blender-2.83.0-linux64:$PATH"

# export .blend to .glb
cd assets_raw/models
python3 export_all_assets.py
cd ../..

# test and build
rustup update
cargo test --release -v --all
cd canon_collision
cargo build --release --no-default-features
cd ..
cargo build --release --all

if [ "${TRAVIS_PULL_REQUEST}" = "false" ]; then
    # package
    mkdir cc
    mv target/release/canon_collision cc/
    mv target/release/cc_cli cc/
    mv target/release/cc_map_controllers cc/
    mv package cc/
    mv assets cc/
    tar -cvzf canoncollision-${TRAVIS_COMMIT:0:15}-linux.tar.gz cc

    # upload
    echo "put canoncollision-${TRAVIS_COMMIT:0:15}-linux.tar.gz /home/ubuntu/CanonCollisionWebsite/builds/" | sftp ubuntu@ec2-13-210-166-146.ap-southeast-2.compute.amazonaws.com
fi
