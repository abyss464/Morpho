import { Alert, Card, Empty, Space, Table, Tag, Tooltip, Typography } from 'antd';
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
  const missing = detail.tts.filter((view) => view.status === 'missing');

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
          <Tag color={STATUS_COLOR[status]} style={{ margin: 0 }}>
            {status}
          </Tag>
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

      {failed.length > 0 && (
        <Alert
          type="error"
          showIcon
          message={`${failed.length} synthesis${failed.length === 1 ? '' : 'es'} failed`}
          description={failed[0]?.last_error ?? 'See the dead letters screen to retry or waive.'}
        />
      )}
      {missing.length > 0 && failed.length === 0 && (
        <Alert
          type="warning"
          showIcon
          message={`${missing.length} desired text${missing.length === 1 ? '' : 's'} not synthesized yet`}
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
