import { Alert, Card, Empty, Space, Table, Tag, Tooltip, Typography } from 'antd';
import { ExclamationCircleFilled } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import { AudioButton } from '../../components/AudioButton';
import type { TtsStatusView, WordDetail } from '../../api/types';

const STATUS_COLOR: Record<TtsStatusView['status'], string> = {
  ready: 'success',
  missing: 'default',
  failed: 'error',
};

function refLabel(view: TtsStatusView): string {
  if (view.kind === 'word') return 'lemma';
  if (view.ref?.pos) return `sense · ${view.ref.pos}`;
  if (view.ref?.slot) return `example · slot ${view.ref.slot}`;
  return view.kind;
}

export function AudioTab({ detail }: { detail: WordDetail }) {
  const failed = detail.tts.filter((view) => view.status === 'failed');
  /*
   * Ruling #6 keys `missing` purely on the absence of a `tts_assets` row for the
   * current voice/params, so a synthesis that ran and blew up reports `missing`
   * too — while the word itself carries a `tts_failed` blocker. `last_error` is
   * what separates the two, and saying "the engine will pick it up next pass"
   * about a dead-lettered job would be a lie.
   */
  const errored = detail.tts.filter(
    (view) => view.status === 'missing' && view.last_error !== null,
  );
  const pending = detail.tts.filter(
    (view) => view.status === 'missing' && view.last_error === null,
  );
  const broken = [...failed, ...errored];

  const columns: ColumnsType<TtsStatusView> = [
    {
      title: '',
      key: 'play',
      width: 56,
      render: (_, view) => (
        <AudioButton
          fileHash={view.file_hash}
          disabled={view.status !== 'ready'}
          title={`Play ${refLabel(view)}`}
        />
      ),
    },
    {
      title: 'Belongs to',
      key: 'ref',
      width: 150,
      render: (_, view) => <Tag style={{ margin: 0 }}>{refLabel(view)}</Tag>,
    },
    {
      title: 'Synthesized text',
      dataIndex: 'text',
      key: 'text',
      render: (text: string) => <Typography.Text>{text}</Typography.Text>,
    },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      width: 110,
      render: (status: TtsStatusView['status'], view) => (
        <Tooltip title={view.last_error ?? undefined}>
          <Space size={4}>
            <Tag color={STATUS_COLOR[status]} style={{ margin: 0 }}>
              {status}
            </Tag>
            {status !== 'failed' && view.last_error !== null && (
              <ExclamationCircleFilled
                style={{ color: '#cf3d3d' }}
                aria-label="last attempt errored"
              />
            )}
          </Space>
        </Tooltip>
      ),
    },
    {
      title: 'Duration',
      dataIndex: 'duration_ms',
      key: 'duration_ms',
      width: 100,
      align: 'right',
      render: (ms: number | null) => (
        <Typography.Text type="secondary" className="morpho-mono">
          {ms === null ? '—' : `${(ms / 1000).toFixed(2)}s`}
        </Typography.Text>
      ),
    },
    {
      title: 'input_hash',
      dataIndex: 'input_hash',
      key: 'input_hash',
      width: 160,
      render: (hash: string, view) => (
        <Tooltip title={`${view.voice} · ${view.engine} ${view.engine_ver}`}>
          <Typography.Text type="secondary" className="morpho-mono">
            {hash ? hash.slice(0, 16) : 'not synthesized'}
          </Typography.Text>
        </Tooltip>
      ),
    },
  ];

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Alert
        type="info"
        showIcon
        message="TTS is keyed by what was synthesized, not by which candidate wanted it"
        description="input_hash = blake3(canonical(text) ‖ voice ‖ engine ‖ engine_ver ‖ params). Switching a selection back and forth costs nothing; rejecting a candidate never invalidates audio."
      />

      {broken.length > 0 && (
        <Alert
          type="error"
          showIcon
          message={`${broken.length} synthesis attempt${broken.length === 1 ? '' : 's'} failed`}
          description={
            <Space direction="vertical" size={2}>
              <Typography.Text type="secondary">
                {broken[0]?.last_error ?? 'The adapter reported no detail.'}
              </Typography.Text>
              <Typography.Text type="secondary">
                Retry or waive the job on the dead letters screen; the engine will not attempt these
                again on its own.
              </Typography.Text>
            </Space>
          }
        />
      )}
      {pending.length > 0 && (
        <Alert
          type="warning"
          showIcon
          message={`${pending.length} desired text${pending.length === 1 ? '' : 's'} not synthesized yet`}
          description="These rows exist in tts_desired but not in tts_assets; the engine will pick them up on the next pass."
        />
      )}

      <Card size="small" styles={{ body: { padding: 0 } }}>
        <Table<TtsStatusView>
          rowKey={(view) => `${view.kind}:${view.text}`}
          size="middle"
          columns={columns}
          dataSource={detail.tts}
          pagination={false}
          locale={{
            emptyText: (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description="No desired TTS texts — this word has no selected content yet."
              />
            ),
          }}
        />
      </Card>
    </Space>
  );
}
