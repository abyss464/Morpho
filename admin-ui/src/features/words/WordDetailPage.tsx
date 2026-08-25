import { Link, useNavigate } from '@tanstack/react-router';
import {
  Button,
  Card,
  Descriptions,
  Flex,
  Space,
  Tabs,
  Tag,
  Tooltip,
  Typography,
  theme,
} from 'antd';
import { ArrowLeftOutlined, ReloadOutlined } from '@ant-design/icons';
import { AudioButton } from '../../components/AudioButton';
import { EventTimeline } from '../../components/EventTimeline';
import { formatTimestamp } from '../../lib/format';
import { QueryState } from '../../components/QueryState';
import { BlockerTags, ReadyBadge, RoleTag } from '../../components/StatusChips';
import { useWordDetail, useWordMutations } from '../../hooks/queries';
import { AudioTab } from './AudioTab';
import { DistractorsTab } from './DistractorsTab';
import { ExamplesTab } from './ExamplesTab';
import { ImageTab } from './ImageTab';
import { SensesTab } from './SensesTab';
import type { WordDetailTab } from './tabs';

export interface WordDetailPageProps {
  wordId: number;
  tab: WordDetailTab;
  onTabChange: (tab: WordDetailTab) => void;
}

export function WordDetailPage({ wordId, tab, onTabChange }: WordDetailPageProps) {
  const { token } = theme.useToken();
  const navigate = useNavigate();
  const query = useWordDetail(wordId);
  const mutations = useWordMutations(wordId);

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Flex justify="space-between" align="center" gap={12} wrap>
        <Button icon={<ArrowLeftOutlined />} onClick={() => void navigate({ to: '/words' })}>
          All words
        </Button>
        <Button
          icon={<ReloadOutlined />}
          loading={query.isFetching}
          onClick={() => void query.refetch()}
        >
          Refresh
        </Button>
      </Flex>

      <QueryState query={query} skeletonRows={8}>
        {(detail) => {
          const lemmaAudio = detail.tts.find((view) => view.kind === 'word');
          return (
            <Space direction="vertical" size={16} style={{ width: '100%' }}>
              <Card size="small">
                <Flex justify="space-between" align="flex-start" gap={16} wrap>
                  <Space direction="vertical" size={8}>
                    <Space size={12} align="center" wrap>
                      <Typography.Title level={2} style={{ margin: 0 }}>
                        {detail.word.lemma}
                      </Typography.Title>
                      {detail.word.phonetic && (
                        <Typography.Text type="secondary" style={{ fontSize: 16 }}>
                          {detail.word.phonetic}
                        </Typography.Text>
                      )}
                      <AudioButton
                        fileHash={lemmaAudio?.file_hash ?? null}
                        disabled={lemmaAudio?.status !== 'ready'}
                        title="Play the word audio"
                        size="middle"
                      />
                      <RoleTag role={detail.word.role} auxStatus={detail.word.aux_status} />
                      <ReadyBadge ready={detail.word.ready} />
                    </Space>
                    <BlockerTags blockers={detail.word.blockers} max={8} />
                  </Space>

                  <Descriptions
                    size="small"
                    column={1}
                    style={{ minWidth: 300 }}
                    items={[
                      {
                        key: 'id',
                        label: 'word_id',
                        children: (
                          <Typography.Text className="morpho-mono">
                            {detail.word.word_id}
                          </Typography.Text>
                        ),
                      },
                      {
                        key: 'rank',
                        label: 'frequency rank',
                        children: detail.word.frequency_rank ?? '—',
                      },
                      {
                        key: 'created',
                        label: 'created',
                        children: (
                          <Space size={6}>
                            <Tag style={{ margin: 0 }}>{detail.word.created_by}</Tag>
                            <Typography.Text type="secondary">
                              {formatTimestamp(detail.word.created_at)}
                            </Typography.Text>
                          </Space>
                        ),
                      },
                      {
                        key: 'etymology',
                        label: 'etymology',
                        children: detail.word.etymology ? (
                          <Tooltip title={`source: ${detail.word.etymology_source ?? 'unknown'}`}>
                            <Typography.Text>{detail.word.etymology}</Typography.Text>
                          </Tooltip>
                        ) : (
                          <Typography.Text type="secondary">not fetched yet</Typography.Text>
                        ),
                      },
                    ]}
                  />
                </Flex>
              </Card>

              <Card size="small" styles={{ body: { paddingTop: 8 } }}>
                <Tabs
                  activeKey={tab}
                  onChange={(key) => onTabChange(key as WordDetailTab)}
                  destroyOnHidden
                  items={[
                    {
                      key: 'senses',
                      label: `Senses (${detail.definitions.length})`,
                      children: <SensesTab detail={detail} mutations={mutations} />,
                    },
                    {
                      key: 'examples',
                      label: `Examples (${detail.examples.filter((slot) => slot.selection).length}/3)`,
                      children: <ExamplesTab detail={detail} mutations={mutations} />,
                    },
                    {
                      key: 'image',
                      label: `Image (${detail.image.candidates.length})`,
                      children: <ImageTab detail={detail} mutations={mutations} />,
                    },
                    {
                      key: 'audio',
                      label: `Audio (${detail.tts.filter((view) => view.status === 'ready').length}/${detail.tts.length})`,
                      children: <AudioTab detail={detail} />,
                    },
                    {
                      key: 'distractors',
                      label: `Distractors (${detail.distractors.length}/3)`,
                      children: <DistractorsTab detail={detail} />,
                    },
                    {
                      key: 'events',
                      label: 'Events',
                      children: (
                        <Space direction="vertical" size={12} style={{ width: '100%' }}>
                          <Typography.Text type="secondary">
                            Every selection change, approval flip and job outcome for this word,
                            newest first.
                          </Typography.Text>
                          <EventTimeline
                            events={detail.recent_events}
                            emptyText="No events recorded against this word yet."
                          />
                          <Link
                            to="/words"
                            search={{ q: detail.word.lemma }}
                            style={{ color: token.colorLink }}
                          >
                            Find related words →
                          </Link>
                        </Space>
                      ),
                    },
                  ]}
                />
              </Card>
            </Space>
          );
        }}
      </QueryState>
    </Space>
  );
}
