---
file: adapters/sdxl/src/morpho_sdxl/__init__.py
---

Package entry point, exports two names:

- **OPS** — a mapping table from operation names to handler functions, for adapters to register and use when processing rows/lines.
- **OP_GENERATE** — the operation name constant `"sdxl.generate"`.
