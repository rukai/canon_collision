import bpy
import pathlib

def main():
    out_dir = pathlib.Path("../../assets/models")
    filename = pathlib.Path(bpy.data.filepath).parent.name

    gltf_path = out_dir.joinpath(filename)
    bpy.ops.export_scene.gltf(filepath=str(gltf_path),export_apply=True)

main()
