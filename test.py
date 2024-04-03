from pathlib import Path
from shutil import copy
from subprocess import run
from os import environ

vst_file = Path("./target/bundled/Vst-Filter.vst3/Contents/x86_64-win/Vst-Filter.vst3")
dest = Path("C:/Program Files/Common Files/VST3/Vst-Filter.vst3")
print(copy(vst_file, dest))


env = environ
env["NIH_LOG"] = "C:/Users/Turtvaiz/Downloads/vst-filter/thing.log"
run("C:/Program Files/Audacity/Audacity.exe", env=env)
