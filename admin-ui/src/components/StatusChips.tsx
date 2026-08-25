import { Badge, Space, Tag, Tooltip, Typography } from 'antd';
import {
  CheckCircleFilled,
  CloseCircleFilled,
  ExclamationCircleFilled,
  MinusCircleOutlined,
  PushpinFilled,
  StarFilled,
} from '@ant-design/icons';
import type { BlockerCode, CandidateSource, WordListItem, WordRole } from '../api/types';

/* ------------------------------------------------------------------ */
/* Readiness                                                           */
/* ------------------------------------------------------------------ */

export function ReadyBadge({ ready }: { ready: boolean }) {
  return ready ? (
    <Tag color="success" icon={<CheckCircleFilled />} style={{ margin: 0 }}>
      ready
    </Tag>
  ) : (
    <Tag color="warning" icon={<ExclamationCircleFilled />} style={{ margin: 0 }}>
      blocked
    </Tag>
  );
}

const BLOCKER_HELP: Record<string, string> = {
  missing_primary_sense: 'No sense is flagged is_primary; every question type needs one.',
  sense_not_approved: 'An enabled sense selection is still unapproved.',
  missing_definition: 'No definition candidate is selected for this word.',
  oos_pending:
    'The selected definition contains a token that is neither base, target nor auxiliary.',
  dependency_not_ready: 'A word this definition depends on is not itself ready.',
  missing_example: 'Slot 1 (the mode-1 sentence) has no selection.',
  example_not_approved: 'Slot 1 is selected but has not been approved.',
  missing_image: 'No live image is selected; this word cannot appear in a four-image question.',
  image_not_approved: 'The live image is selected but not approved.',
  tts_missing: 'One or more desired TTS texts have no synthesized audio.',
  tts_failed: 'A TTS synthesis for this word is in the failed state.',
  distractors_unbound: 'Fewer than three distractors are bound to this word.',
  distractor_1_not_ready: 'Bound distractor 1 is not core-ready, so its assets would be missing.',
  distractor_2_not_ready: 'Bound distractor 2 is not core-ready, so its assets would be missing.',
  distractor_3_not_ready: 'Bound distractor 3 is not core-ready, so its assets would be missing.',
  not_in_plan: 'The word has no learning_order in the current plan artifact.',
};

const BLOCKER_COLOR = (code: string): string => {
  if (code.startsWith('distractor')) return 'geekblue';
  if (code.startsWith('tts')) return 'volcano';
  if (code.includes('image')) return 'magenta';
  if (code.includes('oos')) return 'red';
  if (code.includes('example')) return 'gold';
  if (code.includes('sense') || code.includes('definition')) return 'orange';
  return 'default';
};

export function BlockerTags({ blockers, max = 3 }: { blockers: BlockerCode[]; max?: number }) {
  if (blockers.length === 0) {
    return <Typography.Text type="secondary">—</Typography.Text>;
  }
  const shown = blockers.slice(0, max);
  const rest = blockers.slice(max);
  return (
    <Space size={[4, 4]} wrap>
      {shown.map((code) => (
        <Tooltip key={code} title={BLOCKER_HELP[code] ?? code}>
          <Tag color={BLOCKER_COLOR(code)} style={{ margin: 0 }}>
            {code}
          </Tag>
        </Tooltip>
      ))}
      {rest.length > 0 && (
        <Tooltip title={rest.join(', ')}>
          <Tag style={{ margin: 0 }}>+{rest.length}</Tag>
        </Tooltip>
      )}
    </Space>
  );
}

/* ------------------------------------------------------------------ */
/* Per-asset chips                                                     */
/* ------------------------------------------------------------------ */

export type AssetChipStatus = 'ready' | 'partial' | 'missing' | 'failed';

const CHIP_STYLE: Record<AssetChipStatus, { color: string; icon: React.ReactNode }> = {
  ready: { color: 'success', icon: <CheckCircleFilled /> },
  partial: { color: 'warning', icon: <ExclamationCircleFilled /> },
  missing: { color: 'default', icon: <MinusCircleOutlined /> },
  failed: { color: 'error', icon: <CloseCircleFilled /> },
};

export function AssetChip({
  label,
  status,
  title,
}: {
  label: string;
  status: AssetChipStatus;
  title: string;
}) {
  const style = CHIP_STYLE[status];
  return (
    <Tooltip title={title}>
      <Tag color={style.color} icon={style.icon} style={{ margin: 0 }}>
        {label}
      </Tag>
    </Tooltip>
  );
}

/** Rolls a word-list row up into the four asset chips shown in the table. */
export function WordAssetChips({ word }: { word: WordListItem }) {
  const has = (code: string) => word.blockers.includes(code);

  const defStatus: AssetChipStatus = has('missing_definition')
    ? 'missing'
    : has('sense_not_approved') || has('missing_primary_sense') || has('oos_pending')
      ? 'partial'
      : 'ready';

  const exStatus: AssetChipStatus = has('missing_example')
    ? 'missing'
    : has('example_not_approved')
      ? 'partial'
      : 'ready';

  const imgStatus: AssetChipStatus = !word.has_image
    ? 'missing'
    : has('image_not_approved')
      ? 'partial'
      : 'ready';

  const ttsStatus: AssetChipStatus = has('tts_failed')
    ? 'failed'
    : word.tts_missing > 0
      ? 'missing'
      : 'ready';

  return (
    <Space size={[4, 4]} wrap>
      <AssetChip
        label={`def ${word.sense_count}`}
        status={defStatus}
        title={`${word.sense_count} enabled sense(s) — ${defStatus}`}
      />
      <AssetChip
        label={`ex ${word.example_count}`}
        status={exStatus}
        title={`${word.example_count} example slot(s) filled — ${exStatus}`}
      />
      <AssetChip label="img" status={imgStatus} title={`Live image — ${imgStatus}`} />
      <AssetChip
        label={word.tts_missing > 0 ? `tts −${word.tts_missing}` : 'tts'}
        status={ttsStatus}
        title={
          word.tts_missing > 0
            ? `${word.tts_missing} desired TTS text(s) not ready`
            : 'All desired TTS texts are ready'
        }
      />
    </Space>
  );
}

/* ------------------------------------------------------------------ */
/* Candidate metadata                                                  */
/* ------------------------------------------------------------------ */

const SOURCE_COLOR: Record<string, string> = {
  manual: 'purple',
  llm_rewrite: 'geekblue',
  llm: 'geekblue',
  freedict: 'blue',
  wordnet: 'cyan',
  exam_corpus: 'green',
  unsplash: 'blue',
  pexels: 'cyan',
  pixabay: 'green',
  sdxl: 'magenta',
};

export function SourceBadge({ source }: { source: CandidateSource | string }) {
  return (
    <Tag color={SOURCE_COLOR[source] ?? 'default'} style={{ margin: 0 }}>
      {source}
    </Tag>
  );
}

export function ScoreBadge({
  score,
  detail,
}: {
  score: number | null;
  detail?: Record<string, number> | null;
}) {
  if (score === null) return <Typography.Text type="secondary">unscored</Typography.Text>;
  const tone = score >= 0.85 ? 'success' : score >= 0.7 ? 'processing' : 'default';
  const tip = detail
    ? Object.entries(detail)
        .map(([key, value]) => `${key}: ${value}`)
        .join('\n')
    : 'Auto score from the current scorer version';
  return (
    <Tooltip title={<span style={{ whiteSpace: 'pre-line' }}>{tip}</span>}>
      <Badge status={tone as 'success' | 'processing' | 'default'} text={score.toFixed(2)} />
    </Tooltip>
  );
}

export function RoleTag({ role, auxStatus }: { role: WordRole; auxStatus?: string | null }) {
  const color = role === 'target' ? 'blue' : role === 'auxiliary' ? 'purple' : 'default';
  return (
    <Tag color={color} style={{ margin: 0 }}>
      {role}
      {role === 'auxiliary' && auxStatus ? ` · ${auxStatus}` : ''}
    </Tag>
  );
}

export function PinnedMark({ pinned }: { pinned: boolean }) {
  if (!pinned) return null;
  return (
    <Tooltip title="Pinned — automatic selection will never touch this slot.">
      <PushpinFilled style={{ color: '#d99328' }} />
    </Tooltip>
  );
}

export function PrimaryMark({ isPrimary }: { isPrimary: boolean }) {
  if (!isPrimary) return null;
  return (
    <Tooltip title="Primary sense — drives every question type and the review prompt.">
      <Tag color="gold" icon={<StarFilled />} style={{ margin: 0 }}>
        primary
      </Tag>
    </Tooltip>
  );
}

export function ApprovalTag({ approved }: { approved: boolean }) {
  return approved ? (
    <Tag color="success" style={{ margin: 0 }}>
      approved
    </Tag>
  ) : (
    <Tag color="warning" style={{ margin: 0 }}>
      unapproved
    </Tag>
  );
}
