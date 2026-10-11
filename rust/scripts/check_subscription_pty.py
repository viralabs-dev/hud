import runpy
from pathlib import Path
runpy.run_path(str(Path(__file__).with_name("check_real_launcher_pty.py")),run_name="__main__")
