---
file: admin-ui/src/features/words/DistractorsTab.tsx
---

# Distractors tab

Word detail tab showing the three bound distractors as cards. Each card displays the distractor lemma (linked), rank, core_ready status, blockers, and binding metadata. Alerts when any distractor is not core-ready (the parent word cannot ship until they are). Distractors are bound once and never recomputed automatically -- only a manual rebind changes them.
