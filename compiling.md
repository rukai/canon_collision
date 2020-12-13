# Setup for Windows

Install rust via https://www.rustup.rs/ using the default settings. If you already have rustup, ensure it is setup with the msvc toolchain.
Install [Build Tools for Visual Studio 2017](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2017)

Install ninja, cmake, python 3 and blender.
The recommended way to do this is to install [chocolatey](https://chocolatey.org) then run:
*   `choco install ninja`
*   `choco install cmake --installargs 'ADD_CMAKE_TO_PATH=System'`
*   `choco install python`
*   `choco install blender`

Make sure to add blender to your [PATH environment variable](https://www.architectryan.com/2018/03/17/add-to-the-path-on-windows-10/).
The path to blender will be somewhere like `C:\Program Files\Blender Foundation\Blender 2.83`.

<!--
Leave this out until gtk-rs get their act together
Install gtk, the recommended way is to run the following commands in cmd:
```cmd
git clone https://github.com/Microsoft/vcpkg
cd vcpkg
bootstrap-vcpkg.bat
vcpkg install gtk:x64-windows
set VCPKGDIR=%CD%\installed\x64-windows
set PATH=%VCPKGDIR%\bin;%PATH%
set GTK_LIB_DIR=%VCPKGDIR%\lib
mklink %VCPKGDIR%\lib\gtk-3.lib %VCPKGDIR%\lib\gtk-3.0.lib
mklink %VCPKGDIR%\lib\gdk-3.lib %VCPKGDIR%\lib\gdk-3.0.lib
mklink %VCPKGDIR%\bin\gtk-3.0.dll %VCPKGDIR%\bin\gtk-3.dll
mklink %VCPKGDIR%\bin\gdk-3.0.dll %VCPKGDIR%\bin\gdk-3.dll
mkdir %VCPKGDIR%\etc
mkdir %VCPKGDIR%\etc\gtk-3.0
echo "[Settings]" > %VCPKGDIR%\etc\gtk-3.0\settings.ini
echo "gtk-theme-name=win32" > %VCPKGDIR%\etc\gtk-3.0\settings.ini
```
-->

# Setup for Ubuntu

Install rust via https://www.rustup.rs/ (Use the default settings)

```
sudo apt-get install build-essential libssl-dev libusb-1.0-0-dev pkg-config cmake libvulkan-dev vulkan-utils libudev-dev blender
```

Currently the blender provided by ubuntu is too old.
Instead you must manually download the latest blender for linux: https://www.blender.org/download/
Then you need to add the path containing the blender executable to your PATH environment variable.
Maybe the CI script will help illustrate the simplest possible commands to do this: https://github.com/rukai/canon_collision/blob/44fa7c1c2335292e6c0b41d07185cc470c58e1c9/travis.sh#L19
But you will probably want to set it up better for your own usage.

Need to also install one of the following packages depending on your graphics card:
*   Intel: sudo apt-get install mesa-vulkan-drivers
*   Nvidia: No extra drivers required
*   AMD:   TODO

If it fails to launch, you may need to enable DRI3,
Create a file /etc/X11/xorg.conf.d/20-intel.conf containing:
```
Section "Device"
   Identifier  "Intel Graphics"
   Driver      "intel"
   Option      "DRI" "3"
EndSection
```

# Setup for Arch

```
sudo pacman -Syu rustup gcc make python libusb cmake vulkan-icd-loader blender
```

Need to also install one of the following packages depending on your graphics card:
*   Intel: vulkan-intel
*   Nvidia: No extra drivers required
*   AMD:   vulkan-radeon

# Compile and run the game

1.  In the assets_raw/models directory run: `python export_all_assets.py`
2.  In the canon_collision directory run: `cargo run --release`

# Compile and run the Controller Mapper

In the map_controllers directory run: `cargo run --release`

# Setup CLI

To build the CLI tool run `cargo build` in the cc_cli directory, the resulting binary is stored at `target/debug/cc_cli`.
Copy `cc_cli` to somewhere in your PATH.
