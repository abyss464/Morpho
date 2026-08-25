import { Link } from '@tanstack/react-router';
import ReactECharts from 'echarts-for-react';
import {
  Card,
  Col,
  Empty,
  Flex,
  Progress,
  Row,
  Skeleton,
  Space,
  Statistic,
  Tag,
  Typography,
  theme,
} from 'antd';
import {
  AlertOutlined,
  BulbOutlined,
  CheckCircleOutlined,
  DeploymentUnitOutlined,
  ExclamationCircleOutlined,
  ReadOutlined,
} from '@ant-design/icons';
import { QueryState } from '../../components/QueryState';
import { EventTimeline } from '../../components/EventTimeline';
import { formatTimestamp } from '../../lib/format';
import { useDashboard } from '../../hooks/queries';
import type { AssetRollup, DashboardResponse } from '../../api/types';

/** Router links render their own anchor, so they carry the link styling. */
const LINK_STYLE: React.CSSProperties = { fontSize: 12, color: '#3b6fd4' };

function PageTitle() {
  return (
    <Space direction="vertical" size={0} style={{ marginBottom: 16 }}>
      <Typography.Title level={3} style={{ margin: 0 }}>
        Reconciliation overview
      </Typography.Title>
      <Typography.Text type="secondary">
        Desired state versus current state. Nothing here is a trigger — the engine converges on its
        own; these numbers say where a human still has to decide something.
      </Typography.Text>
    </Space>
  );
}

function StatCard({
  title,
  value,
  suffix,
  icon,
  tone,
  footer,
}: {
  title: string;
  value: number;
  suffix?: string;
  icon: React.ReactNode;
  tone?: string;
  footer?: React.ReactNode;
}) {
  return (
    <Card size="small" styles={{ body: { padding: 16 } }}>
      <Statistic
        title={
          <Space size={6}>
            <span style={{ color: tone }}>{icon}</span>
            <span>{title}</span>
          </Space>
        }
        value={value}
        suffix={suffix}
        valueStyle={{ color: tone, fontSize: 26 }}
      />
      {footer}
    </Card>
  );
}

function ReadinessDonut({ data }: { data: DashboardResponse }) {
  const { token } = theme.useToken();
  const active = data.words.ready + data.words.blocked;
  const option = {
    animationDuration: 420,
    tooltip: { trigger: 'item', formatter: '{b}: {c} ({d}%)' },
    legend: {
      bottom: 0,
      icon: 'circle',
      textStyle: { color: token.colorTextSecondary, fontSize: 12 },
    },
    series: [
      {
        type: 'pie',
        radius: ['58%', '80%'],
        center: ['50%', '44%'],
        avoidLabelOverlap: true,
        itemStyle: { borderRadius: 4, borderWidth: 2, borderColor: token.colorBgContainer },
        label: {
          show: true,
          position: 'center',
          formatter: () => `${active === 0 ? 0 : Math.round((data.words.ready / active) * 100)}%`,
          fontSize: 26,
          fontWeight: 600,
          color: token.colorText,
        },
        emphasis: { label: { show: true, fontSize: 26 } },
        data: [
          { value: data.words.ready, name: 'ready', itemStyle: { color: token.colorSuccess } },
          { value: data.words.blocked, name: 'blocked', itemStyle: { color: token.colorWarning } },
        ],
      },
    ],
  };
  return <ReactECharts option={option} style={{ height: 260 }} notMerge lazyUpdate />;
}

const ASSET_ROWS: Array<{ key: keyof DashboardResponse['assets']; label: string }> = [
  { key: 'definitions', label: 'Definitions' },
  { key: 'examples', label: 'Examples (slot 1)' },
  { key: 'images', label: 'Images' },
  { key: 'tts', label: 'TTS assets' },
];

function AssetGapBars({ data }: { data: DashboardResponse }) {
  const { token } = theme.useToken();
  const categories = ASSET_ROWS.map((row) => row.label);
  const pick = (field: keyof AssetRollup) => ASSET_ROWS.map((row) => data.assets[row.key][field]);

  const option = {
    animationDuration: 420,
    grid: { left: 8, right: 18, top: 34, bottom: 6, containLabel: true },
    tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
    legend: {
      top: 0,
      icon: 'circle',
      textStyle: { color: token.colorTextSecondary, fontSize: 12 },
    },
    xAxis: {
      type: 'value',
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: token.colorBorderSecondary } },
      axisLabel: { color: token.colorTextTertiary },
    },
    yAxis: {
      type: 'category',
      data: categories,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { color: token.colorTextSecondary },
    },
    series: [
      {
        name: 'ready',
        type: 'bar',
        stack: 'total',
        barWidth: 18,
        itemStyle: { color: token.colorSuccess, borderRadius: [3, 0, 0, 3] },
        data: pick('ready'),
      },
      {
        name: 'missing',
        type: 'bar',
        stack: 'total',
        itemStyle: { color: token.colorTextQuaternary },
        data: pick('missing'),
      },
      {
        name: 'unapproved / failed',
        type: 'bar',
        stack: 'total',
        itemStyle: { color: token.colorWarning, borderRadius: [0, 3, 3, 0] },
        data: pick('failed'),
      },
    ],
  };
  return <ReactECharts option={option} style={{ height: 260 }} notMerge lazyUpdate />;
}

function AssetReadinessRates({ data }: { data: DashboardResponse }) {
  const { token } = theme.useToken();
  return (
    <Space direction="vertical" size={14} style={{ width: '100%' }}>
      {ASSET_ROWS.map(({ key, label }) => {
        const rollup = data.assets[key];
        const total = rollup.ready + rollup.missing + rollup.failed;
        const percent = total === 0 ? 0 : Math.round((rollup.ready / total) * 100);
        return (
          <div key={key}>
            <Flex justify="space-between" align="baseline">
              <Typography.Text>{label}</Typography.Text>
              <Typography.Text type="secondary" className="morpho-mono">
                {rollup.ready}/{total}
              </Typography.Text>
            </Flex>
            <Progress
              percent={percent}
              size="small"
              strokeColor={percent === 100 ? token.colorSuccess : token.colorPrimary}
              aria-label={`${label} readiness`}
            />
          </div>
        );
      })}
    </Space>
  );
}

export function DashboardPage() {
  const query = useDashboard();

  return (
    <>
      <PageTitle />
      <QueryState
        query={query}
        skeleton={
          <Row gutter={[16, 16]}>
            {[0, 1, 2, 3, 4, 5].map((index) => (
              <Col key={index} xs={24} sm={12} lg={8} xl={4}>
                <Card size="small">
                  <Skeleton active paragraph={{ rows: 1 }} />
                </Card>
              </Col>
            ))}
          </Row>
        }
      >
        {(data) => {
          const active = data.words.ready + data.words.blocked;
          return (
            <Space direction="vertical" size={16} style={{ width: '100%' }}>
              <Row gutter={[16, 16]}>
                <Col xs={24} sm={12} xl={5}>
                  <StatCard
                    title="Words in scope"
                    value={active}
                    icon={<ReadOutlined />}
                    footer={
                      <Space size={6} style={{ marginTop: 6 }} wrap>
                        <Tag color="blue" style={{ margin: 0 }}>
                          {data.words.target} target
                        </Tag>
                        <Tag color="purple" style={{ margin: 0 }}>
                          {data.words.auxiliary} auxiliary
                        </Tag>
                      </Space>
                    }
                  />
                </Col>
                <Col xs={24} sm={12} xl={5}>
                  <StatCard
                    title="Ready to ship"
                    value={data.words.ready}
                    suffix={`/ ${active}`}
                    icon={<CheckCircleOutlined />}
                    tone="#2f9e63"
                  />
                </Col>
                <Col xs={24} sm={12} xl={4}>
                  <StatCard
                    title="Blocked"
                    value={data.words.blocked}
                    icon={<ExclamationCircleOutlined />}
                    tone="#d99328"
                    footer={
                      <Link to="/words" search={{ ready: 'false' }} style={LINK_STYLE}>
                        Open worklist →
                      </Link>
                    }
                  />
                </Col>
                <Col xs={24} sm={12} xl={4}>
                  <StatCard
                    title="OOV open"
                    value={data.oos_open}
                    icon={<AlertOutlined />}
                    tone={data.oos_open > 0 ? '#d99328' : undefined}
                    footer={
                      <Link to="/oov" style={LINK_STYLE}>
                        Resolve queue →
                      </Link>
                    }
                  />
                </Col>
                <Col xs={24} sm={12} xl={3}>
                  <StatCard
                    title="Dead letters"
                    value={data.dead_letters}
                    icon={<BulbOutlined />}
                    tone={data.dead_letters > 0 ? '#d4443b' : undefined}
                    footer={
                      <Link to="/dead-letters" style={LINK_STYLE}>
                        Triage →
                      </Link>
                    }
                  />
                </Col>
                <Col xs={24} sm={12} xl={3}>
                  <StatCard
                    title="Plan groups"
                    value={data.plan?.group_count ?? 0}
                    icon={<DeploymentUnitOutlined />}
                    footer={
                      <Typography.Text type="secondary" style={{ fontSize: 11 }}>
                        {data.plan
                          ? `#${data.plan.plan_id} · ${formatTimestamp(data.plan.built_at)}`
                          : 'no plan'}
                      </Typography.Text>
                    }
                  />
                </Col>
              </Row>

              <Row gutter={[16, 16]}>
                <Col xs={24} lg={8}>
                  <Card title="Word readiness" size="small">
                    <ReadinessDonut data={data} />
                  </Card>
                </Col>
                <Col xs={24} lg={9}>
                  <Card title="Asset gaps by type" size="small">
                    <AssetGapBars data={data} />
                  </Card>
                </Col>
                <Col xs={24} lg={7}>
                  <Card
                    title="Asset readiness rate"
                    size="small"
                    styles={{ body: { padding: 20 } }}
                  >
                    <AssetReadinessRates data={data} />
                  </Card>
                </Col>
              </Row>

              <Card
                title="Recent events"
                size="small"
                extra={
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                    append-only audit log, newest first
                  </Typography.Text>
                }
              >
                {data.recent_events.length === 0 ? (
                  <Empty
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    description="No events recorded yet."
                  />
                ) : (
                  <div className="morpho-scroll" style={{ maxHeight: 380, paddingTop: 8 }}>
                    <EventTimeline events={data.recent_events} />
                  </div>
                )}
              </Card>
            </Space>
          );
        }}
      </QueryState>
    </>
  );
}
