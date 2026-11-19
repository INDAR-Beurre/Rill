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
def hb(a,x,y0,y1,thr=252):
    col=a[:,x].astype(int)
    g=col[:,0]*0.299+col[:,1]*0.587+col[:,2]*0.114
    return [(a_,b) for a,b in runs([y for y in range(y0,y1) if g[y]<thr])]
def vr(y0,y1,x0,x1,thr=252):
    pass
for p,l in [('council/round-32/01-default.png','01'),('council/round-32/02-pins.png','02'),('council/round-32/03-tabs.png','03'),('council/round-32/04-omni.png','04')]:
    a=load(p).astype(int)
    g=a[:,:,0]*0.299+a[:,:,1]*0.587+a[:,:,2]*0.114
    s=g[:,:232]
    rows=[y for y in range(0,460) if (s[y]<250).sum()>3]
    print(p, 'rows:', runs(rows))
    for r in [(0,40),(40,80),(40,110),(100,140)]:
        seg=[y for y in range(*seg2) ] if False else None
