import sys; sys.path.insert(0,'/tmp')
from png import load
import numpy as np
def runs(v):
    out=[];s=p=None
    for x in v:
        if s is None: s=p=x; continue
        if x==p+1: p=x
        else: out.append((s,p)); s=p=x
    if s is not None: out.append((s,p))
    return out
def bg_runs(a,y,x0,x1,white=250):
    row=a[y].mean(axis=1)
    cols=[x for x in range(x0,x1) if row[x]<white]
    return runs(cols)
def hbands(a,x,y0,y1,white=250):
    col=a[:,x].mean(axis=1)
    rows=[y for y in range(y0,y1) if col[y]<white]
    return runs(rows)
for p,l in [('reference/reference-window.png','REF'),('council/round-32/01-default.png','R32-01')]:
    a=load(p)
    print('==',l)
    print(' tab chip row y=123 dark run:',bg_runs(a,123,0,232))
    print(' chip vertical x=100:',hbands(a,100,95,150))
    print(' chip row2 y=154:',bg_runs(a,154,0,232))
    print(' chip row3 y=186:',bg_runs(a,186,0,232))
    print(' chip row4 y=217:',bg_runs(a,217,0,232))
    print(' newtab y=249:',bg_runs(a,249,0,232))
    print(' small chips y=82:',bg_runs(a,82,0,232))
    print(' x=8 col rows0-110:',hbands(a,8,0,110))
