import { useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Empty,
  Flex,
  Form,
  Input,
  Popconfirm,
  Select,
  Space,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import { DeleteOutlined, PlusOutlined } from '@ant-design/icons';
import type { ExampleCandidate, ExampleSlotNumber, WordDetail } from '../../api/types';
import { HighlightRange } from '../../components/HighlightText';
import { ScoreBadge, SourceBadge } from '../../components/StatusChips';
import { SlotApprovalControls } from './SlotHeader';
import type { useWordMutations } from '../../hooks/queries';

type Mutations = ReturnType<typeof useWordMutations>;

const SLOT_HELP: Record<ExampleSlotNumber, string> = {
  1: 'Slot 1 is the mode-1 sentence: shown with four images, the target word highlighted.',
  2: 'Slot 2 appears on the word detail screen in the app.',
  3: 'Slot 3 appears on the word detail screen in the app.',
};

function ExampleCandidateRow({
  candidate,
  occupiedSlot,
  busy,
  onReject,
}: {
  candidate: ExampleCandidate;
  occupiedSlot: number | null;
  busy: boolean;
  onReject: () => void;
}) {
  const rejected = candidate.status === 'rejected';
  return (
    <Card
      size="small"
      className={`morpho-candidate${occupiedSlot !== null ? ' morpho-candidate--selected' : ''}`}
      styles={{ body: { padding: 12, opacity: rejected ? 0.55 : 1 } }}
    >
      <Flex justify="space-between" align="flex-start" gap={12} wrap>
        <Space direction="vertical" size={6} style={{ flex: 1, minWidth: 280 }}>
          <HighlightRange text={candidate.text} start={candidate.hl_start} end={candidate.hl_end} />
          <Space size={8} wrap>
            <SourceBadge source={candidate.source} />
            <ScoreBadge score={candidate.auto_score} detail={candidate.score_detail} />
            <Tag style={{ margin: 0 }}>
              hl [{candidate.hl_start}, {candidate.hl_end})
            </Tag>
            {occupiedSlot !== null && (
              <Tag color="processing" style={{ margin: 0 }}>
                in slot {occupiedSlot}
              </Tag>
            )}
            {rejected && (
              <Tag color="error" style={{ margin: 0 }}>
                rejected
              </Tag>
            )}
            <Typography.Text type="secondary" className="morpho-mono">
              #{candidate.ex_cand_id}
            </Typography.Text>
          </Space>
        </Space>
        <Popconfirm
          title="Reject this example?"
          description="If it is currently selected, the slot falls back to the next best candidate."
          okText="Reject"
          okButtonProps={{ danger: true }}
          onConfirm={onReject}
          disabled={rejected}
        >
          <Button size="small" danger icon={<DeleteOutlined />} disabled={rejected || busy} />
        </Popconfirm>
      </Flex>
    </Card>
  );
}

function truncate(text: string, max = 76): string {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

function MintExampleForm({ mutations }: { mutations: Mutations }) {
  const [form] = Form.useForm<{ text: string; highlight: string }>();
  const [open, setOpen] = useState(false);

  if (!open) {
    return (
      <Button icon={<PlusOutlined />} onClick={() => setOpen(true)}>
        Mint a manual example candidate
      </Button>
    );
  }

  return (
    <Card size="small" title="New manual example">
      <Form
        form={form}
        layout="vertical"
        onFinish={(values) => {
          const text = values.text.trim();
          const start = text.indexOf(values.highlight);
          if (start < 0) {
            form.setFields([
              { name: 'highlight', errors: ['That phrase does not occur in the sentence.'] },
            ]);
            return;
          }
          mutations.mintExample.mutate(
            { text, hl_start: start, hl_end: start + values.highlight.length },
            {
              onSuccess: () => {
                form.resetFields();
                setOpen(false);
              },
            },
          );
        }}
      >
        <Form.Item
          name="text"
          label="Sentence"
          rules={[{ required: true, message: 'Sentence text is required.' }]}
        >
          <Input.TextArea rows={2} placeholder="A shared calendar will facilitate planning." />
        </Form.Item>
        <Form.Item
          name="highlight"
          label="Highlighted span (must occur verbatim in the sentence)"
          rules={[{ required: true, message: 'The highlight phrase is required.' }]}
        >
          <Input style={{ maxWidth: 320 }} placeholder="facilitate" />
        </Form.Item>
        <Space>
          <Button type="primary" htmlType="submit" loading={mutations.mintExample.isPending}>
            Mint candidate
          </Button>
          <Button onClick={() => setOpen(false)}>Cancel</Button>
        </Space>
      </Form>
    </Card>
  );
}

export function ExamplesTab({ detail, mutations }: { detail: WordDetail; mutations: Mutations }) {
  const slotOf = (candId: number): number | null =>
    detail.examples.find((slot) => slot.selection?.ex_cand_id === candId)?.slot ?? null;

  const candidatePool = detail.examples[0]?.candidates ?? [];

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Alert
        type="info"
        showIcon
        message="Three slots, one highlighted span each"
        description="Slot 1 drives mode 1 and is a hard factory gate; slots 2 and 3 are optional, but any slot that is filled must be approved."
      />

      {detail.examples.map((slot) => (
        <Card
          key={slot.slot}
          size="small"
          title={
            <Space size={8}>
              <Tag color={slot.slot === 1 ? 'blue' : 'default'} style={{ margin: 0 }}>
                slot {slot.slot}
              </Tag>
              <Tooltip title={SLOT_HELP[slot.slot]}>
                <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                  {slot.slot === 1 ? 'mode-1 sentence' : 'detail screen'}
                </Typography.Text>
              </Tooltip>
            </Space>
          }
        >
          <Space direction="vertical" size={12} style={{ width: '100%' }}>
            {slot.selection ? (
              <Card size="small" styles={{ body: { padding: 12 } }}>
                {(() => {
                  const selected = slot.candidates.find(
                    (candidate) => candidate.ex_cand_id === slot.selection?.ex_cand_id,
                  );
                  return selected ? (
                    <HighlightRange
                      text={selected.text}
                      start={selected.hl_start}
                      end={selected.hl_end}
                    />
                  ) : (
                    <Typography.Text type="secondary">
                      Selected candidate #{slot.selection.ex_cand_id} is not in the candidate list.
                    </Typography.Text>
                  );
                })()}
              </Card>
            ) : (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={`Slot ${slot.slot} has no selection.`}
              />
            )}
            <Select
              showSearch
              optionFilterProp="label"
              style={{ width: '100%' }}
              placeholder={`Choose the candidate for slot ${slot.slot}`}
              value={slot.selection?.ex_cand_id ?? undefined}
              loading={mutations.select.isPending}
              aria-label={`Candidate for example slot ${slot.slot}`}
              options={candidatePool
                .filter((candidate) => candidate.status === 'available')
                .map((candidate) => ({
                  value: candidate.ex_cand_id,
                  label: `#${candidate.ex_cand_id} · ${candidate.source} · ${truncate(candidate.text)}`,
                }))}
              onChange={(candId: number) =>
                mutations.select.mutate({ kind: 'example', candId, key: { slot: slot.slot } })
              }
            />
            <SlotApprovalControls
              selection={slot.selection}
              busy={mutations.approve.isPending || mutations.unapprove.isPending}
              onApprove={() =>
                mutations.approve.mutate({ kind: 'example', key: { slot: slot.slot } })
              }
              onUnapprove={() =>
                mutations.unapprove.mutate({ kind: 'example', key: { slot: slot.slot } })
              }
            />
          </Space>
        </Card>
      ))}

      <Card size="small" title={`Candidate pool (${candidatePool.length})`}>
        {candidatePool.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No example candidates yet." />
        ) : (
          <Space direction="vertical" size={8} style={{ width: '100%' }}>
            {candidatePool.map((candidate) => (
              <ExampleCandidateRow
                key={candidate.ex_cand_id}
                candidate={candidate}
                occupiedSlot={slotOf(candidate.ex_cand_id)}
                busy={mutations.busy}
                onReject={() =>
                  mutations.reject.mutate({ kind: 'example', candId: candidate.ex_cand_id })
                }
              />
            ))}
          </Space>
        )}
      </Card>

      <MintExampleForm mutations={mutations} />
    </Space>
  );
}
