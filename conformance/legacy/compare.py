"""Typed, duplicate-preserving result comparison."""
import math

def equivalent(a,b):
 if a is None or b is None: return a is b
 if isinstance(a,bool) or isinstance(b,bool): return type(a)==type(b) and a==b
 if isinstance(a,(int,float)) and isinstance(b,(int,float)): return math.isclose(a,b,rel_tol=1e-9,abs_tol=1e-9)
 if isinstance(a,list) and isinstance(b,list): return len(a)==len(b) and all(equivalent(x,y) for x,y in zip(a,b))
 if isinstance(a,dict) and isinstance(b,dict): return a.keys()==b.keys() and all(equivalent(a[k],b[k]) for k in a)
 return type(a)==type(b) and a==b

def equal_rows(actual,expected,ordered):
 if ordered: return equivalent(actual,expected)
 remaining=list(actual)
 for row in expected:
  match=next((i for i,r in enumerate(remaining) if equivalent(r,row)),None)
  if match is None:return False
  remaining.pop(match)
 return not remaining
