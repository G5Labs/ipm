from pathlib import Path
import runpy
from harness import main
here=Path(__file__).resolve().parent
projection=runpy.run_path(str(here/'model-lane/check-settings-projection.py'))
assert projection['check']()['checked']
inputs=runpy.run_path(str(here/'check_inputs.py'))
assert inputs['check']()['checked']
main(here)
