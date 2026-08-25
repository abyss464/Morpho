import { Empty, Space, Tag, Timeline, Tooltip, Typography } from 'antd';
import { formatTimestamp, relativeTime } from '../lib/format';
import type { AdminEvent } from '../api/types';

const ACTION_COLOR: Record<string, string> = {
  approved: 'green',
  approval_invalidated: 'orange',
  candidate_added: 'blue',
  candidate_rejected: 'red',
  selection_changed: 'blue',
  primary_moved: 'gold',
  pin_fallback: 'orange',
  aux_promoted: 'purple',
  aux_retired: 'gray',
  plan_rebuilt: 'cyan',
  distractor_bound: 'geekblue',
  job_dead: 'red',
  job_waived: 'orange',
  job_retry_requested: 'blue',
  release_exported: 'green',
  oos_resolved: 'green',
  oos_auto_closed: 'gray',
  word_created: 'purple',
  sense_enabled_changed: 'gold',
};

function actorTone(actor: string): string {
  if (actor.startsWith('admin:')) return 'purple';
  if (actor.startsWith('worker:')) return 'blue';
  return 'default';
}

function detailSummary(detail: Record<string, unknown> | null): string | null {
  if (!detail) return null;
  const parts = Object.entries(detail)
    .slice(0, 4)
    .map(([key, value]) =>
      typeof value === 'object' && value !== null
        ? `${key}=${JSON.stringify(value)}`
        : `${key}=${String(value)}`,
    );
  return parts.length > 0 ? parts.join('  ·  ') : null;
}

export function EventTimeline({ events, emptyText }: { events: AdminEvent[]; emptyText?: string }) {
  if (events.length === 0) {
    return (
      <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={emptyText ?? 'No events yet.'} />
    );
  }
  return (
    <Timeline
      items={events.map((event) => ({
        color: ACTION_COLOR[event.action] ?? 'gray',
        children: (
          <Space direction="vertical" size={2} style={{ width: '100%' }}>
            <Space size={6} wrap>
              <Tag color={ACTION_COLOR[event.action] ?? 'default'} style={{ margin: 0 }}>
                {event.action}
              </Tag>
              <Tag color={actorTone(event.actor)} style={{ margin: 0 }}>
                {event.actor}
              </Tag>
              <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                {event.entity_type} #{event.entity_id}
              </Typography.Text>
              <Tooltip title={formatTimestamp(event.ts)}>
                <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                  {relativeTime(event.ts)}
                </Typography.Text>
              </Tooltip>
            </Space>
            {detailSummary(event.detail) && (
              <Typography.Text type="secondary" className="morpho-mono">
                {detailSummary(event.detail)}
              </Typography.Text>
            )}
          </Space>
        ),
      }))}
    />
  );
}
