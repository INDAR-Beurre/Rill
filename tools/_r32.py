import sys; sys.path.insert(0,'/tmp')
from png import load
import numpy as np

def dark_rows(a, x0,x1,y0,y1, thr=140):
    sub=a[y0:y1,x0:x1].mean(axis=2)
    return [(y0+i) for i in range(sub.shape[0]) if (sub[i]<thr).sum()>0]

def bbox(a,x0,x1,y0,y1,thr=140):
    sub=a[y0:y1,x0:x1].mean(axis=2)
    ys,xs=np.where(sub<thr)
    if len(ys)==0: return None
    return (x0+xs.min(),x0+xs.max(),y0+ys.min(),y0+ys.max())

def runs(vals):
    out=[];s=None;p=None
    for v in vals:
        if s is None: s=v; p=v; continue
        if v==p+1: p=v
        else: out.append((s,p)); s=v; p=v
    if s is not None: out.append((s,p))
    return out

def lines(a,x0,x1,y0,y1,thr=150,minpix=1):
    sub=a[y0:y1,x0:x1].mean(axis=2)
    rows=[y0+i for i in range(sub.shape[0]) if (sub[i]<thr).sum()>=minpix]
    return runs(rows)

def report(path,label):
    a=load(path)
    print('---',label)
    # separator column near 230
    for y in (300,600):
        row=a[y].mean(axis=1)
        cols=[x for x in range(200,260) if row[x]<252]
        print(' sep y=%d cols=%s'%(y,cols))
    print(' content lines x560-1180:',[(a,b) for a,b in [(s,e) for s,e in lines(a,560,1180,340,660)]])
    print(' title bbox', bbox(a,540,1200,360,395))
    print(' button bbox', bbox(a,540,1200,570,640,thr=200))
    print(' btn x-extent y=610', runs([x for x in range(500,800) if a[610,x].mean()<200]))
for p,l in [('reference/reference-window.png','REF'),('council/round-32/01-default.png','R32-01')]:
    report(p,l)
