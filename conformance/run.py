#!/usr/bin/env python3
"""Primary entry point: upstream suites, executed locally only."""
import runpy,sys
from pathlib import Path
entry=Path(__file__).resolve().parent/'upstream'
sys.path.insert(0,str(entry))
runpy.run_path(str(entry/'run.py'),run_name='__main__')
