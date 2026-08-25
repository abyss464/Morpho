package dev.morpho.data.haptics

import android.content.Context
import android.os.Build
import android.os.CombinedVibration
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/** Haptic patterns from docs/contracts/app-design.md, "Haptics map". */
enum class HapticPattern {
    /** Light tick on a correct answer. */
    CORRECT,

    /** Double buzz, 2 x 40 ms, on a wrong answer. */
    WRONG,

    /** Tick-tick on a mode promotion. */
    PROMOTE,

    /** Success pattern when a group is cleared. */
    GROUP_COMPLETE,

    /** The lightest possible confirmation for an ordinary tap. */
    TAP,
}

/**
 * Vibration feedback.
 *
 * Every pattern is **always paired with a visual signal** — haptics are never the only
 * channel carrying information, per the contract. A single global toggle in settings
 * silences the whole layer; nothing here decides on its own whether the user wants it.
 */
class HapticsManager(context: Context) {

    private val vibrator: Vibrator? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        (context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager)
            ?.defaultVibrator
    } else {
        @Suppress("DEPRECATION")
        context.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
    }

    private val vibratorManager: VibratorManager? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager
        } else {
            null
        }

    @Volatile
    var enabled: Boolean = true

    val hasVibrator: Boolean get() = vibrator?.hasVibrator() == true

    fun perform(pattern: HapticPattern) {
        if (!enabled || !hasVibrator) return
        val effect = effectFor(pattern) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && vibratorManager != null) {
            vibratorManager.vibrate(CombinedVibration.createParallel(effect))
        } else {
            vibrator?.vibrate(effect)
        }
    }

    private fun effectFor(pattern: HapticPattern): VibrationEffect? = when (pattern) {
        HapticPattern.TAP -> predefinedOr(VibrationEffect.EFFECT_TICK) {
            VibrationEffect.createOneShot(8, 40)
        }

        HapticPattern.CORRECT -> predefinedOr(VibrationEffect.EFFECT_TICK) {
            VibrationEffect.createOneShot(15, 90)
        }

        // Two short buzzes, 40 ms each, with a 60 ms gap.
        HapticPattern.WRONG -> VibrationEffect.createWaveform(
            longArrayOf(0, 40, 60, 40),
            intArrayOf(0, 160, 0, 160),
            -1,
        )

        // Tick-tick: two very light taps, rising.
        HapticPattern.PROMOTE -> VibrationEffect.createWaveform(
            longArrayOf(0, 14, 70, 18),
            intArrayOf(0, 110, 0, 170),
            -1,
        )

        // Success: three accelerating pulses.
        HapticPattern.GROUP_COMPLETE -> VibrationEffect.createWaveform(
            longArrayOf(0, 22, 70, 22, 55, 42),
            intArrayOf(0, 130, 0, 180, 0, 255),
            -1,
        )
    }

    /**
     * Predefined effects are tuned per device and feel far better than a raw duration,
     * but only exist from API 29. Below that, fall back to a hand-rolled one-shot.
     */
    private inline fun predefinedOr(id: Int, fallback: () -> VibrationEffect): VibrationEffect =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            VibrationEffect.createPredefined(id)
        } else {
            fallback()
        }
}
