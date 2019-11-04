#!/usr/bin/env python3

# Originally I had this script combined with the export.py,
# But it turns out that that is C U R S E D
# Exporting textures would fail if they were outside of the main directory with the most obscure error.
# This error here: https://github.com/KhronosGroup/glTF-Blender-IO/blob/860b8103cd2c09ced79b261ddb9e0e64243e6014/addons/io_scene_gltf2/blender/exp/gltf2_blender_gather_image.py#L165

import os
import pathlib
import subprocess
import sys

def main():
    if not os.path.isfile("export.py"):
        print("Script needs to be run from the folder it is stored in")
        return

    args = sys.argv[1:]

    out_dir = pathlib.Path("../../assets/models")
    out_dir.mkdir(parents=True, exist_ok=True)

    for filename in os.listdir("."):
        if os.path.isdir(filename) and filename != 'Shared' and (len(args) == 0 or filename in args):
            blend_path = pathlib.Path(filename).joinpath(filename + ".blend")
            subprocess.run(["blender", str(blend_path), "-b", "-P", "export.py"])

main()
