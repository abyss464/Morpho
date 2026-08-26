# `content_media` asset pack

Install-time Play Asset Delivery pack holding every image and audio file `release.db`
references. Empty scaffolding until wave 3b.

## Dropping the media in

Unpack the exported media bundle so the tree looks like this:

```
content_media/src/main/assets/content_media/
    img/{hash}.webp
    audio/{hash}.ogg
```

That is it — no Gradle edit, no code edit. The layout is fixed by two things that must
agree:

* `docs/contracts/release-db.sql` stores media as `img/{hash}.webp` and
  `audio/{hash}.ogg` (normative as of the wave-3 rulings).
* `AssetContentStore` is constructed with `root = "content_media"`, so it opens
  `content_media/<name>` through `AssetManager`.

An install-time pack's assets are merged into the app's ordinary asset namespace at
install, so the path inside `src/main/assets/` *is* the runtime path. Hence the doubled
`content_media/content_media` — the outer one is the source set, the inner one is the
namespace prefix the app asks for.

## Flavours

* **pad** — this module is listed in `android.assetPacks`, so `bundlePadRelease`
  produces an AAB with the pack alongside the base module.
* **fatApk** — asset packs are a bundle-level concept and are not packaged into an APK,
  so the `fatApk` source set instead points its `assets.srcDirs` at this same directory.
  One copy of the media on disk, both distributions fed from it.

## Keeping the directory in git

`assets/content_media/PLACEHOLDER.txt` exists only so the empty directory survives a
clone. Delete it once real media lands.
