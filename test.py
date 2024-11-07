from pathlib import Path
from shutil import copy
from subprocess import run
from os import environ

vst_file = Path("./target/bundled/deepfilter-vst.vst3/Contents/x86_64-win/deepfilter-vst.vst3")
dest = Path("C:/Program Files/Common Files/VST3/deepfilter-vst.vst3")
if dest.exists():
    dest.unlink()
print(copy(vst_file, dest))


env = environ
env["NIH_LOG"] = "C:/Users/Turtvaiz/Downloads/deepfilter-vst/thing.log"
env["TRACT_LOG"] = "off"
env["RUST_LOG"] = "off"
run("C:/Program Files/Audacity/Audacity.exe", env=env)
# run("C:/Program Files/Adobe/Adobe Audition 2023/Adobe Audition.exe", env=env)
