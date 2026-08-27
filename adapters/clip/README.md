# adapters/clip — CLIP scoring sidecar

Scores Morpho's image candidates against the sentence their word's card shows.
The engine folds the answer into image selection; see
`docs/contracts/clip-service.md` for the wire contract and
`morpho_reconcile::sources::clip` for the client.

**This is not a subprocess adapter.** The other three speak the stdin/stdout
envelope and are spawned per job; this one is a long-running HTTP service,
because it needs a GPU the engine's container does not have and because loading
the model per job would cost more than the scoring does.

## Running it

Under a venv that already has a working accelerator build of torch plus
`open_clip` — on this machine, ComfyUI's:

```bash
PYTHONPATH=~/Code/learning/Morpho/adapters/clip/src \
~/Code/vendor/ComfyUI/.venv/bin/python -m morpho_clip \
  --media-root ~/Code/learning/Morpho/data/media \
  --host 0.0.0.0 --port 30013
```

`--host 0.0.0.0` only when morphod is in a container and reaches the host from
outside its network namespace; a native morphod wants the `127.0.0.1` default.

Then point the engine at it: `MORPHO_CLIP_URL=http://127.0.0.1:30013`, or
`http://host.docker.internal:30013` from the container (docker-compose maps the
name).

## Testing it

```bash
uv run --directory adapters/clip pytest
```

No model, no weights and no accelerator are needed: `Scorer` is a protocol and
the suite substitutes a stub for it. What the tests cover is everything that can
go wrong without one — request validation, hash resolution, missing files, reply
shape and status codes.
