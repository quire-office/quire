import re
def parse(p):
    out={}
    for line in open(p,encoding='utf-8'):
        if not line.startswith('| corpus'): continue
        cells=[c.strip() for c in line.strip().strip('|').split('|')]
        f=cells[0]
        sims=[float(x) for x in re.findall(r'[\d.]+', cells[4])[:6]] if cells[4] else []
        out[f]=(cells[1],cells[2],sims)
    return out
v5=parse('out/render-lab-sweep40-v5/report.md')
v6=parse('out/render-lab-sweep40-v6/report.md')
assert v5 and v6, (len(v5),len(v6))
def avg(d): 
    vals=[s for v in d.values() for s in v[2]]
    return sum(vals)/len(vals)
print(f"files v5={len(v5)} v6={len(v6)}  avg v5={avg(v5):.4f}  v6={avg(v6):.4f}")
page_ok5=sum(1 for v in v5.values() if v[1].isdigit() and v[0]==v[1])
page_ok6=sum(1 for v in v6.values() if v[1].isdigit() and v[0]==v[1])
diffs=[]
for k in v6:
    a,b=v5.get(k),v6.get(k)
    if a is None: print('NEW',k); continue
    if a[0]!=b[0] or a[1]!=b[1]: diffs.append((k,f"PAGES {a[0]}/{a[1]} -> {b[0]}/{b[1]}"))
    elif len(a[2])!=len(b[2]): diffs.append((k,f"LENS {a[2]} -> {b[2]}"))
    else:
        for i,(x,y) in enumerate(zip(a[2],b[2])):
            if abs(x-y)>0.0005: diffs.append((k,f"p{i+1} {x:.3f} -> {y:.3f}"))
print(f"page-count-match v5={page_ok5} v6={page_ok6}; diffs={len(diffs)}")
for d in diffs: print('  ',d[0],d[1])
