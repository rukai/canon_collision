import bpy
import os
import pathlib

def main():
    if not os.path.isfile("export.py"):
        print("Script needs to be run from the folder it is stored in")
        return

    out_dir = pathlib.Path("../../assets/models")
    out_dir.mkdir(parents=True, exist_ok=True)

    for filename in os.listdir("."):
        if os.path.isdir(filename):
            blend_path = pathlib.Path(filename).joinpath(filename + ".blend")
            bpy.ops.wm.open_mainfile(filepath=str(blend_path))

            gltf_path = out_dir.joinpath(filename)
            bpy.ops.export_scene.gltf(filepath=str(gltf_path))

main()
