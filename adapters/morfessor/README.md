# morpho-morfessor-adapter

Serves `morfessor.segment` — batch morphological segmentation, the fallback
etymology source when Wiktionary has nothing (README part 4).

Wave 1 ships a **dev stopgap**: with no pretrained model on disk the adapter
trains a throwaway Baseline model from the requested batch itself. See
`adapters/README.md` for the model-file layout and the plan for real corpus
training.
