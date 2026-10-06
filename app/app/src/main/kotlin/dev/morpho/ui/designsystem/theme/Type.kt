package dev.morpho.ui.designsystem.theme

import androidx.compose.material3.Typography
import androidx.compose.runtime.Immutable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.sp
import dev.morpho.R

/**
 * Typography layer (docs/contracts/stream.md §8, docs/contracts/app-design.md "Brand").
 *
 * Three faces, each with one job:
 *
 *  * **EB Garamond** is the icon's own face — the "M" wordmark in
 *    `ic_launcher_foreground.xml` is drawn from its outlines. It carries display text:
 *    the wordmark, screen titles, the headword, word options and figures.
 *  * **Source Serif 4** carries what the learner reads as English: definitions, example
 *    sentences and the definition pieces of the explain step. It is a text serif drawn
 *    for body sizes, so it stays legible where Garamond turns thin.
 *  * The **system sans** carries interface text: labels, buttons, counts, IPA.
 *
 * EB Garamond is bundled as a variable font on the `wght` axis. Source Serif 4 is bundled
 * as two static instances, regular (400) and semibold (600), so its weight holds on every
 * platform version whether or not a variation setting reaches the typeface.
 */
object MorphoFonts {

    /** EB Garamond, the icon's face: display text only. */
    val displayFontFamily: FontFamily = FontFamily(
        Font(R.font.eb_garamond, FontWeight.Normal),
        Font(R.font.eb_garamond, FontWeight.Medium),
        Font(R.font.eb_garamond, FontWeight.SemiBold),
        Font(R.font.eb_garamond, FontWeight.Bold),
    )

    /** Source Serif 4: definitions and sentences. */
    val readingFontFamily: FontFamily = FontFamily(
        Font(R.font.source_serif_4_regular, FontWeight.Normal),
        Font(R.font.source_serif_4_semibold, FontWeight.SemiBold),
    )

    val uiFontFamily: FontFamily = FontFamily.SansSerif
}

private val lineHeightStyle = LineHeightStyle(
    alignment = LineHeightStyle.Alignment.Center,
    trim = LineHeightStyle.Trim.None,
)

/**
 * M3 defaults re-cut: display / headline / title onto the Garamond brand face, body
 * and label onto the UI face. `displaySmall` sets the wordmark on Today.
 *
 * Garamond's x-height is small, so the serif tiers carry a touch more size and line
 * height than the M3 default and drop the tightened tracking that a sans needs.
 */
val MorphoTypography = Typography().run {
    val ui = MorphoFonts.uiFontFamily
    val brand = MorphoFonts.displayFontFamily
    copy(
        displayLarge = displayLarge.copy(fontFamily = brand, letterSpacing = 0.sp),
        displayMedium = displayMedium.copy(fontFamily = brand, letterSpacing = 0.sp),
        displaySmall = displaySmall.copy(
            fontFamily = brand,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        ),
        headlineLarge = headlineLarge.copy(
            fontFamily = brand,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        ),
        headlineMedium = headlineMedium.copy(
            fontFamily = brand,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        ),
        headlineSmall = headlineSmall.copy(
            fontFamily = brand,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        ),
        titleLarge = titleLarge.copy(
            fontFamily = brand,
            fontSize = 24.sp,
            lineHeight = 30.sp,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        ),
        titleMedium = titleMedium.copy(
            fontFamily = brand,
            fontSize = 18.sp,
            lineHeight = 24.sp,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        ),
        titleSmall = titleSmall.copy(fontFamily = ui, fontWeight = FontWeight.Medium),
        bodyLarge = bodyLarge.copy(fontFamily = ui),
        bodyMedium = bodyMedium.copy(fontFamily = ui),
        bodySmall = bodySmall.copy(fontFamily = ui),
        labelLarge = labelLarge.copy(fontFamily = ui, fontWeight = FontWeight.Medium),
        labelMedium = labelMedium.copy(fontFamily = ui, fontWeight = FontWeight.Medium),
        labelSmall = labelSmall.copy(fontFamily = ui, fontWeight = FontWeight.Medium),
    )
}

/**
 * Small caps-ish section label: the running head above each Today section and the step
 * label in the stream.
 * Sans, tracked out, uppercase applied by the caller.
 */
val MorphoSectionLabel = TextStyle(
    fontFamily = MorphoFonts.uiFontFamily,
    fontSize = 11.sp,
    lineHeight = 16.sp,
    fontWeight = FontWeight.Medium,
    letterSpacing = 1.6.sp,
)

/**
 * Reading styles: what the learner reads as English in Source Serif 4, the headword and
 * word options in Garamond, and the phonetic in sans (alpha applied by the caller).
 * Sizes follow the phone design boards of the stream.
 */
@Immutable
data class MorphoReadingTypography(
    /** The definition on a word card. */
    val definition: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 19.sp,
        lineHeight = 28.sp,
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** A definition set smaller: the review result, a wrong option's meaning. */
    val definitionCompact: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 17.sp,
        lineHeight = 25.sp,
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** The example sentence under a definition. */
    val sentence: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 16.sp,
        lineHeight = 25.sp,
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** The example sentence with its gap, on the use step. */
    val gapSentence: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 24.sp,
        lineHeight = 36.sp,
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** One definition piece in the explain step's tray and bank. */
    val piece: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 17.sp,
        lineHeight = 22.sp,
        fontWeight = FontWeight.Normal,
    ),
    /** IPA — sans, never serif; render at alpha 0.7. */
    val phonetic: TextStyle = TextStyle(
        fontFamily = MorphoFonts.uiFontFamily,
        fontSize = 15.sp,
        lineHeight = 20.sp,
        fontWeight = FontWeight.Normal,
    ),
    /** The headword on a word card. */
    val wordHeadline: TextStyle = TextStyle(
        fontFamily = MorphoFonts.displayFontFamily,
        fontSize = 40.sp,
        lineHeight = 44.sp,
        fontWeight = FontWeight.Medium,
    ),
    /** A step's question: "What does … mean?", "Which word fits?". */
    val prompt: TextStyle = TextStyle(
        fontFamily = MorphoFonts.displayFontFamily,
        fontSize = 28.sp,
        lineHeight = 32.sp,
        fontWeight = FontWeight.Medium,
    ),
    /** A word as a fill-in option. */
    val wordOption: TextStyle = TextStyle(
        fontFamily = MorphoFonts.displayFontFamily,
        fontSize = 22.sp,
        lineHeight = 28.sp,
        fontWeight = FontWeight.Medium,
    ),
    /** Figures on the done screen's tiles. Serif — a figure is display. */
    val statNumber: TextStyle = TextStyle(
        fontFamily = MorphoFonts.displayFontFamily,
        fontSize = 32.sp,
        lineHeight = 36.sp,
        fontWeight = FontWeight.SemiBold,
    ),
    /** The one headline figure of a screen: words met so far, on Today. */
    val heroNumber: TextStyle = TextStyle(
        fontFamily = MorphoFonts.displayFontFamily,
        fontSize = 52.sp,
        lineHeight = 56.sp,
        fontWeight = FontWeight.Medium,
    ),
)

/** Alpha the contract mandates for phonetics. */
const val PHONETIC_ALPHA = 0.7f
