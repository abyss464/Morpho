import { useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Col,
  Empty,
  Flex,
  Pagination,
  Popconfirm,
  Row,
  Space,
  Statistic,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { ReloadOutlined, RedoOutlined, StopOutlined } from '@ant-design/icons';
import { QueryState } from '../../components/QueryState';
import { WordLink } from '../../components/WordLink';
import { formatTimestamp } from '../../lib/format';
import { useDeadLetterActions, useDeadLetters } from '../../hooks/queries';
import type { DeadLetter } from '../../api/types';

const LANE_HINT: Record<string, string> = {
  unsplash: 'Image lane. Waiving all three stock lanes arms the SDXL fallback.',
  pexels: 'Image lane. Waiving all three stock lanes arms the SDXL fallback.',
  pixabay: 'Image lane. Waiving all three stock lanes arms the SDXL fallback.',
  sdxl: 'Local ComfyUI generation, concurrency 1, 10 minute timeout.',
  edge_tts: 'Subprocess adapter with a 60 second hard timeout.',
  llm: 'OpenAI-compatible endpoint; rewrites that still carry an OOV token fail permanently.',
  freedict: 'Free Dictionary API; a 404 is a legitimate empty result, not a failure.',
  wiktionary: 'Etymology source. Waiving it arms the Morfessor fallback.',
  cpu: 'Local compute lane: extraction, scoring, distractor binding.',
};

const DEFAULT_PAGE_SIZE = 50;

export function DeadLettersPage() {
  // Morphod caps `page_size` at 200 and defaults to 50, so a real incident —
  // a whole lane dying at once — arrives as several pages, not one list.
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE);
  const query = useDeadLetters({ page, page_size: pageSize });
  const { retry, waive } = useDeadLetterActions();

  const columns: ColumnsType<DeadLetter> = [
    {
      title: 'Lane',
      dataIndex: 'rate_key',
      key: 'rate_key',
      width: 130,
      render: (rateKey: string) => (
        <Tooltip title={LANE_HINT[rateKey] ?? 'Adapter lane'}>
          <Tag color="volcano" style={{ margin: 0 }}>
            {rateKey}
          </Tag>
        </Tooltip>
      ),
    },
    {
      title: 'Job',
      dataIndex: 'kind',
      key: 'kind',
      width: 190,
      render: (kind: string) => <Typography.Text className="morpho-mono">{kind}</Typography.Text>,
    },
    {
      title: 'Subject',
      key: 'subject',
      width: 200,
      render: (_, row) =>
        row.subject.word_id !== null ? (
          <WordLink wordId={row.subject.word_id} strong>
            {row.subject.lemma}
          </WordLink>
        ) : (
          <Typography.Text type="secondary">{row.subject.label}</Typography.Text>
        ),
    },
    {
      title: 'Attempts',
      dataIndex: 'attempts',
      key: 'attempts',
      width: 100,
      align: 'right',
      sorter: (a, b) => a.attempts - b.attempts,
      render: (attempts: number) => (
        <Tag color="red" style={{ margin: 0 }}>
          {attempts}
        </Tag>
      ),
    },
    {
      title: 'Last error',
      dataIndex: 'last_error',
      key: 'last_error',
      render: (error: string | null) => (
        <Tooltip title={error ?? undefined}>
          <Typography.Text type="danger" ellipsis style={{ maxWidth: 420 }}>
            {error ?? '—'}
          </Typography.Text>
        </Tooltip>
      ),
    },
    {
      title: 'Updated',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 170,
      render: (value: string) => (
        <Typography.Text type="secondary">{formatTimestamp(value)}</Typography.Text>
      ),
    },
    {
      title: '',
      key: 'actions',
      width: 180,
      render: (_, row) => {
        const key = {
          kind: row.kind,
          subject_type: row.subject_type,
          subject_id: row.subject_id,
        };
        // Spinners belong on the row being written, not on all 134 of them.
        const isTarget = (variables: typeof key | undefined) =>
          variables?.kind === key.kind && variables.subject_id === key.subject_id;

        return (
          <Space size={6}>
            <Tooltip title="Delete the job_state row; the demand is re-derived on the next pass.">
              <Button
                size="small"
                icon={<RedoOutlined />}
                loading={retry.isPending && isTarget(retry.variables)}
                onClick={() => retry.mutate(key)}
              >
                Retry
              </Button>
            </Tooltip>
            <Popconfirm
              title="Waive this requirement?"
              description="Waiving says the need is permanently satisfied by absence — that is exactly what arms the fallback rule for this lane."
              okText="Waive"
              onConfirm={() => waive.mutate(key)}
            >
              <Button
                size="small"
                danger
                icon={<StopOutlined />}
                loading={waive.isPending && isTarget(waive.variables)}
              >
                Waive
              </Button>
            </Popconfirm>
          </Space>
        );
      },
    },
  ];

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Flex justify="space-between" align="flex-end" gap={12} wrap>
        <Space direction="vertical" size={0}>
          <Typography.Title level={3} style={{ margin: 0 }}>
            Dead letters
          </Typography.Title>
          <Typography.Text type="secondary">
            Jobs that exhausted their retry budget. Both actions are plain database writes; there is
            no "re-run stage" button because there are no stages.
          </Typography.Text>
        </Space>
        <Button
          icon={<ReloadOutlined />}
          loading={query.isFetching}
          onClick={() => void query.refetch()}
        >
          Refresh
        </Button>
      </Flex>

      <QueryState
        query={query}
        isEmpty={(data) => data.items.length === 0}
        emptyText="No dead letters. Every lane is either healthy or backing off."
      >
        {(data) => {
          const byLane = data.items.reduce<Record<string, DeadLetter[]>>((acc, row) => {
            (acc[row.rate_key] ??= []).push(row);
            return acc;
          }, {});
          const lanes = Object.entries(byLane).sort((a, b) => b[1].length - a[1].length);
          const partial = data.items.length < data.total;
          const from = (page - 1) * pageSize + 1;
          const to = from + data.items.length - 1;

          return (
            <Space direction="vertical" size={16} style={{ width: '100%' }}>
              <Alert
                type="warning"
                showIcon
                message={`${data.total} job${data.total === 1 ? '' : 's'} in the dead-letter box`}
                description="Dead jobs are excluded from derivation, so the words behind them stay blocked indefinitely until a human retries or waives."
              />

              <Flex align="center" justify="space-between" gap={12} wrap>
                <Typography.Text type="secondary">
                  {partial
                    ? `Showing ${from}–${to} of ${data.total}; the lane breakdown below covers this page.`
                    : `All ${data.total} row${data.total === 1 ? '' : 's'} across ${lanes.length} lane${lanes.length === 1 ? '' : 's'}.`}
                </Typography.Text>
                <Pagination
                  size="small"
                  current={page}
                  pageSize={pageSize}
                  total={data.total}
                  showSizeChanger
                  pageSizeOptions={[25, 50, 100, 200]}
                  showTotal={(count, range) => `${range[0]}–${range[1]} of ${count}`}
                  onChange={(nextPage, nextSize) => {
                    setPage(nextPage);
                    setPageSize(nextSize);
                  }}
                />
              </Flex>

              <Row gutter={[12, 12]}>
                {lanes.map(([lane, rows]) => (
                  <Col key={lane} xs={12} sm={8} md={6} xl={4}>
                    <Card size="small" styles={{ body: { padding: 14 } }}>
                      <Statistic
                        title={
                          <Tooltip title={LANE_HINT[lane] ?? 'Adapter lane'}>
                            <span>{lane}</span>
                          </Tooltip>
                        }
                        value={rows.length}
                        suffix={
                          partial ? (
                            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                              on this page
                            </Typography.Text>
                          ) : undefined
                        }
                        valueStyle={{ fontSize: 22 }}
                      />
                    </Card>
                  </Col>
                ))}
              </Row>

              {lanes.map(([lane, rows]) => (
                <Card
                  key={lane}
                  size="small"
                  title={
                    <Space size={8}>
                      <Tag color="volcano" style={{ margin: 0 }}>
                        {lane}
                      </Tag>
                      <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                        {LANE_HINT[lane] ?? 'Adapter lane'}
                      </Typography.Text>
                    </Space>
                  }
                  styles={{ body: { padding: 0 } }}
                >
                  <Table<DeadLetter>
                    rowKey={(row) => `${row.kind}:${row.subject_type}:${row.subject_id}`}
                    size="small"
                    columns={columns}
                    dataSource={rows}
                    pagination={false}
                    scroll={{ x: 1180 }}
                    locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
                  />
                </Card>
              ))}
            </Space>
          );
        }}
      </QueryState>
    </Space>
  );
}
