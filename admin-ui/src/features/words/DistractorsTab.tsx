import { Alert, Card, Col, Empty, Row, Space, Tag, Typography } from 'antd';
import { CheckCircleFilled, ExclamationCircleFilled } from '@ant-design/icons';
import { BlockerTags } from '../../components/StatusChips';
import { WordLink } from '../../components/WordLink';
import { formatTimestamp } from '../../lib/format';
import type { WordDetail } from '../../api/types';

export function DistractorsTab({ detail }: { detail: WordDetail }) {
  const notReady = detail.distractors.filter((link) => !link.core_ready);

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Alert
        type="info"
        showIcon
        message="Bound once, never recomputed"
        description="Distractors are deliberately exempt from the staleness machinery: the learner memorizes word-to-meaning, not process of elimination. Only a manual rebind changes these rows."
      />

      {notReady.length > 0 && (
        <Alert
          type="warning"
          showIcon
          message={`${notReady.length} bound distractor${notReady.length === 1 ? ' is' : 's are'} not core-ready`}
          description="This word cannot ship until every distractor carries its own image, definition and audio. Fixing the distractor fixes this word — no edit is needed here."
        />
      )}

      {detail.distractors.length === 0 ? (
        <Empty description="No distractors bound yet. The engine binds three morphologically close words on its next pass." />
      ) : (
        <Row gutter={[12, 12]}>
          {detail.distractors.map((link) => (
            <Col key={link.rank} xs={24} md={8}>
              <Card
                size="small"
                title={
                  <Space size={8}>
                    <Tag style={{ margin: 0 }}>rank {link.rank}</Tag>
                    <WordLink wordId={link.word_id} strong>
                      {link.lemma}
                    </WordLink>
                  </Space>
                }
                extra={
                  link.core_ready ? (
                    <Tag color="success" icon={<CheckCircleFilled />} style={{ margin: 0 }}>
                      core ready
                    </Tag>
                  ) : (
                    <Tag color="warning" icon={<ExclamationCircleFilled />} style={{ margin: 0 }}>
                      not ready
                    </Tag>
                  )
                }
              >
                <Space direction="vertical" size={8} style={{ width: '100%' }}>
                  <BlockerTags blockers={link.blockers} max={6} />
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                    bound {formatTimestamp(link.bound_at)} by {link.bound_by}
                  </Typography.Text>
                </Space>
              </Card>
            </Col>
          ))}
        </Row>
      )}
    </Space>
  );
}
