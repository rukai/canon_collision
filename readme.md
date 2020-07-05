# Canon Collision [![Build Status](https://travis-ci.com/rukai/canon_collision.svg?branch=master)](https://travis-ci.com/rukai/canon_collision) [![Build status](https://ci.appveyor.com/api/projects/status/89drle66lde9pq35?svg=true)](https://ci.appveyor.com/project/rukai/canon-collision)

## Quick links

*   [Compile from source (Windows & Linux)](compiling.md)
*   [Discord](https://discord.gg/KyjBs4x)
*   [Infrastructure Repository](https://github.com/rukai/pf_sandbox_infra)

## OS/Controller requirements

*   Windows 10: Xbox controllers + native GC adapter
*   Other Windows: [Unsupported](https://gitlab.com/Arvamer/gilrs/commit/56bf4e2d04c972a73cb195afff2a9a8563f6aa34#note_58842780)
*   Linux: All controllers + native GC adapter
*   Mac OS: Unsupported

You cannot use a keyboard to play, you must use a controller.

## CI Infrastructure

We build and test on:

*   Rust stable/nightly - Linux 64 bit (Travis)
*   Rust stable/nightly GNU - Windows 64 bit (Appveyor)

We build and test when:

*   All incoming pull requests are built and tested.
*   Every commit merged to master is built, tested and then an incrementing tag/release is created for it.
