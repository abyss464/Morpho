package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import dev.morpho.ui.designsystem.theme.MorphoFonts
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Consecutive-day pill: the count in Garamond on a copper tint, so it reads as a reward
 * rather than a metric. [label] follows the count ("days" on Today, "day streak" on the
 * done screen).
 */
@Composable
fun StreakBadge(
    days: Int,
    label: String,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .clip(MorphoTheme.radii.shapeFull)
            .background(MorphoTheme.accents.streak.copy(alpha = 0.14f))
            .padding(horizontal = MorphoTheme.spacing.sm, vertical = MorphoTheme.spacing.xxs)
            .clearAndSetSemantics { contentDescription = "$days $label" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
    ) {
        Text(
            text = "$days",
            style = MaterialTheme.typography.titleMedium.copy(
                fontFamily = MorphoFonts.displayFontFamily,
                fontWeight = FontWeight.Medium,
            ),
            color = MaterialTheme.colorScheme.onSurface,
        )
        Text(
            text = label,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@ThemePreviews
@Composable
private fun StreakBadgePreview() {
    PreviewBox {
        StreakBadge(days = 12, label = "days")
    }
}
