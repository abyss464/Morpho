package dev.morpho.data.content

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.util.Log
import java.io.File
import java.io.RandomAccessFile
import kotlin.math.PI
import kotlin.math.exp
import kotlin.math.sin

/**
 * Generates placeholder content media for the wave-1 demo.
 *
 * Real media arrives with `morphod export`; until then this writes, on first launch:
 *
 *  * **images** — a distinct two-stop gradient PNG per filename, colours derived from
 *    the same hash the design system's preview renderer uses, so a preview and the
 *    running app show the same colour for the same word.
 *  * **audio** — a short, quiet tone whose pitch and length are derived from the
 *    filename. Long enough to prove the Media3 path (prepare, play, preload the next
 *    question) without pretending to be speech.
 *
 * Everything is deterministic: the same filename always yields the same file, so a
 * reinstall reproduces the demo exactly.
 */
class DemoMediaGenerator(private val root: File) {

    fun ensure(imageFiles: Collection<String>, audioFiles: Collection<String>): Int {
        var written = 0
        imageFiles.forEach { if (writeImage(it)) written++ }
        audioFiles.forEach { if (writeTone(it)) written++ }
        if (written > 0) {
            Log.i(TAG, "generated $written placeholder media files under $root")
        }
        return written
    }

    private fun target(name: String): File? {
        val file = File(root, name.trim('/'))
        if (file.exists() && file.length() > 0) return null
        file.parentFile?.mkdirs()
        return file
    }

    private fun writeImage(name: String): Boolean {
        val file = target(name) ?: return false
        val (top, bottom) = gradientArgbFor(name)
        val bitmap = Bitmap.createBitmap(WIDTH, HEIGHT, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            shader = LinearGradient(
                0f, 0f, WIDTH.toFloat(), HEIGHT.toFloat(),
                top, bottom, Shader.TileMode.CLAMP,
            )
        }
        canvas.drawRect(0f, 0f, WIDTH.toFloat(), HEIGHT.toFloat(), paint)

        // A couple of soft translucent discs so the four cells of a grid are easy to
        // tell apart at a glance rather than reading as four flat rectangles.
        val hash = fnv1a(name)
        val disc = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = (0x33FFFFFF).toInt()
        }
        repeat(3) { i ->
            val cx = ((hash ushr (i * 9)) % WIDTH).toFloat()
            val cy = ((hash ushr (i * 7 + 3)) % HEIGHT).toFloat()
            canvas.drawCircle(cx, cy, (WIDTH / 6f) * (1f + i * 0.35f), disc)
        }

        file.outputStream().use { out ->
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
        }
        bitmap.recycle()
        return true
    }

    /** Writes a 16-bit mono PCM WAV. Media3 plays WAV natively, no codec needed. */
    private fun writeTone(name: String): Boolean {
        val file = target(name) ?: return false
        val hash = fnv1a(name)
        val baseFreq = 180.0 + (hash % 220L)
        val durationMs = 420L + (hash ushr 11) % 380L
        val frames = (SAMPLE_RATE * durationMs / 1000L).toInt()

        val pcm = ByteArray(frames * 2)
        for (n in 0 until frames) {
            val t = n.toDouble() / SAMPLE_RATE
            val progress = n.toDouble() / frames
            // Gentle attack, exponential decay, plus a second partial for a little body.
            val attack = (progress / 0.06).coerceAtMost(1.0)
            val env = attack * exp(-3.0 * progress)
            val s = 0.55 * sin(2 * PI * baseFreq * t) + 0.22 * sin(2 * PI * baseFreq * 2.01 * t)
            val value = (s * env * PEAK * Short.MAX_VALUE).toInt()
                .coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt())
            pcm[n * 2] = (value and 0xFF).toByte()
            pcm[n * 2 + 1] = ((value shr 8) and 0xFF).toByte()
        }

        RandomAccessFile(file, "rw").use { raf ->
            raf.setLength(0)
            raf.write(wavHeader(pcm.size))
            raf.write(pcm)
        }
        return true
    }

    private fun wavHeader(dataBytes: Int): ByteArray {
        val byteRate = SAMPLE_RATE * CHANNELS * BITS_PER_SAMPLE / 8
        val blockAlign = CHANNELS * BITS_PER_SAMPLE / 8
        val out = java.io.ByteArrayOutputStream(44)
        fun ascii(s: String) = out.write(s.toByteArray(Charsets.US_ASCII))
        fun le32(v: Int) {
            out.write(v and 0xFF); out.write((v shr 8) and 0xFF)
            out.write((v shr 16) and 0xFF); out.write((v shr 24) and 0xFF)
        }
        fun le16(v: Int) {
            out.write(v and 0xFF); out.write((v shr 8) and 0xFF)
        }
        ascii("RIFF"); le32(36 + dataBytes); ascii("WAVE")
        ascii("fmt "); le32(16); le16(1); le16(CHANNELS)
        le32(SAMPLE_RATE); le32(byteRate); le16(blockAlign); le16(BITS_PER_SAMPLE)
        ascii("data"); le32(dataBytes)
        return out.toByteArray()
    }

    companion object {
        private const val TAG = "DemoMediaGenerator"
        private const val WIDTH = 768
        private const val HEIGHT = 576
        private const val SAMPLE_RATE = 16_000
        private const val CHANNELS = 1
        private const val BITS_PER_SAMPLE = 16
        private const val PEAK = 0.22 // deliberately quiet: it is a placeholder, not content
    }
}

/** FNV-1a 64, shared with the design system so colours match between preview and app. */
internal fun fnv1a(key: String): Long {
    var hash = -0x340d631b7bdddcdbL
    for (ch in key) {
        hash = hash xor ch.code.toLong()
        hash *= 0x100000001B3L
    }
    return hash and Long.MAX_VALUE
}

/** Same hue maths as `gradientColorsFor`, expressed as packed ARGB ints. */
internal fun gradientArgbFor(key: String): Pair<Int, Int> {
    val hash = fnv1a(key)
    // Golden-angle hue stepping, identical to the design system's preview renderer.
    val hue = ((hash % 1_000L) * GOLDEN_ANGLE_DEGREES % 360.0).toFloat()
    val hue2 = (hue + 26f + (hash ushr 17) % 34L) % 360f
    val sat = 0.52f + ((hash ushr 29) % 26L) / 100f
    return hsvToArgb(hue, sat, 0.88f) to hsvToArgb(hue2, sat * 0.94f, 0.55f)
}

/** 360 / phi^2 - the classic low-discrepancy hue step. */
private const val GOLDEN_ANGLE_DEGREES = 137.50776405003785

private fun hsvToArgb(h: Float, s: Float, v: Float): Int {
    val c = v * s
    val x = c * (1 - kotlin.math.abs((h / 60f) % 2 - 1))
    val m = v - c
    val (r, g, b) = when {
        h < 60 -> Triple(c, x, 0f)
        h < 120 -> Triple(x, c, 0f)
        h < 180 -> Triple(0f, c, x)
        h < 240 -> Triple(0f, x, c)
        h < 300 -> Triple(x, 0f, c)
        else -> Triple(c, 0f, x)
    }
    fun ch(f: Float) = ((f + m) * 255f).toInt().coerceIn(0, 255)
    return (0xFF shl 24) or (ch(r) shl 16) or (ch(g) shl 8) or ch(b)
}
