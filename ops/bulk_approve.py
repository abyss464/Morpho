import json, sqlite3, urllib.request, urllib.error
API="http://127.0.0.1:8787/api"; DB="/home/abysser/Code/learning/Morpho/data/working.db"
def api(path,body):
    req=urllib.request.Request(API+path,data=json.dumps(body).encode(),
        headers={"Content-Type":"application/json","X-Morpho-User":"operator-approve"},method="POST")
    try: urllib.request.urlopen(req,timeout=30); return True
    except urllib.error.HTTPError: return False
conn=sqlite3.connect(f"file:{DB}?mode=ro",uri=True)
active="SELECT word_id FROM words WHERE role='target' OR (role='auxiliary' AND aux_status='active')"
c={"definition":[0,0],"example":[0,0],"image":[0,0]}
for wid,pos in conn.execute(f"SELECT word_id,pos FROM definition_selections WHERE enabled=1 AND approved=0 AND word_id IN ({active})"):
    c["definition"][0 if api("/selections/definition/approve",{"word_id":wid,"pos":pos}) else 1]+=1
for wid,slot in conn.execute(f"SELECT word_id,slot FROM example_selections WHERE approved=0 AND word_id IN ({active})"):
    c["example"][0 if api("/selections/example/approve",{"word_id":wid,"slot":slot}) else 1]+=1
for (wid,) in conn.execute(f"SELECT word_id FROM image_selections WHERE approved=0 AND word_id IN ({active})"):
    c["image"][0 if api("/selections/image/approve",{"word_id":wid}) else 1]+=1
for k,(ok,f) in c.items(): print(f"{k}: approved {ok}, failed {f}")
