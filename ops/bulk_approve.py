import json, os, sqlite3, urllib.request, urllib.error
# MORPHO_API overrides the base URL (dockerised engine publishes 8787 on 30012).
API=os.environ.get("MORPHO_API","http://127.0.0.1:8787").rstrip("/")+"/api"
DB=os.environ.get("MORPHO_DB", os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "working.db"))
def api(path,body):
    req=urllib.request.Request(API+path,data=json.dumps(body).encode(),
        headers={"Content-Type":"application/json","X-Morpho-User":"operator-approve"},method="POST")
    try:
        urllib.request.urlopen(req,timeout=30); return True, None
    except urllib.error.HTTPError as e:
        return False, e.code
    except urllib.error.URLError as e:
        return False, str(e.reason)
conn=sqlite3.connect(f"file:{DB}?mode=ro",uri=True)
active="SELECT word_id FROM words WHERE role='target' OR (role='auxiliary' AND aux_status='active')"
c={"definition":[0,0],"example":[0,0],"image":[0,0]}
c409={"definition":0,"example":0,"image":0}
for wid,pos in conn.execute(f"SELECT word_id,pos FROM definition_selections WHERE enabled=1 AND approved=0 AND word_id IN ({active})"):
    ok,err=api("/selections/definition/approve",{"word_id":wid,"pos":pos})
    c["definition"][0 if ok else 1]+=1
    if not ok:
        if err==409: c409["definition"]+=1
        else: print(f"  definition {wid}/{pos}: {err}")
for wid,slot in conn.execute(f"SELECT word_id,slot FROM example_selections WHERE approved=0 AND word_id IN ({active})"):
    ok,err=api("/selections/example/approve",{"word_id":wid,"slot":slot})
    c["example"][0 if ok else 1]+=1
    if not ok:
        if err==409: c409["example"]+=1
        else: print(f"  example {wid}/{slot}: {err}")
for (wid,) in conn.execute(f"SELECT word_id FROM image_selections WHERE approved=0 AND word_id IN ({active})"):
    ok,err=api("/selections/image/approve",{"word_id":wid})
    c["image"][0 if ok else 1]+=1
    if not ok:
        if err==409: c409["image"]+=1
        else: print(f"  image {wid}: {err}")
# 409 = engine refused because the slot's current candidate is not 'available'
# (e.g. a stale auto-selection outrun by the reconciler). Expected, not fatal --
# every row is still attempted; only genuinely unexpected errors print above.
for k,(ok,f) in c.items(): print(f"{k}: approved {ok}, failed {f} (of which 409={c409[k]})")
