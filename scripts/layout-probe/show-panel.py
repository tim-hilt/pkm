import json,sys
d=json.loads(json.load(open(sys.argv[1])))
for f in d:
    print('##',f['legend'],f['rect'])
    seen=set()
    for i in f['items']:
        k=tuple(i['rect'])
        if k in seen: continue
        seen.add(k)
        print('   ',i['t'][:28].ljust(28),i['rect'],'bg',i['bg'],'r',i['br'],'chk',i['checked'],'svg',i['svg'],i['cls'][:40])
