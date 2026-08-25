import { useMemo } from 'react';
import { useNavigate } from '@tanstack/react-router';
import {
  Button,
  Card,
  Empty,
  Flex,
  Input,
  Select,
  Space,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { ClearOutlined, ReloadOutlined } from '@ant-design/icons';
import { BlockerTags, ReadyBadge, RoleTag, WordAssetChips } from '../../components/StatusChips';
import { errorMessage } from '../../lib/errors';
import { usePlan, useWordList } from '../../hooks/queries';
import { DEFAULT_PAGE, DEFAULT_PAGE_SIZE, type WordsSearch } from './wordsSearch';
import type { BlockerCode, WordListItem, WordRole } from '../../api/types';

const ROLE_OPTIONS = [
  { value: 'target', label: 'target' },
  { value: 'auxiliary', label: 'auxiliary' },
  { value: 'base', label: 'base' },
];

const READY_OPTIONS = [
  { value: 'true', label: 'ready' },
  { value: 'false', label: 'blocked' },
];

const BLOCKER_OPTIONS: BlockerCode[] = [
  'missing_definition',
  'missing_primary_sense',
  'sense_not_approved',
  'oos_pending',
  'missing_example',
  'example_not_approved',
  'missing_image',
  'image_not_approved',
  'tts_missing',
  'tts_failed',
  'distractors_unbound',
  'distractor_1_not_ready',
  'distractor_2_not_ready',
  'distractor_3_not_ready',
];

export interface WordsPageProps {
  search: WordsSearch;
  onSearchChange: (next: WordsSearch) => void;
}

export function WordsPage({ search, onSearchChange }: WordsPageProps) {
  const navigate = useNavigate();
  const plan = usePlan();

  const page = search.page ?? DEFAULT_PAGE;
  const pageSize = search.page_size ?? DEFAULT_PAGE_SIZE;

  const query = useWordList({
    page,
    page_size: pageSize,
    role: search.role,
    ready: search.ready === undefined ? undefined : search.ready === 'true',
    blocker: search.blocker,
    group: search.group,
    q: search.q,
  });

  const patch = (partial: Partial<WordsSearch>) =>
    onSearchChange({ ...search, ...partial, page: partial.page ?? DEFAULT_PAGE });

  const openWord = (wordId: number) =>
    void navigate({ to: '/words/$wordId', params: { wordId: String(wordId) } });

  const groupOptions = useMemo(
    () =>
      (plan.data?.groups ?? []).map((group) => ({
        value: group.group_seq,
        label: `#${group.group_seq} · ${group.group_type} · ${group.word_count} words`,
      })),
    [plan.data],
  );

  const columns: ColumnsType<WordListItem> = [
    {
      title: 'Lemma',
      dataIndex: 'lemma',
      key: 'lemma',
      width: 180,
      fixed: 'left',
      render: (lemma: string, row) => (
        <Space size={8}>
          <Typography.Link strong onClick={() => openWord(row.word_id)}>
            {lemma}
          </Typography.Link>
        </Space>
      ),
    },
    {
      title: 'Role',
      dataIndex: 'role',
      key: 'role',
      width: 116,
      render: (role: WordRole) => <RoleTag role={role} />,
    },
    {
      title: 'Ready',
      dataIndex: 'ready',
      key: 'ready',
      width: 100,
      render: (ready: boolean) => <ReadyBadge ready={ready} />,
    },
    {
      title: 'Assets',
      key: 'assets',
      width: 320,
      render: (_, row) => <WordAssetChips word={row} />,
    },
    {
      title: 'Blockers',
      dataIndex: 'blockers',
      key: 'blockers',
      render: (blockers: BlockerCode[]) => <BlockerTags blockers={blockers} max={4} />,
    },
    {
      title: 'ID',
      dataIndex: 'word_id',
      key: 'word_id',
      width: 76,
      align: 'right',
      render: (id: number) => (
        <Typography.Text type="secondary" className="morpho-mono">
          {id}
        </Typography.Text>
      ),
    },
  ];

  const hasFilters = Boolean(
    search.role || search.ready || search.blocker || search.group || search.q,
  );

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Flex justify="space-between" align="flex-end" wrap gap={12}>
        <Space direction="vertical" size={0}>
          <Typography.Title level={3} style={{ margin: 0 }}>
            Words
          </Typography.Title>
          <Typography.Text type="secondary">
            Readiness is derived state, not a stored flag. Fix the blocker and the row turns green
            on the next pass.
          </Typography.Text>
        </Space>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => void query.refetch()}
            loading={query.isFetching}
          >
            Refresh
          </Button>
        </Space>
      </Flex>

      <Card size="small" styles={{ body: { padding: 12 } }}>
        <Flex gap={10} wrap align="center">
          <Input.Search
            allowClear
            placeholder="Search lemma"
            defaultValue={search.q}
            style={{ width: 240 }}
            onSearch={(value) => patch({ q: value.trim() || undefined })}
            aria-label="Filter by lemma"
          />
          <Select
            allowClear
            placeholder="Role"
            style={{ width: 150 }}
            value={search.role}
            options={ROLE_OPTIONS}
            onChange={(value) => patch({ role: value as WordRole | undefined })}
            aria-label="Filter by role"
          />
          <Select
            allowClear
            placeholder="Readiness"
            style={{ width: 150 }}
            value={search.ready}
            options={READY_OPTIONS}
            onChange={(value) => patch({ ready: value as WordsSearch['ready'] })}
            aria-label="Filter by readiness"
          />
          <Select
            allowClear
            showSearch
            placeholder="Blocker"
            style={{ width: 230 }}
            value={search.blocker}
            options={BLOCKER_OPTIONS.map((code) => ({ value: code, label: code }))}
            onChange={(value) => patch({ blocker: value as string | undefined })}
            aria-label="Filter by blocker code"
          />
          <Select
            allowClear
            showSearch
            optionFilterProp="label"
            placeholder="Plan group"
            style={{ width: 240 }}
            value={search.group}
            options={groupOptions}
            loading={plan.isPending}
            onChange={(value) => patch({ group: value as number | undefined })}
            aria-label="Filter by plan group"
          />
          {hasFilters && (
            <Button
              icon={<ClearOutlined />}
              onClick={() => onSearchChange({ page: DEFAULT_PAGE, page_size: pageSize })}
            >
              Clear
            </Button>
          )}
        </Flex>
      </Card>

      <Card size="small" styles={{ body: { padding: 0 } }}>
        <Table<WordListItem>
          rowKey="word_id"
          size="middle"
          columns={columns}
          dataSource={query.data?.items ?? []}
          loading={query.isPending}
          scroll={{ x: 1080 }}
          locale={{
            emptyText: query.isError ? (
              <Empty description={errorMessage(query.error)} />
            ) : (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={
                  hasFilters ? 'No word matches these filters.' : 'The lexicon is empty.'
                }
              />
            ),
          }}
          pagination={{
            current: page,
            pageSize,
            total: query.data?.total ?? 0,
            showSizeChanger: true,
            pageSizeOptions: [10, 25, 50, 100],
            showTotal: (total, range) => `${range[0]}–${range[1]} of ${total}`,
            onChange: (nextPage, nextSize) =>
              onSearchChange({ ...search, page: nextPage, page_size: nextSize }),
          }}
          onRow={(row) => ({
            className: 'morpho-row',
            tabIndex: 0,
            'aria-label': `${row.lemma}, ${row.ready ? 'ready' : 'blocked'}`,
            onClick: () => openWord(row.word_id),
            onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                openWord(row.word_id);
              }
            },
          })}
          summary={() =>
            query.data && query.data.items.length > 0 ? (
              <Table.Summary fixed="bottom">
                <Table.Summary.Row>
                  <Table.Summary.Cell index={0} colSpan={6}>
                    <Space size={10}>
                      <Tooltip title="Rows on this page that pass every readiness gate">
                        <Tag color="success" style={{ margin: 0 }}>
                          {query.data.items.filter((item) => item.ready).length} ready on page
                        </Tag>
                      </Tooltip>
                      <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                        Click a row or press Enter to open its detail page.
                      </Typography.Text>
                    </Space>
                  </Table.Summary.Cell>
                </Table.Summary.Row>
              </Table.Summary>
            ) : null
          }
        />
      </Card>
    </Space>
  );
}
