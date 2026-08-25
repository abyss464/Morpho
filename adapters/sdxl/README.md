# morpho-sdxl-adapter

Serves `sdxl.generate` against a local ComfyUI instance: submit a versioned
txt2img graph, poll `/history`, download the render, convert to WebP at the
requested size.

An unreachable backend is reported as a **permanent** error so the word lands in
dead letters instead of retry-looping — SDXL is the last-resort image fallback
and a machine without ComfyUI will never grow one by waiting.

See `adapters/README.md` for env vars and the workflow template contract.
