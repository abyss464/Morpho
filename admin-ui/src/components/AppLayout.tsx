import { useMemo, useState } from 'react';
import { Link, useRouterState } from '@tanstack/react-router';
import { Badge, Button, Layout, Menu, Space, Tooltip, Typography, theme } from 'antd';
import {
  AlertOutlined,
  BulbOutlined,
  DashboardOutlined,
  DeploymentUnitOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  MoonOutlined,
  PictureOutlined,
  ReadOutlined,
  RocketOutlined,
} from '@ant-design/icons';
import { useLiveStream } from '../app/liveStreamContext';
import { useThemeMode } from '../app/theme';
import { useDashboard } from '../hooks/queries';
import { GlobalSearch } from './GlobalSearch';

const { Header, Sider, Content } = Layout;

/**
 * Health of `GET /api/stream`. It is the only thing keeping the console current
 * once polling is gone, so its state has to be visible rather than assumed.
 */
function StreamIndicator() {
  const { status, enabled } = useLiveStream();
  if (!enabled) return null;

  const presentation = {
    open: { status: 'success' as const, label: 'live', hint: 'Change stream connected.' },
    connecting: {
      status: 'processing' as const,
      label: 'connecting',
      hint: 'Opening the change stream…',
    },
    reconnecting: {
      status: 'warning' as const,
      label: 'reconnecting',
      hint: 'Change stream dropped; retrying with backoff. The dashboard is polling meanwhile.',
    },
    closed: { status: 'default' as const, label: 'offline', hint: 'Change stream closed.' },
  }[status];

  return (
    <Tooltip title={presentation.hint}>
      <Badge
        status={presentation.status}
        text={
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            {presentation.label}
          </Typography.Text>
        }
      />
    </Tooltip>
  );
}

interface NavItem {
  key: string;
  to: string;
  label: string;
  icon: React.ReactNode;
  badge?: number;
}

export function AppLayout({ children }: { children: React.ReactNode }) {
  const { mode, toggle } = useThemeMode();
  const { token } = theme.useToken();
  const [collapsed, setCollapsed] = useState(false);
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const dashboard = useDashboard();

  const items = useMemo<NavItem[]>(
    () => [
      { key: '/', to: '/', label: 'Dashboard', icon: <DashboardOutlined /> },
      { key: '/words', to: '/words', label: 'Words', icon: <ReadOutlined /> },
      { key: '/gallery', to: '/gallery', label: 'Gallery', icon: <PictureOutlined /> },
      {
        key: '/oov',
        to: '/oov',
        label: 'OOV Queue',
        icon: <AlertOutlined />,
        badge: dashboard.data?.oos_open,
      },
      {
        key: '/dead-letters',
        to: '/dead-letters',
        label: 'Dead Letters',
        icon: <BulbOutlined />,
        badge: dashboard.data?.dead_letters,
      },
      { key: '/plan', to: '/plan', label: 'Plan', icon: <DeploymentUnitOutlined /> },
      { key: '/releases', to: '/releases', label: 'Releases', icon: <RocketOutlined /> },
    ],
    [dashboard.data],
  );

  const selectedKey =
    items.filter((item) => item.key !== '/').find((item) => pathname.startsWith(item.key))?.key ??
    '/';

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider
        collapsible
        collapsed={collapsed}
        trigger={null}
        width={216}
        style={{ borderRight: `1px solid ${token.colorBorderSecondary}` }}
      >
        <div
          style={{
            height: 56,
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            padding: collapsed ? '0 18px' : '0 20px',
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
          }}
        >
          <div
            aria-hidden
            style={{
              width: 22,
              height: 22,
              borderRadius: 6,
              flex: '0 0 auto',
              background: `linear-gradient(135deg, ${token.colorPrimary}, ${token.colorInfoActive})`,
            }}
          />
          {!collapsed && (
            <Typography.Text strong style={{ fontSize: 15, letterSpacing: 0.2 }}>
              Morpho
            </Typography.Text>
          )}
        </div>
        <Menu
          mode="inline"
          selectedKeys={[selectedKey]}
          style={{ borderInlineEnd: 'none', paddingTop: 8 }}
          items={items.map((item) => ({
            key: item.key,
            icon: item.icon,
            label: (
              <Link to={item.to} style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span>{item.label}</span>
                {!collapsed && item.badge ? (
                  <Badge
                    count={item.badge}
                    size="small"
                    color={item.key === '/dead-letters' ? token.colorError : token.colorWarning}
                  />
                ) : null}
              </Link>
            ),
          }))}
        />
      </Sider>

      <Layout>
        <Header
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 16,
            padding: '0 20px',
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
          }}
        >
          <Space size={12}>
            <Button
              type="text"
              aria-label={collapsed ? 'Expand navigation' : 'Collapse navigation'}
              icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
              onClick={() => setCollapsed((value) => !value)}
            />
            <GlobalSearch />
          </Space>
          <Space size={10}>
            <StreamIndicator />
            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
              {import.meta.env.VITE_API_MOCK === '1' ? 'mock data' : 'live morphod'}
            </Typography.Text>
            <Tooltip title={mode === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}>
              <Button
                type="text"
                aria-label="Toggle colour theme"
                icon={mode === 'dark' ? <BulbOutlined /> : <MoonOutlined />}
                onClick={toggle}
              />
            </Tooltip>
          </Space>
        </Header>

        <Content style={{ padding: 20, maxWidth: 1560, width: '100%', margin: '0 auto' }}>
          {children}
        </Content>
      </Layout>
    </Layout>
  );
}
