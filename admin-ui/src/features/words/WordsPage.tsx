import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import {
  App,
  Button,
  Card,
  Empty,
  Flex,
  Input,
  Segmented,
  Select,
  Space,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { CheckOutlined, ClearOutlined, ReloadOutlined } from '@ant-design/icons';
import { BlockerTags, ReadyBadge, RoleTag, WordAssetChips } from '../../components/StatusChips';
import { errorMessage } from '../../lib/errors';
import { usePlan, useWordList } from '../../hooks/queries';
import { collectMatchingWords, useBulkApprove } from '../../hooks/useBulkApprove';
import { BULK_ACTIONS, type BulkApproveKind } from './bulkApprove';
import { BulkApproveModal } from './BulkApproveModal';
import { DEFAULT_PAGE, DEFAULT_PAGE_SIZE, type WordsSearch } from './wordsSearch';
import type { BlockerCode, WordListItem, WordRole, WordsQuery } from '../../api/types';

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

/**
 * "Awaiting approval" presets. Each maps to exactly one `?blocker=` value,
 * because that blocker *is* the statement "this slot is filled but nobody has
 * signed off on it" — which is precisely the worklist the matching bulk action
 * clears. Server-side filtering keeps it honest at syllabus scale.
 */
const APPROVAL_PRESETS = [
  { value: 'all', label: 'All words', blocker: undefined },
  { value: 'sense_not_approved', label: 'Awaiting sense', blocker: 'sense_not_approved' },
  { value: 'example_not_approved', label: 'Awaiting example', blocker: 'example_not_approved' },
  { value: 'image_not_approved', label: 'Awaiting image', blocker: 'image_not_approved' },
] as const;

const PRESET_HINT =
  'Filters to the words whose slot is selected but unapproved — the queue the bulk actions clear.';

export interface WordsPageProps {
  search: WordsSearch;
  onSearchChange: (next: WordsSearch) => void;
}

export function WordsPage({ search, onSearchChange }: WordsPageProps) {
  const navigate = useNavigate();
  const { message } = App.useApp();
  const plan = usePlan();

  const page = search.page ?? DEFAULT_PAGE;
  const pageSize = search.page_size ?? DEFAULT_PAGE_SIZE;

  const listQuery: WordsQuery = {
    page,
    page_size: pageSize,
    role: search.role,
    ready: search.ready === undefined ? undefined : search.ready === 'true',
    blocker: search.blocker,
    group: search.group,
    q: search.q,
  };

  const query = useWordList(listQuery);

  /* ---- selection ---- */
  // Keyed by word_id and kept in this component rather than in the table, so a
  // selection made on page 1 survives paging, filtering and a live refetch.
  const [selected, setSelected] = useState<Map<number, string>>(() => new Map());
  const [pendingKind, setPendingKind] = useState<BulkApproveKind | null>(null);
  const [selectingAll, setSelectingAll] = useState(false);
  const selectAllAbort = useRef<AbortController | null>(null);
  const bulk = useBulkApprove();

  useEffect(() => () => selectAllAbort.current?.abort(), []);

  const selectedKeys = useMemo(() => [...selected.keys()], [selected]);
  const selectedWords = useMemo(
    () => [...selected.entries()].map(([word_id, lemma]) => ({ word_id, lemma })),
    [selected],
  );

  const toggleRows = useCallback((rows: readonly WordListItem[], next: boolean) => {
    setSelected((current) => {
      const updated = new Map(current);
      for (const row of rows) {
        if (next) updated.set(row.word_id, row.lemma);
        else updated.delete(row.word_id);
      }
      return updated;
    });
  }, []);

  const clearSelection = useCallback(() => setSelected(new Map()), []);

  const selectAllMatching = async () => {
    selectAllAbort.current?.abort();
    const controller = new AbortController();
    selectAllAbort.current = controller;
    setSelectingAll(true);
    try {
      const rows = await collectMatchingWords(
        { ...listQuery, page: undefined, page_size: undefined },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setSelected(new Map(rows.map((row) => [row.word_id, row.lemma])));
      message.success(`Selected ${rows.length} word${rows.length === 1 ? '' : 's'}.`);
    } catch (error) {
      if (!controller.signal.aborted) message.error(errorMessage(error));
    } finally {
      setSelectingAll(false);
    }
  };

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

  const activePreset =
    APPROVAL_PRESETS.find((preset) => preset.blocker === search.blocker)?.value ?? 'all';

  const total = query.data?.total ?? 0;
  const pageRows = query.data?.items ?? [];
  const allOnPageSelected =
    pageRows.length > 0 && pageRows.every((row) => selected.has(row.word_id));

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
          <Tooltip title={PRESET_HINT}>
            <Segmented
              value={activePreset}
              options={APPROVAL_PRESETS.map((preset) => ({
                value: preset.value,
                label: preset.label,
              }))}
              onChange={(value) => {
                const preset = APPROVAL_PRESETS.find((entry) => entry.value === value);
                patch({
                  blocker: preset?.blocker,
                  ready: preset?.blocker ? 'false' : search.ready,
                });
              }}
              aria-label="Approval worklist preset"
            />
          </Tooltip>
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

      {selected.size > 0 && (
        <Card
          size="small"
          styles={{ body: { padding: '10px 12px' } }}
          style={{ borderColor: '#4a90d9' }}
        >
          <Flex gap={12} wrap align="center" justify="space-between">
            <Space size={10} wrap>
              <Tag color="processing" style={{ margin: 0 }}>
                {selected.size} selected
              </Tag>
              {allOnPageSelected && selected.size < total && (
                <Button type="link" size="small" loading={selectingAll} onClick={selectAllMatching}>
                  Select all {total} matching this filter
                </Button>
              )}
              <Button size="small" icon={<ClearOutlined />} onClick={clearSelection}>
                Clear
              </Button>
            </Space>
            <Space size={8} wrap>
              <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                Bulk approve:
              </Typography.Text>
              {BULK_ACTIONS.map((action) => (
                <Tooltip key={action.kind} title={action.description}>
                  <Button
                    size="small"
                    icon={<CheckOutlined />}
                    onClick={() => setPendingKind(action.kind)}
                  >
                    {action.label}
                  </Button>
                </Tooltip>
              ))}
            </Space>
          </Flex>
        </Card>
      )}

      <BulkApproveModal
        kind={pendingKind}
        words={selectedWords}
        controller={bulk}
        onClose={() => setPendingKind(null)}
      />

      <Card size="small" styles={{ body: { padding: 0 } }}>
        <Table<WordListItem>
          rowKey="word_id"
          size="middle"
          columns={columns}
          dataSource={pageRows}
          rowSelection={{
            selectedRowKeys: selectedKeys,
            columnWidth: 44,
            onSelect: (row, checked) => toggleRows([row], checked),
            onSelectAll: (checked, _rows, changed) => toggleRows(changed, checked),
          }}
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
            onClick: (event: React.MouseEvent<HTMLElement>) => {
              // The checkbox column lives inside the row; a click there is a
              // selection, never a navigation.
              const target = event.target as HTMLElement;
              if (target.closest('.ant-table-selection-column, .ant-checkbox-wrapper')) return;
              openWord(row.word_id);
            },
            onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                openWord(row.word_id);
              }
            },
          })}
          summary={() =>
            pageRows.length > 0 ? (
              <Table.Summary fixed="bottom">
                <Table.Summary.Row>
                  <Table.Summary.Cell index={0} colSpan={7}>
                    <Space size={10}>
                      <Tooltip title="Rows on this page that pass every readiness gate">
                        <Tag color="success" style={{ margin: 0 }}>
                          {pageRows.filter((item) => item.ready).length} ready on page
                        </Tag>
                      </Tooltip>
                      <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                        Click a row to open its detail page; tick the checkbox to queue it for a
                        bulk approval.
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
