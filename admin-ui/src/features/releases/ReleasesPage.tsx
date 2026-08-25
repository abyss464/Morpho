import { useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Col,
  Empty,
  Flex,
  Input,
  List,
  Modal,
  Row,
  Space,
  Statistic,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { CloudUploadOutlined, ReloadOutlined, WarningOutlined } from '@ant-design/icons';
import { extractGateFailures } from '../../api/client';
import { QueryState } from '../../components/QueryState';
import { WordLink } from '../../components/WordLink';
import { errorMessage } from '../../lib/errors';
import { formatBytes, formatTimestamp } from '../../lib/format';
import { useExportRelease, useReleasePreview, useReleases } from '../../hooks/queries';
import type { ExportGateFailure, HoldbackEntry, Release } from '../../api/types';

function GateFailureList({ failures }: { failures: ExportGateFailure[] }) {
  return (
    <List
      size="small"
      dataSource={failures}
      renderItem={(failure) => (
        <List.Item>
          <Space direction="vertical" size={2} style={{ width: '100%' }}>
            <Space size={8}>
              <Tag color="error" style={{ margin: 0 }}>
                {failure.gate}
              </Tag>
              {failure.word_id !== null && (
                <WordLink wordId={failure.word_id}>
                  {failure.lemma ?? `#${failure.word_id}`}
                </WordLink>
              )}
            </Space>
            <Typography.Text type="secondary">{failure.message}</Typography.Text>
          </Space>
        </List.Item>
      )}
    />
  );
}

function ExportButton({ gatesPass, blockingCount }: { gatesPass: boolean; blockingCount: number }) {
  const exportRelease = useExportRelease();
  const [open, setOpen] = useState(false);
  const [notes, setNotes] = useState('');
  const [failures, setFailures] = useState<ExportGateFailure[]>([]);
  const [message, setMessage] = useState<string | null>(null);

  return (
    <>
      <Tooltip
        title={
          gatesPass
            ? 'Build a byte-reproducible release package from the current working state.'
            : `${blockingCount} validation gate(s) currently fail; the export will be refused.`
        }
      >
        <Button
          type="primary"
          icon={<CloudUploadOutlined />}
          onClick={() => {
            setFailures([]);
            setMessage(null);
            setOpen(true);
          }}
        >
          Export release
        </Button>
      </Tooltip>

      <Modal
        open={open}
        title="Export a release"
        okText="Run export"
        confirmLoading={exportRelease.isPending}
        onCancel={() => setOpen(false)}
        width={680}
        onOk={() => {
          setFailures([]);
          setMessage(null);
          exportRelease.mutate(
            { ...(notes.trim() ? { notes: notes.trim() } : {}) },
            {
              onSuccess: () => {
                setOpen(false);
                setNotes('');
              },
              onError: (error) => {
                setFailures(extractGateFailures(error));
                setMessage(errorMessage(error));
              },
            },
          );
        }}
      >
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          <Alert
            type="info"
            showIcon
            message="Deterministic export"
            description="release.db is rebuilt from scratch with a fixed page size, ordered inserts and zero timestamps; media are copied under their content-addressed names. Identical inputs produce identical bytes and therefore an identical content_version."
          />
          {!gatesPass && (
            <Alert
              type="warning"
              showIcon
              icon={<WarningOutlined />}
              message={`${blockingCount} gate(s) are failing right now`}
              description="Running the export will return 409 and write nothing. The failures will be listed here."
            />
          )}
          <Input.TextArea
            rows={3}
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
            placeholder="Release notes (optional)"
            aria-label="Release notes"
          />
          {message && (
            <Alert type="error" showIcon message="Export refused" description={message} />
          )}
          {failures.length > 0 && (
            <Card size="small" title="Validation gate failures">
              <GateFailureList failures={failures} />
            </Card>
          )}
        </Space>
      </Modal>
    </>
  );
}

function HoldbackReportCard() {
  const query = useReleasePreview();

  const columns: ColumnsType<HoldbackEntry> = [
    {
      title: 'Impact',
      dataIndex: 'impact_count',
      key: 'impact_count',
      width: 100,
      align: 'right',
      defaultSortOrder: 'descend',
      sorter: (a, b) => a.impact_count - b.impact_count,
      render: (count: number) => (
        <Tooltip title="How many otherwise-shippable words this one keeps off the boat.">
          <Tag color={count > 3 ? 'red' : count > 0 ? 'orange' : 'default'} style={{ margin: 0 }}>
            {count}
          </Tag>
        </Tooltip>
      ),
    },
    {
      title: 'Word',
      dataIndex: 'lemma',
      key: 'lemma',
      width: 170,
      render: (lemma: string, row) => (
        <WordLink wordId={row.word_id} strong>
          {lemma}
        </WordLink>
      ),
    },
    {
      title: 'Root cause',
      dataIndex: 'root_cause',
      key: 'root_cause',
      width: 210,
      render: (cause: string) => (
        <Tag color={cause === 'dependency_holdback' ? 'geekblue' : 'orange'} style={{ margin: 0 }}>
          {cause}
        </Tag>
      ),
    },
    {
      title: 'Detail',
      dataIndex: 'root_cause_detail',
      key: 'root_cause_detail',
      render: (detail: string, row) => (
        <Space size={6} wrap>
          <Typography.Text type="secondary">{detail}</Typography.Text>
          {row.blocking_word_id !== null && (
            <WordLink wordId={row.blocking_word_id}>{row.blocking_lemma}</WordLink>
          )}
        </Space>
      ),
    },
  ];

  return (
    <QueryState query={query} skeletonRows={8}>
      {(report) => (
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <Row gutter={[12, 12]}>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic
                  title="Pass every gate"
                  value={report.shippable_count}
                  valueStyle={{ fontSize: 22 }}
                />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Tooltip title="The maximal dependency-closed subset R: what the export would actually contain.">
                  <Statistic
                    title="In the closed subset"
                    value={report.exportable_count}
                    valueStyle={{ fontSize: 22, color: '#2f9e63' }}
                  />
                </Tooltip>
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic
                  title="Held back"
                  value={report.excluded_count}
                  valueStyle={{ fontSize: 22, color: '#d99328' }}
                />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Flex justify="space-between" align="center">
                  <Statistic
                    title="Export gates"
                    value={report.gates_pass ? 'pass' : `${report.gate_failures.length} failing`}
                    valueStyle={{ fontSize: 22, color: report.gates_pass ? '#2f9e63' : '#d4443b' }}
                  />
                  <ExportButton
                    gatesPass={report.gates_pass}
                    blockingCount={report.gate_failures.length}
                  />
                </Flex>
              </Card>
            </Col>
          </Row>

          {report.gate_failures.length > 0 && (
            <Card size="small" title="Blocking validation gates">
              <GateFailureList failures={report.gate_failures} />
            </Card>
          )}

          <Card
            size="small"
            title="Holdback report"
            extra={
              <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                sorted by downstream impact — fix the top row first
              </Typography.Text>
            }
            styles={{ body: { padding: 0 } }}
          >
            <Table<HoldbackEntry>
              rowKey="word_id"
              size="small"
              columns={columns}
              dataSource={report.excluded}
              scroll={{ x: 900 }}
              pagination={{ pageSize: 15, showSizeChanger: false }}
              locale={{
                emptyText: (
                  <Empty
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    description="Nothing is held back — every active word is in the closed subset."
                  />
                ),
              }}
            />
          </Card>
        </Space>
      )}
    </QueryState>
  );
}

function ReleaseHistory() {
  const query = useReleases();

  const columns: ColumnsType<Release> = [
    {
      title: 'Version',
      dataIndex: 'version',
      key: 'version',
      width: 220,
      render: (version: string) => (
        <Typography.Text strong className="morpho-mono">
          {version}
        </Typography.Text>
      ),
    },
    {
      title: 'Exported',
      dataIndex: 'exported_at',
      key: 'exported_at',
      width: 180,
      render: (value: string) => formatTimestamp(value),
    },
    { title: 'By', dataIndex: 'exported_by', key: 'exported_by', width: 130 },
    {
      title: 'Plan',
      dataIndex: 'plan_id',
      key: 'plan_id',
      width: 90,
      render: (planId: number) => <Tag style={{ margin: 0 }}>#{planId}</Tag>,
    },
    {
      title: 'Words',
      dataIndex: 'word_count',
      key: 'word_count',
      width: 90,
      align: 'right',
    },
    {
      title: 'Media',
      dataIndex: 'media_count',
      key: 'media_count',
      width: 90,
      align: 'right',
    },
    {
      title: 'Size',
      dataIndex: 'total_bytes',
      key: 'total_bytes',
      width: 110,
      align: 'right',
      render: (bytes: number) => (
        <Typography.Text className="morpho-mono">{formatBytes(bytes)}</Typography.Text>
      ),
    },
    {
      title: 'Notes',
      dataIndex: 'notes',
      key: 'notes',
      render: (notes: string | null) =>
        notes ? (
          <Typography.Text type="secondary">{notes}</Typography.Text>
        ) : (
          <Typography.Text type="secondary">—</Typography.Text>
        ),
    },
  ];

  return (
    <QueryState
      query={query}
      isEmpty={(data) => data.items.length === 0}
      emptyText="No release has been exported yet."
    >
      {(data) => (
        <Card
          size="small"
          title="Release history"
          extra={
            <Button
              size="small"
              icon={<ReloadOutlined />}
              loading={query.isFetching}
              onClick={() => void query.refetch()}
            >
              Refresh
            </Button>
          }
          styles={{ body: { padding: 0 } }}
        >
          <Table<Release>
            rowKey="release_id"
            size="small"
            columns={columns}
            dataSource={data.items}
            pagination={false}
            scroll={{ x: 1100 }}
          />
        </Card>
      )}
    </QueryState>
  );
}

export function ReleasesPage() {
  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          Releases
        </Typography.Title>
        <Typography.Text type="secondary">
          The engine builds a release candidate whenever the export input hash changes. Shipping the
          APK stays a human decision.
        </Typography.Text>
      </Space>

      <HoldbackReportCard />
      <ReleaseHistory />
    </Space>
  );
}
