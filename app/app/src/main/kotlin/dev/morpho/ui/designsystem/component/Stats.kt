package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.LocalFireDepartment
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** Consecutive-day badge. Warm accent so it reads as a reward, not a metric. */
@Composable
fun StreakBadge(
    days: Int,
    modifier: Modifier = Modifier,
) {
    val accents = MorphoTheme.accents
    Row(
        modifier = modifier
            .clip(MorphoTheme.radii.shapeFull)
            .background(accents.streak.copy(alpha = 0.16f))
            .padding(
                horizontal = MorphoTheme.spacing.sm,
                vertical = MorphoTheme.spacing.xs,
            )
            .clearAndSetSemantics {
                contentDescription = if (days == 1) "1 day streak" else "$days day streak"
            },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
    ) {
        Icon(
            imageVector = Icons.Rounded.LocalFireDepartment,
            contentDescription = null,
            tint = accents.streak,
            modifier = Modifier.size(18.dp),
        )
        Text(
            text = "$days",
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Text(
            text = if (days == 1) "day" else "days",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** A single headline number with a caption. Used on home and the session summary. */
@Composable
fun StatTile(
    value: String,
    label: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    emphasis: Boolean = false,
) {
    ElevatedCard(
        modifier = modifier.clearAndSetSemantics { contentDescription = "$label: $value" },
        shape = MorphoTheme.radii.shapeMd,
        colors = CardDefaults.elevatedCardColors(
            containerColor = if (emphasis) {
                MaterialTheme.colorScheme.primaryContainer
            } else {
                MaterialTheme.colorScheme.surfaceContainerLow
            },
        ),
        elevation = CardDefaults.elevatedCardElevation(
            defaultElevation = MorphoTheme.elevations.raised,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(MorphoTheme.spacing.md),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
        ) {
            if (icon != null) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = if (emphasis) {
                        MaterialTheme.colorScheme.onPrimaryContainer
                    } else {
                        MaterialTheme.colorScheme.primary
                    },
                    modifier = Modifier.size(20.dp),
                )
            }
            Text(
                text = value,
                style = MorphoTheme.reading.statNumber,
                color = if (emphasis) {
                    MaterialTheme.colorScheme.onPrimaryContainer
                } else {
                    MaterialTheme.colorScheme.onSurface
                },
            )
            Text(
                text = label,
                style = MaterialTheme.typography.bodySmall,
                color = if (emphasis) {
                    MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f)
                } else {
                    MaterialTheme.colorScheme.onSurfaceVariant
                },
                textAlign = TextAlign.Center,
            )
        }
    }
}

/** Result card shown at the end of a learning or review session. */
@Composable
fun SessionSummaryCard(
    headline: String,
    supporting: String,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit = {},
) {
    ElevatedCard(
        modifier = modifier.fillMaxWidth(),
        shape = MorphoTheme.radii.shapeLg,
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
        elevation = CardDefaults.elevatedCardElevation(
            defaultElevation = MorphoTheme.elevations.card,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(MorphoTheme.spacing.xl),
            verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm),
        ) {
            Text(
                text = headline,
                style = MaterialTheme.typography.headlineSmall,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                text = supporting,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Box(Modifier.padding(top = MorphoTheme.spacing.xs)) { content() }
        }
    }
}

/** Small circular icon badge used in list rows and headers. */
@Composable
fun IconPill(icon: ImageVector, contentDescription: String?, modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(MorphoTheme.sizes.streakBadge)
            .background(MaterialTheme.colorScheme.primaryContainer, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = contentDescription,
            tint = MaterialTheme.colorScheme.onPrimaryContainer,
            modifier = Modifier.size(20.dp),
        )
    }
}

@ThemePreviews
@Composable
private fun StatsPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.md)) {
            StreakBadge(days = 12)
            StreakBadge(days = 1)
            Row(horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm)) {
                StatTile(value = "18", label = "new words", modifier = Modifier.weight(1f))
                StatTile(
                    value = "94%",
                    label = "accuracy",
                    modifier = Modifier.weight(1f),
                    emphasis = true,
                )
                StatTile(value = "26", label = "reviewed", modifier = Modifier.weight(1f))
            }
            SessionSummaryCard(
                headline = "Group cleared",
                supporting = "18 words, three rounds, 94% first-try accuracy.",
            )
        }
    }
}
