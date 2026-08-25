package dev.morpho.ui.designsystem.theme

import androidx.compose.material3.Typography
import androidx.compose.runtime.Immutable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.sp

/**
 * Typography layer (docs/contracts/app-design.md, "Brand").
 *
 * UI chrome is sans; **definitions and example sentences are serif** for the
 * dictionary feel and long-form readability. Bundled Inter + Literata land in a
 * later wave — swapping them in means changing [uiFontFamily] and
 * [readingFontFamily] here and nothing else.
 */
object MorphoFonts {
    val uiFontFamily: FontFamily = FontFamily.SansSerif
    val readingFontFamily: FontFamily = FontFamily.Serif
}

private val lineHeightStyle = LineHeightStyle(
    alignment = LineHeightStyle.Alignment.Center,
    trim = LineHeightStyle.Trim.None,
)

/** M3 defaults re-cut onto the UI family, with `displaySmall` reserved for the word headline. */
val MorphoTypography = Typography().run {
    val ui = MorphoFonts.uiFontFamily
    copy(
        displayLarge = displayLarge.copy(fontFamily = ui),
        displayMedium = displayMedium.copy(fontFamily = ui),
        displaySmall = displaySmall.copy(
            fontFamily = ui,
            fontWeight = FontWeight.SemiBold,
            letterSpacing = (-0.5).sp,
        ),
        headlineLarge = headlineLarge.copy(fontFamily = ui, fontWeight = FontWeight.SemiBold),
        headlineMedium = headlineMedium.copy(fontFamily = ui, fontWeight = FontWeight.SemiBold),
        headlineSmall = headlineSmall.copy(fontFamily = ui, fontWeight = FontWeight.SemiBold),
        titleLarge = titleLarge.copy(fontFamily = ui, fontWeight = FontWeight.SemiBold),
        titleMedium = titleMedium.copy(fontFamily = ui, fontWeight = FontWeight.Medium),
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
 * Reading styles: everything the user is meant to *read as English*, plus the
 * phonetic style (sans, alpha applied by the caller at 0.7 per the contract).
 */
@Immutable
data class MorphoReadingTypography(
    /** Definitions: bodyLarge at 1.5 line height, serif. */
    val definition: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 16.sp,
        lineHeight = 24.sp, // 1.5x
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** One-line definition caption under a mode-2 image cell. */
    val definitionCaption: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 13.sp,
        lineHeight = 18.sp,
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** Mode-3 option cards: a touch larger, still serif. */
    val definitionOption: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 17.sp,
        lineHeight = 26.sp,
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** Example sentences on the sentence card. */
    val sentence: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 22.sp,
        lineHeight = 34.sp,
        fontWeight = FontWeight.Normal,
        lineHeightStyle = lineHeightStyle,
    ),
    /** Example sentences in the detail sheet. */
    val sentenceCompact: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 17.sp,
        lineHeight = 27.sp,
        lineHeightStyle = lineHeightStyle,
    ),
    /** Etymology prose. */
    val etymology: TextStyle = TextStyle(
        fontFamily = MorphoFonts.readingFontFamily,
        fontSize = 15.sp,
        lineHeight = 23.sp,
        lineHeightStyle = lineHeightStyle,
    ),
    /** IPA — sans, never serif; render at alpha 0.7. */
    val phonetic: TextStyle = TextStyle(
        fontFamily = MorphoFonts.uiFontFamily,
        fontSize = 15.sp,
        lineHeight = 20.sp,
        fontWeight = FontWeight.Normal,
    ),
    /** The word itself on the header and detail sheet. */
    val wordHeadline: TextStyle = TextStyle(
        fontFamily = MorphoFonts.uiFontFamily,
        fontSize = 36.sp,
        lineHeight = 44.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = (-0.5).sp,
    ),
    /** Word rendered inside a quiz option cell. */
    val wordOption: TextStyle = TextStyle(
        fontFamily = MorphoFonts.uiFontFamily,
        fontSize = 22.sp,
        lineHeight = 28.sp,
        fontWeight = FontWeight.Medium,
    ),
    /** Letter boxes in the spell input. */
    val spellLetter: TextStyle = TextStyle(
        fontFamily = MorphoFonts.uiFontFamily,
        fontSize = 22.sp,
        lineHeight = 26.sp,
        fontWeight = FontWeight.SemiBold,
    ),
    /** Large numerals on stat tiles and the progress ring. */
    val statNumber: TextStyle = TextStyle(
        fontFamily = MorphoFonts.uiFontFamily,
        fontSize = 30.sp,
        lineHeight = 34.sp,
        fontWeight = FontWeight.Bold,
    ),
)

/** Alpha the contract mandates for phonetics. */
const val PHONETIC_ALPHA = 0.7f
