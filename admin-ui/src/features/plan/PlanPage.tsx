import { useEffect, useRef, useState } from 'react';
import {
  Card,
  Col,
  Descriptions,
  Empty,
  Flex,
  List,
  Progress,
  Row,
  Space,
  Statistic,
  Table,
  Tag,
  Tooltip,
  Typography,
  theme,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { BlockerTags, ReadyBadge, RoleTag } from '../../components/StatusChips';
import { QueryState } from '../../components/QueryState';
import { WordLink } from '../../components/WordLink';
import { formatTimestamp } from '../../lib/format';
import { usePlan, usePlanGroup } from '../../hooks/queries';
import type { PlanGroupSummary, PlanGroupType, PlanWordView } from '../../api/types';

const GROUP_TYPE_COLOR: Record<PlanGroupType, string> = {
  scc: 'magenta',
  root: 'geekblue',
  semantic: 'cyan',
  fill: 'default',
};

const GROUP_TYPE_HELP: Record<PlanGroupType, string> = {
  scc: 'A strongly connected component: these definitions reference each other, so they are learned together.',
  root: 'Aggregated by a shared morphological root.',
  semantic: 'Clustered by WordNet synset similarity.',
  fill: 'Filled to the 15–20 target size from the remaining topological order.',
};

/** Windowed group list: only the visible slice is rendered, so a 400-group plan
 *  scrolls without laying out four hundred rows. */
function GroupList({
  groups,
  selected,
  onSelect,
}: {
  groups: PlanGroupSummary[];
  selected: number | null;
  onSelect: (seq: number) => void;
}) {
  const { token } = theme.useToken();
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);

  const rowHeight = 64;
  const viewportHeight = 520;
  const overscan = 4;

  const startIndex = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const endIndex = Math.min(
    groups.length,
    Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan,
  );
  const visible = groups.slice(startIndex, endIndex);

  useEffect(() => {
    containerRef.current?.scrollTo({ top: 0 });
    setScrollTop(0);
  }, [groups.length]);

  if (groups.length === 0) {
    return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="This plan has no groups." />;
  }

  return (
    <div
      ref={containerRef}
      className="morpho-scroll"
      style={{ height: viewportHeight }}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      <div style={{ height: groups.length * rowHeight, position: 'relative' }}>
        <div style={{ transform: `translateY(${startIndex * rowHeight}px)` }}>
          {visible.map((group) => {
            const isActive = group.group_seq === selected;
            const percent =
              group.word_count === 0 ? 0 : Math.round((group.ready_count / group.word_count) * 100);
            return (
              <div
                key={group.group_seq}
                role="button"
                tabIndex={0}
                aria-label={`Group ${group.group_seq}, ${group.word_count} words`}
                onClick={() => onSelect(group.group_seq)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    onSelect(group.group_seq);
                  }
                }}
                style={{
                  height: rowHeight,
                  padding: '8px 12px',
                  cursor: 'pointer',
                  borderLeft: `3px solid ${isActive ? token.colorPrimary : 'transparent'}`,
                  background: isActive ? token.controlItemBgActive : 'transparent',
                }}
              >
                <Flex justify="space-between" align="center">
                  <Space size={8}>
                    <Typography.Text strong>#{group.group_seq}</Typography.Text>
                    <Tooltip title={GROUP_TYPE_HELP[group.group_type]}>
                      <Tag color={GROUP_TYPE_COLOR[group.group_type]} style={{ margin: 0 }}>
                        {group.group_type}
                      </Tag>
                    </Tooltip>
                  </Space>
                  <Typography.Text type="secondary" className="morpho-mono">
                    {group.ready_count}/{group.word_count}
                  </Typography.Text>
                </Flex>
                <Flex justify="space-between" align="center" gap={10}>
                  <Typography.Text type="secondary" ellipsis style={{ fontSize: 12, flex: 1 }}>
                    {group.first_lemma} … {group.last_lemma}
                  </Typography.Text>
                  <Progress
                    percent={percent}
                    size="small"
                    showInfo={false}
                    style={{ width: 62, margin: 0 }}
                  />
                </Flex>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function GroupWords({ seq }: { seq: number | null }) {
  const query = usePlanGroup(seq);

  const columns: ColumnsType<PlanWordView> = [
    {
      title: '#',
      dataIndex: 'learning_order',
      key: 'learning_order',
      width: 70,
      align: 'right',
      render: (order: number) => (
        <Typography.Text type="secondary" className="morpho-mono">
          {order}
        </Typography.Text>
      ),
    },
    {
      title: 'Lemma',
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
      title: 'Role',
      dataIndex: 'role',
      key: 'role',
      width: 116,
      render: (role: PlanWordView['role']) => <RoleTag role={role} />,
    },
    {
      title: 'Ready',
      dataIndex: 'ready',
      key: 'ready',
      width: 100,
      render: (ready: boolean) => <ReadyBadge ready={ready} />,
    },
    {
      title: 'Blockers',
      dataIndex: 'blockers',
      key: 'blockers',
      render: (blockers: PlanWordView['blockers']) => <BlockerTags blockers={blockers} max={4} />,
    },
  ];

  if (seq === null) {
    return (
      <Empty
        image={Empty.PRESENTED_IMAGE_SIMPLE}
        description="Pick a group to see its words in learning order."
      />
    );
  }

  return (
    <QueryState query={query} skeletonRows={8}>
      {(group) => (
        <Space direction="vertical" size={12} style={{ width: '100%' }}>
          <Space size={10} wrap>
            <Typography.Text strong>Group #{group.group_seq}</Typography.Text>
            <Tooltip title={GROUP_TYPE_HELP[group.group_type]}>
              <Tag color={GROUP_TYPE_COLOR[group.group_type]} style={{ margin: 0 }}>
                {group.group_type}
              </Tag>
            </Tooltip>
            <Typography.Text type="secondary">{group.words.length} words</Typography.Text>
          </Space>
          <Table<PlanWordView>
            rowKey="word_id"
            size="small"
            columns={columns}
            dataSource={group.words}
            pagination={false}
            scroll={{ x: 760, y: 460 }}
          />
        </Space>
      )}
    </QueryState>
  );
}

export function PlanPage() {
  const query = usePlan();
  const [selected, setSelected] = useState<number | null>(null);

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          Learning plan
        </Typography.Title>
        <Typography.Text type="secondary">
          A versioned artifact, not a column on the word table. Change one selected definition and
          the whole order is recomputed from scratch in milliseconds.
        </Typography.Text>
      </Space>

      <QueryState query={query} skeletonRows={6}>
        {(plan) => (
          <Space direction="vertical" size={16} style={{ width: '100%' }}>
            <Row gutter={[12, 12]}>
              <Col xs={12} md={6} xl={4}>
                <Card size="small">
                  <Statistic
                    title="Words"
                    value={plan.stats.word_count}
                    valueStyle={{ fontSize: 22 }}
                  />
                </Card>
              </Col>
              <Col xs={12} md={6} xl={4}>
                <Card size="small">
                  <Statistic
                    title="Groups"
                    value={plan.stats.group_count}
                    valueStyle={{ fontSize: 22 }}
                  />
                </Card>
              </Col>
              <Col xs={12} md={6} xl={4}>
                <Card size="small">
                  <Statistic
                    title="Dependency edges"
                    value={plan.stats.edge_count}
                    valueStyle={{ fontSize: 22 }}
                  />
                </Card>
              </Col>
              <Col xs={12} md={6} xl={4}>
                <Card size="small">
                  <Tooltip title="Groups formed from a strongly connected component of the dependency graph.">
                    <Statistic
                      title="SCC groups"
                      value={plan.stats.scc_group_count}
                      valueStyle={{ fontSize: 22 }}
                    />
                  </Tooltip>
                </Card>
              </Col>
              <Col xs={12} md={6} xl={4}>
                <Card size="small">
                  <Statistic
                    title="Largest group"
                    value={plan.stats.largest_group}
                    valueStyle={{ fontSize: 22 }}
                  />
                </Card>
              </Col>
              <Col xs={12} md={6} xl={4}>
                <Card size="small">
                  <Statistic
                    title="Avg group size"
                    value={plan.stats.avg_group_size}
                    precision={1}
                    valueStyle={{ fontSize: 22 }}
                  />
                </Card>
              </Col>
            </Row>

            <Card size="small" title={`Plan artifact #${plan.plan_id}`}>
              <Descriptions
                size="small"
                column={{ xs: 1, sm: 2, lg: 3 }}
                items={[
                  { key: 'built', label: 'built at', children: formatTimestamp(plan.built_at) },
                  { key: 'algo', label: 'algo_ver', children: plan.algo_ver },
                  {
                    key: 'hash',
                    label: 'input_hash',
                    children: (
                      <Typography.Text copyable className="morpho-mono">
                        {plan.input_hash.slice(0, 24)}
                      </Typography.Text>
                    ),
                  },
                  {
                    key: 'params',
                    label: 'params',
                    children: (
                      <Typography.Text className="morpho-mono">
                        {JSON.stringify(plan.params)}
                      </Typography.Text>
                    ),
                  },
                  {
                    key: 'diff',
                    label: `diff vs #${plan.diff.previous_plan_id ?? '—'}`,
                    children: (
                      <Space size={6}>
                        <Tag color="green" style={{ margin: 0 }}>
                          +{plan.diff.added} added
                        </Tag>
                        <Tag color="red" style={{ margin: 0 }}>
                          −{plan.diff.removed} removed
                        </Tag>
                        <Tag style={{ margin: 0 }}>{plan.diff.reordered} reordered</Tag>
                      </Space>
                    ),
                  },
                ]}
              />
            </Card>

            <Row gutter={[16, 16]}>
              <Col xs={24} lg={9} xl={7}>
                <Card
                  size="small"
                  title={`Groups (${plan.groups.length})`}
                  styles={{ body: { padding: 0 } }}
                  extra={
                    <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                      in learning order
                    </Typography.Text>
                  }
                >
                  <GroupList
                    groups={plan.groups}
                    selected={selected}
                    onSelect={(seq) => setSelected(seq)}
                  />
                </Card>
              </Col>
              <Col xs={24} lg={15} xl={17}>
                <Card size="small" title="Words in group">
                  <GroupWords seq={selected} />
                </Card>
              </Col>
            </Row>

            {plan.groups.length > 0 && selected === null && (
              <List
                size="small"
                header={
                  <Typography.Text type="secondary">
                    Every group is 15–20 words unless it is an SCC or a root cluster, which are
                    sized by the graph rather than by the target.
                  </Typography.Text>
                }
              />
            )}
          </Space>
        )}
      </QueryState>
    </Space>
  );
}
