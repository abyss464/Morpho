import { useMemo } from 'react';
import { Alert, Button, List, Modal, Progress, Space, Statistic, Tag, Typography } from 'antd';
import { CheckCircleFilled, CloseCircleFilled, MinusCircleOutlined } from '@ant-design/icons';
import { bulkAction, tally, type BulkApproveItem, type BulkApproveKind } from './bulkApprove';
import type { BulkApproveController, BulkTargetWord } from '../../hooks/useBulkApprove';

export interface BulkApproveModalProps {
  /** Null keeps the modal closed. */
  kind: BulkApproveKind | null;
  words: readonly BulkTargetWord[];
  controller: BulkApproveController;
  onClose: () => void;
}

const STATUS_ICON: Record<string, React.ReactNode> = {
  skipped: <MinusCircleOutlined />,
  failed: <CloseCircleFilled style={{ color: '#cf3d3d' }} />,
  approved: <CheckCircleFilled style={{ color: '#3f9142' }} />,
};

/**
 * Confirmation and progress for a bulk run, in that order and in one place, so
 * the operator sees the count before firing and the per-word outcome after.
 */
export function BulkApproveModal({ kind, words, controller, onClose }: BulkApproveModalProps) {
  const { progress } = controller;
  const action = kind ? bulkAction(kind) : null;
  const counts = useMemo(() => (progress ? tally(progress.items) : null), [progress]);

  const notable = useMemo(
    () => (progress?.items ?? []).filter((item) => item.status === 'failed'),
    [progress],
  );
  const skipped = useMemo(
    () => (progress?.items ?? []).filter((item) => item.status === 'skipped'),
    [progress],
  );

  const close = () => {
    controller.reset();
    onClose();
  };

  if (!action) return null;

  /* ---- phase 1: confirm ---- */
  if (!progress) {
    return (
      <Modal
        open
        title={action.label}
        okText={`Approve ${words.length} word${words.length === 1 ? '' : 's'}`}
        cancelText="Cancel"
        onOk={() => void controller.start(action.kind, words)}
        onCancel={close}
        okButtonProps={{ disabled: words.length === 0 }}
        width={560}
      >
        <Space direction="vertical" size={12} style={{ width: '100%' }}>
          <Statistic title="Selected words" value={words.length} />
          <Typography.Paragraph style={{ marginBottom: 0 }}>
            {action.description}
          </Typography.Paragraph>
          <Alert
            type="info"
            showIcon
            message="Words with nothing to approve are skipped, not failed."
            description="Each word is read first; one with no eligible selection, or one already approved, is reported as a skip and no write is sent for it."
          />
          <Typography.Text type="secondary">
            Writes go out one word at a time and can be stopped mid-run.
          </Typography.Text>
        </Space>
      </Modal>
    );
  }

  /* ---- phase 2: progress and outcome ---- */
  const percent = counts && counts.total > 0 ? Math.round((counts.done / counts.total) * 100) : 0;
  const current = progress.items[progress.cursor];

  return (
    <Modal
      open
      title={action.label}
      onCancel={progress.running ? undefined : close}
      closable={!progress.running}
      maskClosable={false}
      width={640}
      footer={
        progress.running ? (
          <Button danger onClick={controller.cancel} disabled={progress.cancelled}>
            {progress.cancelled ? 'Stopping…' : 'Stop after this word'}
          </Button>
        ) : (
          <Space>
            {notable.length > 0 && (
              <Button
                onClick={() =>
                  void controller.start(
                    action.kind,
                    notable.map((item) => ({ word_id: item.word_id, lemma: item.lemma })),
                  )
                }
              >
                Retry {notable.length} failed
              </Button>
            )}
            <Button type="primary" onClick={close}>
              Close
            </Button>
          </Space>
        )
      }
    >
      <Space direction="vertical" size={12} style={{ width: '100%' }}>
        <Progress
          percent={percent}
          status={progress.running ? 'active' : notable.length > 0 ? 'exception' : 'success'}
        />
        <Space size={24} wrap>
          <Statistic title="Approved" value={counts?.approved ?? 0} />
          <Statistic title="Skipped" value={counts?.skipped ?? 0} />
          <Statistic
            title="Failed"
            value={counts?.failed ?? 0}
            valueStyle={notable.length > 0 ? { color: '#cf3d3d' } : undefined}
          />
          <Statistic title="Total" value={counts?.total ?? 0} />
        </Space>

        {progress.running && current && (
          <Typography.Text type="secondary">
            Working on <Typography.Text strong>{current.lemma}</Typography.Text> (
            {progress.cursor + 1} of {counts?.total ?? 0})…
          </Typography.Text>
        )}

        {progress.cancelled && !progress.running && (
          <Alert
            type="warning"
            showIcon
            message="Run stopped early."
            description="Words after the cursor were left untouched; re-select them to continue."
          />
        )}

        {notable.length > 0 && <ResultList title="Failures" items={notable} />}
        {skipped.length > 0 && <ResultList title="Skipped" items={skipped} />}
      </Space>
    </Modal>
  );
}

function ResultList({ title, items }: { title: string; items: readonly BulkApproveItem[] }) {
  return (
    <div>
      <Typography.Text strong>
        {title} <Tag style={{ marginInlineStart: 6 }}>{items.length}</Tag>
      </Typography.Text>
      <List
        size="small"
        bordered
        style={{ maxHeight: 200, overflowY: 'auto', marginTop: 8 }}
        dataSource={items as BulkApproveItem[]}
        renderItem={(item) => (
          <List.Item>
            <Space size={8} align="start">
              {STATUS_ICON[item.status]}
              <Typography.Text strong>{item.lemma}</Typography.Text>
              <Typography.Text type="secondary">{item.note}</Typography.Text>
            </Space>
          </List.Item>
        )}
      />
    </div>
  );
}
