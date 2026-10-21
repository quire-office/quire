import re, sys
def parse(p):
    out={}
    for line in open(p,encoding='utf-8'):
        if not line.startswith('| corpus'): continue
        cells=[c.strip() for c in line.strip().strip('|').split('|')]
        f=cells[0]
        sims=[float(x) for x in re.findall(r'[\d.]+', cells[4])[:6]] if cells[4] else []
        out[f]=(cells[1],cells[2],sims)
    return out
a=parse(sys.argv[1]); b=parse(sys.argv[2])
def avg(d):
    vals=[s for v in d.values() for s in v[2]]
    return sum(vals)/len(vals)
ok_a=sum(1 for v in a.values() if v[1].isdigit() and v[0]==v[1])
ok_b=sum(1 for v in b.values() if v[1].isdigit() and v[0]==v[1])
print(f"files={len(a)}/{len(b)}  avg {avg(a):.4f} -> {avg(b):.4f}  page-match {ok_a} -> {ok_b}")
diffs=[]
for k in b:
    x,y=a.get(k),b[k]
    if x is None: print('NEW',k); continue
    if x[0]!=y[0] or x[1]!=y[1]: diffs.append((k,f"PAGES {x[0]}/{x[1]} -> {y[0]}/{y[1]}"))
    elif len(x[2])!=len(y[2]): diffs.append((k,f"LENS {x[2]} -> {y[2]}"))
    else:
        for i,(u,v) in enumerate(zip(x[2],y[2])):
            if abs(u-v)>0.0005: diffs.append((k,f"p{i+1} {u:.3f} -> {v:.3f}"))
print(f"diffs={len(diffs)}")
for d in diffs: print('  ',d[0],d[1])
