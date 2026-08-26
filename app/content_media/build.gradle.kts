plugins {
    alias(libs.plugins.android.asset.pack)
}

/**
 * Word images and audio, delivered as an install-time Play Asset Delivery pack.
 *
 * Install-time is the only delivery mode that fits this product: the app is fully
 * offline and every word needs its picture and audio the moment it comes up, so there is
 * no useful "fetch later". The payoff over stuffing the media into the base module is
 * purely the 200 MB base-module cap — an install-time pack raises the ceiling without
 * changing how the app reads the files, because Android merges the pack's assets into
 * the normal `AssetManager` namespace.
 *
 * That is why the payload lives at `src/main/assets/content_media/{img,audio}/`: at
 * runtime it resolves to `content_media/img/{hash}.webp`, exactly the path
 * `AssetContentStore` already asks for. Dropping the exported media into that directory
 * needs no build change and no code change.
 */
assetPack {
    packName.set("content_media")
    dynamicDelivery {
        deliveryType.set("install-time")
    }
}
