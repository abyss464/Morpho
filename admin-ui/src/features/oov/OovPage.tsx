import { useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Flex,
  Form,
  Input,
  Modal,
  Radio,
  Segmented,
  Space,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { ArrowUpOutlined, EditOutlined, ReloadOutlined } from '@ant-design/icons';
import { HighlightToken } from '../../components/HighlightText';
import { QueryState } from '../../components/QueryState';
import { WordLink } from '../../components/WordLink';
import { formatTimestamp } from '../../lib/format';
import { useOovList, useResolveOov } from '../../hooks/queries';
import type { OovOccurrence, OovQueueEntry, OovStatus } from '../../api/types';

/** All optional so `<Link to="/oov" />` needs no search object. */
export interface OovSearch {
  status?: OovStatus;
  page?: number;
  page_size?: number;
}

export const OOV_DEFAULT_STATUS: OovStatus = 'open';
export const OOV_DEFAULT_PAGE = 1;
export const OOV_DEFAULT_PAGE_SIZE = 20;

const STATUS_COLOR: Record<OovStatus, string> = {
  open: 'warning',
  resolved_rewrite: 'success',
  resolved_promote: 'purple',
  auto_closed: 'default',
};

function OccurrenceList({ entry }: { entry: OovQueueEntry }) {
  if (entry.occurrences.length === 0) {
    return (
      <Typography.Text type="secondary">
        No selected definition contains this lemma any more — the reconciler will auto-close the
        row.
      </Typography.Text>
    );
  }
  return (
    <Space direction="vertical" size={10} style={{ width: '100%' }}>
      {entry.occurrences.map((occurrence) => (
        <Card
          key={`${occurrence.word_id}:${occurrence.def_cand_id}`}
          size="small"
          styles={{ body: { padding: 12 } }}
        >
          <Space direction="vertical" size={6} style={{ width: '100%' }}>
            <Space size={8} wrap>
              <WordLink wordId={occurrence.word_id} strong>
                {occurrence.lemma}
              </WordLink>
              <Tag color="blue" style={{ margin: 0 }}>
                {occurrence.pos}
              </Tag>
              <Typography.Text type="secondary" className="morpho-mono">
                def #{occurrence.def_cand_id} · {occurrence.hits} hit
                {occurrence.hits === 1 ? '' : 's'}
              </Typography.Text>
            </Space>
            <HighlightToken text={occurrence.text} token={entry.oos_lemma} />
          </Space>
        </Card>
      ))}
    </Space>
  );
}

type ResolveMode = 'promote' | 'rewrite';

function ResolveModal({
  entry,
  initialMode,
  onClose,
}: {
  entry: OovQueueEntry | null;
  initialMode: ResolveMode;
  onClose: () => void;
}) {
  const resolve = useResolveOov();
  const [mode, setMode] = useState<ResolveMode>(initialMode);
  const [form] = Form.useForm<{ def_cand_id: number; text: string; notes?: string }>();

  const occurrences = entry?.occurrences ?? [];
  const defaultOccurrence: OovOccurrence | undefined = occurrences[0];

  const open = entry !== null;

  const handleOpenChange = () => {
    if (!open) return;
    setMode(occurrences.length === 0 ? 'promote' : initialMode);
    form.setFieldsValue({
      def_cand_id: defaultOccurrence?.def_cand_id ?? 0,
      text: defaultOccurrence?.suggested_rewrite ?? defaultOccurrence?.text ?? '',
      notes: undefined,
    });
  };

  return (
    <Modal
      open={open}
      title={
        entry ? (
          <Space size={8}>
            <span>Resolve</span>
            <Tag color="red" style={{ margin: 0 }}>
              {entry.oos_lemma}
            </Tag>
          </Space>
        ) : null
      }
      afterOpenChange={handleOpenChange}
      onCancel={onClose}
      width={720}
      okText={mode === 'promote' ? 'Promote to auxiliary' : 'Mint rewrite and select'}
      confirmLoading={resolve.isPending}
      okButtonProps={{ disabled: mode === 'rewrite' && occurrences.length === 0 }}
      onOk={() => {
        if (!entry) return;
        if (mode === 'promote') {
          resolve.mutate(
            { lemma: entry.oos_lemma, body: { mode: 'promote' } },
            { onSuccess: onClose },
          );
          return;
        }
        void form.validateFields().then((values) => {
          resolve.mutate(
            {
              lemma: entry.oos_lemma,
              body: {
                mode: 'rewrite',
                def_cand_id: values.def_cand_id,
                text: values.text.trim(),
                ...(values.notes ? { notes: values.notes } : {}),
              },
            },
            { onSuccess: onClose },
          );
        });
      }}
    >
      {entry && (
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          <Radio.Group
            value={mode}
            onChange={(event) => setMode(event.target.value as ResolveMode)}
            optionType="button"
            buttonStyle="solid"
            options={[
              {
                value: 'rewrite',
                label: 'Rewrite the definition',
                disabled: occurrences.length === 0,
              },
              { value: 'promote', label: 'Promote to auxiliary word' },
            ]}
          />

          {mode === 'promote' ? (
            <Alert
              type="warning"
              showIcon
              message={`Insert "${entry.oos_lemma}" as an auxiliary word`}
              description="A brand-new word row has zero assets, so the engine derives a full fetch fan-out for it. Every definition containing this token is re-classified immediately, and the plan places the new word before every word that references it."
            />
          ) : (
            <Alert
              type="info"
              showIcon
              message="Rewriting mints a new candidate"
              description="The rewrite is inserted as an llm_rewrite candidate linked to its parent and selected. The original candidate is never modified."
            />
          )}

          {mode === 'rewrite' && (
            <Form form={form} layout="vertical">
              <Form.Item
                name="def_cand_id"
                label="Definition to rewrite"
                rules={[{ required: true, message: 'Pick a definition.' }]}
              >
                <Radio.Group
                  style={{ width: '100%' }}
                  onChange={(event) => {
                    const chosen = occurrences.find(
                      (item) => item.def_cand_id === Number(event.target.value),
                    );
                    form.setFieldValue('text', chosen?.suggested_rewrite ?? chosen?.text ?? '');
                  }}
                >
                  <Space direction="vertical" size={8} style={{ width: '100%' }}>
                    {occurrences.map((occurrence) => (
                      <Radio key={occurrence.def_cand_id} value={occurrence.def_cand_id}>
                        <Space direction="vertical" size={2}>
                          <Typography.Text strong>
                            {occurrence.lemma} · {occurrence.pos}
                          </Typography.Text>
                          <HighlightToken text={occurrence.text} token={entry.oos_lemma} />
                        </Space>
                      </Radio>
                    ))}
                  </Space>
                </Radio.Group>
              </Form.Item>

              <Form.Item
                name="text"
                label={
                  <Space size={6}>
                    <span>Replacement text</span>
                    {defaultOccurrence?.suggested_rewrite && (
                      <Tooltip title="Pre-filled from the LLM rewrite draft the engine already produced.">
                        <Tag color="geekblue" style={{ margin: 0 }}>
                          llm draft
                        </Tag>
                      </Tooltip>
                    )}
                  </Space>
                }
                rules={[
                  { required: true, message: 'Replacement text is required.' },
                  {
                    validator: (_rule, value: string) =>
                      value && new RegExp(`\\b${entry.oos_lemma}\\b`, 'i').test(value)
                        ? Promise.reject(
                            new Error(`The rewrite still contains "${entry.oos_lemma}".`),
                          )
                        : Promise.resolve(),
                  },
                ]}
              >
                <Input.TextArea rows={3} />
              </Form.Item>

              <Form.Item name="notes" label="Notes (optional)">
                <Input placeholder="Why this wording" />
              </Form.Item>
            </Form>
          )}
        </Space>
      )}
    </Modal>
  );
}

export interface OovPageProps {
  search: OovSearch;
  onSearchChange: (next: OovSearch) => void;
}

export function OovPage({ search, onSearchChange }: OovPageProps) {
  const status = search.status ?? OOV_DEFAULT_STATUS;
  const page = search.page ?? OOV_DEFAULT_PAGE;
  const pageSize = search.page_size ?? OOV_DEFAULT_PAGE_SIZE;

  const query = useOovList({ status, page, page_size: pageSize });
  const [active, setActive] = useState<{ entry: OovQueueEntry; mode: ResolveMode } | null>(null);

  const columns: ColumnsType<OovQueueEntry> = [
    {
      title: 'Lemma',
      dataIndex: 'oos_lemma',
      key: 'oos_lemma',
      width: 180,
      render: (lemma: string) => <Typography.Text strong>{lemma}</Typography.Text>,
    },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      width: 160,
      render: (status: OovStatus) => (
        <Tag color={STATUS_COLOR[status]} style={{ margin: 0 }}>
          {status}
        </Tag>
      ),
    },
    {
      title: 'Occurrences',
      dataIndex: 'occurrence_count',
      key: 'occurrence_count',
      width: 130,
      align: 'right',
      sorter: (a, b) => a.occurrence_count - b.occurrence_count,
      render: (count: number) => (
        <Tag color={count > 0 ? 'blue' : 'default'} style={{ margin: 0 }}>
          {count} definition{count === 1 ? '' : 's'}
        </Tag>
      ),
    },
    {
      title: 'First seen',
      dataIndex: 'first_seen',
      key: 'first_seen',
      width: 180,
      render: (value: string) => (
        <Typography.Text type="secondary">{formatTimestamp(value)}</Typography.Text>
      ),
    },
    {
      title: 'Resolution',
      key: 'resolution',
      render: (_, row) =>
        row.status === 'open' ? (
          <Typography.Text type="secondary">pending</Typography.Text>
        ) : (
          <Typography.Text type="secondary">
            {row.resolved_by ? `${row.resolved_by} · ` : ''}
            {row.resolved_at ? formatTimestamp(row.resolved_at) : ''}
            {row.notes ? ` — ${row.notes}` : ''}
          </Typography.Text>
        ),
    },
    {
      title: '',
      key: 'actions',
      width: 190,
      render: (_, row) =>
        row.status === 'open' ? (
          <Space size={6}>
            <Button
              size="small"
              icon={<EditOutlined />}
              disabled={row.occurrence_count === 0}
              onClick={() => setActive({ entry: row, mode: 'rewrite' })}
            >
              Rewrite
            </Button>
            <Button
              size="small"
              type="primary"
              ghost
              icon={<ArrowUpOutlined />}
              onClick={() => setActive({ entry: row, mode: 'promote' })}
            >
              Promote
            </Button>
          </Space>
        ) : null,
    },
  ];

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Flex justify="space-between" align="flex-end" gap={12} wrap>
        <Space direction="vertical" size={0}>
          <Typography.Title level={3} style={{ margin: 0 }}>
            Out-of-scope queue
          </Typography.Title>
          <Typography.Text type="secondary">
            Tokens appearing in selected definitions that are neither base, target nor auxiliary.
            Each one breaks the readability invariant until it is rewritten away or promoted.
          </Typography.Text>
        </Space>
        <Space>
          <Segmented<OovStatus>
            value={status}
            onChange={(next) => onSearchChange({ ...search, status: next, page: 1 })}
            options={[
              { value: 'open', label: 'Open' },
              { value: 'resolved_rewrite', label: 'Rewritten' },
              { value: 'resolved_promote', label: 'Promoted' },
              { value: 'auto_closed', label: 'Auto-closed' },
            ]}
          />
          <Button
            icon={<ReloadOutlined />}
            loading={query.isFetching}
            onClick={() => void query.refetch()}
          >
            Refresh
          </Button>
        </Space>
      </Flex>

      <QueryState
        query={query}
        isEmpty={(data) => data.items.length === 0}
        emptyText={
          status === 'open'
            ? 'No open out-of-scope lemmas. Every selected definition reads with base, target and auxiliary vocabulary only.'
            : 'No rows with this status.'
        }
      >
        {(data) => (
          <Card size="small" styles={{ body: { padding: 0 } }}>
            <Table<OovQueueEntry>
              rowKey="oos_lemma"
              size="middle"
              columns={columns}
              dataSource={data.items}
              scroll={{ x: 1080 }}
              expandable={{
                expandedRowRender: (row) => <OccurrenceList entry={row} />,
                rowExpandable: (row) => row.occurrences.length > 0,
              }}
              pagination={{
                current: page,
                pageSize,
                total: data.total,
                showSizeChanger: true,
                showTotal: (total, range) => `${range[0]}–${range[1]} of ${total}`,
                onChange: (nextPage, nextSize) =>
                  onSearchChange({ ...search, page: nextPage, page_size: nextSize }),
              }}
            />
          </Card>
        )}
      </QueryState>

      <ResolveModal
        key={active?.entry.oos_lemma ?? 'none'}
        entry={active?.entry ?? null}
        initialMode={active?.mode ?? 'rewrite'}
        onClose={() => setActive(null)}
      />
    </Space>
  );
}
