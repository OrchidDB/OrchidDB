#!/usr/bin/env python3
"""Index every imported case, preserving the corpus's actual scope and identity."""
import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
rows=[]
for suite,base in [('Ladybug','cases/cypher/ladybug'),('TinkerPop','cases/gremlin/tinkerpop')]:
 for p in sorted((ROOT/base).rglob('*.case')):
  raw=p.read_bytes();s=raw.decode();meta=json.loads(s.split('--- metadata\n',1)[1].split('\n',1)[0])
  rows.append({'suite':suite,'group':str(p.parent.relative_to(ROOT/base)),'id':meta['id'],'path':str(p.relative_to(ROOT)),'sha256':hashlib.sha256(raw).hexdigest(),'dataset':meta.get('dataset',''),'source':meta.get('source','')})
(ROOT/'conformance/data/inventory.json').write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'imported cases indexed')
