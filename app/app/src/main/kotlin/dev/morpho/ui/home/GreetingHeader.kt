package dev.morpho.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import dev.morpho.R
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.StreakBadge
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme

@Composable
fun GreetingHeader(
    greetingPeriod: GreetingPeriod,
    streakDays: Int,
    onOpenSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val greetingText = when (greetingPeriod) {
        GreetingPeriod.MORNING -> stringResource(R.string.home_greeting_morning)
        GreetingPeriod.AFTERNOON -> stringResource(R.string.home_greeting_afternoon)
        GreetingPeriod.EVENING -> stringResource(R.string.home_greeting_evening)
    }

    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Top,
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs)) {
            Text(
                text = greetingText,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = stringResource(R.string.home_title),
                style = MaterialTheme.typography.titleLarge,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }

        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
        ) {
            if (streakDays > 0) {
                StreakBadge(days = streakDays)
            }
            IconButton(onClick = onOpenSettings) {
                Icon(
                    Icons.Rounded.Settings,
                    contentDescription = stringResource(R.string.action_settings),
                )
            }
        }
    }
}

@ThemePreviews
@Composable
private fun GreetingHeaderPreview() {
    PreviewBox {
        GreetingHeader(
            greetingPeriod = GreetingPeriod.EVENING,
            streakDays = 12,
            onOpenSettings = {},
        )
    }
}
