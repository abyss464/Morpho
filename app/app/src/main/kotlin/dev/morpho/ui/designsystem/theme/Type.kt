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
 * Typography layer (docs/contracts/app-design.md, "Brand").
 *
 * **EB Garamond** is the icon's own face — the "M" wordmark in
 * `ic_launcher_foreground.xml` is drawn from its outlines — so it carries every
 * headline, title and display number and makes the app read as one object with its
 * launcher icon. It also carries the reading styles, which were already serif.
 *
 * Small functional chrome (labels, body copy, IPA, spell boxes) stays sans: at 11-16sp
 * a text face loses to a UI face on legibility, and letter-by-letter spelling needs
 * unambiguous shapes.
 *
 * The bundled file is the variable roman (`wght` 400-800); Compose's resource `Font`
 * defaults `variationSettings` to the requested weight, so each declared weight is a
 * real instance rather than a synthetic bold.
 */
object MorphoFonts {

    /**
     * EB Garamond, the icon's face. Swapping the whole brand face is this one
     * declaration and nothing else.
     */
    val displayFontFamily: FontFamily = FontFamily(
        Font(R.font.eb_garamond, FontWeight.Normal),
        Font(R.font.eb_garamond, FontWeight.Medium),
        Font(R.font.eb_garamond, FontWeight.SemiBold),
        Font(R.font.eb_garamond, FontWeight.Bold),
    )

    val uiFontFamily: FontFamily = FontFamily.SansSerif

    /** Definitions and example sentences: the same serif as the display face. */
    val readingFontFamily: FontFamily = displayFontFamily
}

private val lineHeightStyle = LineHeightStyle(
    alignment = LineHeightStyle.Alignment.Center,
    trim = LineHeightStyle.Trim.None,
)

/**
 * M3 defaults re-cut: display / headline / title onto the Garamond brand face, body
 * and label onto the UI face. `displaySmall` stays reserved for the word headline.
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
 * Small caps-ish section label: the scholarly running head above each home section.
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
    /** Large numerals on stat tiles and the progress ring. Serif — a figure is display. */
    val statNumber: TextStyle = TextStyle(
        fontFamily = MorphoFonts.displayFontFamily,
        fontSize = 32.sp,
        lineHeight = 36.sp,
        fontWeight = FontWeight.SemiBold,
    ),
    /** The one headline figure of a screen: words learned so far, on home. */
    val heroNumber: TextStyle = TextStyle(
        fontFamily = MorphoFonts.displayFontFamily,
        fontSize = 52.sp,
        lineHeight = 56.sp,
        fontWeight = FontWeight.Medium,
    ),
)

/** Alpha the contract mandates for phonetics. */
const val PHONETIC_ALPHA = 0.7f
