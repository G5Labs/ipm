from pathlib import Path
import sys
here=Path(__file__).resolve().parent
sys.path.insert(0,str(here.parent/'common30'))
sys.path.insert(0,str(here))
from harness import main
main(here)
