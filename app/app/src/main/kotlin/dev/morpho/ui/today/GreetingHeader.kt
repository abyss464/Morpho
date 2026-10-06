package dev.morpho.ui.today

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import dev.morpho.R
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.ui.designsystem.component.MotifSignature
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.StreakBadge
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoSectionLabel
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The Today masthead: a tracked-out greeting, the wordmark in the icon's own Garamond,
 * and the icon's four marks sitting under it like a printer's device; the streak and the
 * way into settings on the right.
 */
@Composable
fun GreetingHeader(
    greetingPeriod: GreetingPeriod,
    streakDays: Int,
    onOpenSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val greetingText = when (greetingPeriod) {
        GreetingPeriod.MORNING -> stringResource(R.string.today_greeting_morning)
        GreetingPeriod.AFTERNOON -> stringResource(R.string.today_greeting_afternoon)
        GreetingPeriod.EVENING -> stringResource(R.string.today_greeting_evening)
    }

    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Top,
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs)) {
            Text(
                text = greetingText.uppercase(),
                style = MorphoSectionLabel,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = stringResource(R.string.app_name),
                style = MaterialTheme.typography.displaySmall,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Spacer(Modifier.height(MorphoTheme.spacing.xxs))
            MotifSignature()
        }

        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
        ) {
            if (streakDays > 0) {
                StreakBadge(
                    days = streakDays,
                    label = pluralStringResource(R.plurals.streak_days, streakDays),
                )
            }
            IconButton(onClick = onOpenSettings) {
                Icon(
                    Icons.Rounded.Settings,
                    contentDescription = stringResource(R.string.action_settings),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
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
