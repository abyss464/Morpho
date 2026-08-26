# `content_media` asset pack

Install-time Play Asset Delivery pack holding every image and audio file `release.db`
references. As of wave 3b it carries the real export: 4,011 images and 21,600 audio
clips, ~470 MB, content-addressed.

## Dropping the media in

The payload is **not in git** (see `../.gitignore`) — it is a derived artifact and
`data/releases/<export>/` is the source of truth. After a fresh clone, or after a new
export lands, repopulate it:

```sh
export=data/releases/export-20260826T080617034Z          # whichever release you build against
rsync -a "$export"/img "$export"/audio app/content_media/src/main/assets/content_media/
cp "$export"/release.db app/app/src/main/assets/release.db
```

The tree must end up looking like this:

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

`release.db` and the media must come from the same export. They agree by content hash:
every filename in the database is the SHA-256 of the bytes it names, so a mismatched pair
does not silently render the wrong picture — it renders nothing, and
`AssetContentStore.exists` says so.

## Flavours

* **pad** — this module is listed in `android.assetPacks`, so `bundlePadRelease`
  produces an AAB with the pack alongside the base module. `assemblePadDebug` produces a
  small APK with **no** media, because asset packs are a bundle-level concept: useful for
  iterating on UI, useless for playing.
* **fatApk** — the `fatApk` source set points its `assets.srcDirs` at this same
  directory, so the media is packed straight into the APK. One copy of the media on
  disk, both distributions fed from it. This is the variant to install on a device.

## Why the assets stay uncompressed

`android.androidResources.noCompress` covers `webp`, `ogg` and `db`. Deflating an
already-compressed codec buys nothing and costs the zero-copy path: Media3 plays an
`AssetFileDescriptor` straight out of the APK only when the entry is stored, and
`release.db` is copied out byte-for-byte on first launch rather than inflated.
