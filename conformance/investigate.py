#!/usr/bin/env python3
"""Reproduce SQLg optimizer differences in the disposable fixture database."""
import json,datetime
from pathlib import Path
from run import Sqlg
engine=Sqlg();results=[]
try:
 for query in [
  'g.inject([1,2],[3]).local(unfold().sum())',
  'g.withoutStrategies(org.umlg.sqlg.strategy.barrier.SqlgLocalStepStrategy).inject([1,2],[3]).local(unfold().sum())',
  "g.V().not(out('KNOWS')).values('name')",
  "g.withoutStrategies(org.umlg.sqlg.strategy.SqlgGraphStepStrategy).V().not(out('KNOWS')).values('name')",
 ]:results.append({'query':query,**engine.query({'query':query})})
finally:engine.close()
Path(__file__).with_name('data').joinpath('sqlg-investigation.json').write_text(json.dumps({'version':'3.1.6','tested_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'results':results},indent=2)+'\n')
