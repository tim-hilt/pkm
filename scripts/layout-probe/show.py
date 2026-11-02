import json,sys
d=json.load(open(sys.argv[1])); d=json.loads(d) if isinstance(d,str) else d
keys=sys.argv[2:]
def fmt(o):
    if o is None: return 'NONE'
    s=[]
    r=o.get('rect'); s.append('[%g,%g %gx%g]'%tuple(r))
    for k,v in o.items():
        if k in('rect',): continue
        if k=='fontFamily': v=v.split(',')[0]
        s.append('%s=%s'%(k,v))
    return ' '.join(s)
for k,v in d.items():
    if keys and k not in keys: continue
    if k=='__vars':
        print('VARS',json.dumps(v)); continue
    if k=='__viewport': print('VIEWPORT',v); continue
    if isinstance(v,list):
        print(k+':')
        for x in v: print('   ',fmt(x))
    else: print(k+':',fmt(v))
