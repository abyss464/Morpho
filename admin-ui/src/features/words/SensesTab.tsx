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
  Switch,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import { DeleteOutlined, PlusOutlined, SelectOutlined, StarOutlined } from '@ant-design/icons';
import type { DefinitionCandidate, DefinitionSlotView, Pos, WordDetail } from '../../api/types';
import { PrimaryMark, ScoreBadge, SourceBadge } from '../../components/StatusChips';
import { SlotApprovalControls } from './SlotHeader';
import type { useWordMutations } from '../../hooks/queries';

const POS_OPTIONS: Pos[] = ['noun', 'verb', 'adj', 'adv', 'prep', 'conj', 'interj', 'phrase'];

type Mutations = ReturnType<typeof useWordMutations>;

function CandidateRow({
  candidate,
  isSelected,
  busy,
  onSelect,
  onReject,
}: {
  candidate: DefinitionCandidate;
  isSelected: boolean;
  busy: boolean;
  onSelect: () => void;
  onReject: () => void;
}) {
  const rejected = candidate.status === 'rejected';
  return (
    <Card
      size="small"
      className={`morpho-candidate${isSelected ? ' morpho-candidate--selected' : ''}`}
      styles={{ body: { padding: 12, opacity: rejected ? 0.55 : 1 } }}
    >
      <Flex justify="space-between" align="flex-start" gap={12} wrap>
        <Space direction="vertical" size={6} style={{ flex: 1, minWidth: 260 }}>
          <Typography.Text delete={rejected}>{candidate.text}</Typography.Text>
          <Space size={8} wrap>
            <SourceBadge source={candidate.source} />
            <ScoreBadge score={candidate.auto_score} detail={candidate.score_detail} />
            {candidate.parent_cand_id !== null && (
              <Tooltip title={`Rewritten from candidate #${candidate.parent_cand_id}`}>
                <Tag style={{ margin: 0 }}>rewrite of #{candidate.parent_cand_id}</Tag>
              </Tooltip>
            )}
            {rejected && (
              <Tag color="error" style={{ margin: 0 }}>
                rejected
              </Tag>
            )}
            <Typography.Text type="secondary" className="morpho-mono">
              #{candidate.def_cand_id} · {candidate.text_hash.slice(0, 10)}
            </Typography.Text>
          </Space>
        </Space>
        <Space size={6}>
          <Tooltip title="Pin this candidate as the human-chosen selection">
            <Button
              size="small"
              icon={<SelectOutlined />}
              disabled={isSelected || rejected || busy}
              onClick={onSelect}
            >
              {isSelected ? 'Selected' : 'Select'}
            </Button>
          </Tooltip>
          <Popconfirm
            title="Reject this candidate?"
            description="Candidates are immutable; rejecting hides it and re-runs auto-selection."
            okText="Reject"
            okButtonProps={{ danger: true }}
            onConfirm={onReject}
            disabled={rejected}
          >
            <Button size="small" danger icon={<DeleteOutlined />} disabled={rejected || busy} />
          </Popconfirm>
        </Space>
      </Flex>
    </Card>
  );
}

function SenseSlot({
  slot,
  wordId,
  mutations,
  hasOosBlocker,
}: {
  slot: DefinitionSlotView;
  wordId: number;
  mutations: Mutations;
  hasOosBlocker: boolean;
}) {
  const selectedId = slot.selection?.def_cand_id ?? null;
  const enabled = slot.selection?.enabled ?? true;

  return (
    <Card
      size="small"
      title={
        <Space size={10} wrap>
          <Tag color="blue" style={{ margin: 0 }}>
            {slot.pos}
          </Tag>
          <PrimaryMark isPrimary={slot.selection?.is_primary ?? false} />
          {!slot.selection?.is_primary && slot.selection && (
            <Button
              size="small"
              type="text"
              icon={<StarOutlined />}
              loading={mutations.setPrimary.isPending}
              onClick={() => mutations.setPrimary.mutate(slot.pos)}
            >
              Make primary
            </Button>
          )}
        </Space>
      }
      extra={
        slot.selection && (
          <Space size={8}>
            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
              enabled
            </Typography.Text>
            <Switch
              size="small"
              checked={enabled}
              loading={mutations.setEnabled.isPending}
              aria-label={`Toggle ${slot.pos} sense`}
              onChange={(next) => mutations.setEnabled.mutate({ pos: slot.pos, enabled: next })}
            />
          </Space>
        )
      }
    >
      <Space direction="vertical" size={12} style={{ width: '100%' }}>
        <SlotApprovalControls
          selection={slot.selection}
          busy={mutations.approve.isPending || mutations.unapprove.isPending}
          disabledReason={
            hasOosBlocker
              ? 'Resolve the out-of-scope token first; approval would be invalidated immediately.'
              : undefined
          }
          onApprove={() => mutations.approve.mutate({ kind: 'definition', key: { pos: slot.pos } })}
          onUnapprove={() =>
            mutations.unapprove.mutate({ kind: 'definition', key: { pos: slot.pos } })
          }
        />
        {slot.candidates.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No candidates fetched yet." />
        ) : (
          <Space direction="vertical" size={8} style={{ width: '100%' }}>
            {slot.candidates.map((candidate) => (
              <CandidateRow
                key={candidate.def_cand_id}
                candidate={candidate}
                isSelected={candidate.def_cand_id === selectedId}
                busy={mutations.busy}
                onSelect={() =>
                  mutations.select.mutate({
                    kind: 'definition',
                    candId: candidate.def_cand_id,
                    key: { pos: slot.pos },
                  })
                }
                onReject={() =>
                  mutations.reject.mutate({ kind: 'definition', candId: candidate.def_cand_id })
                }
              />
            ))}
          </Space>
        )}
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          word #{wordId} · slot ({wordId}, {slot.pos})
        </Typography.Text>
      </Space>
    </Card>
  );
}

function MintDefinitionForm({ mutations }: { mutations: Mutations }) {
  const [form] = Form.useForm<{ pos: Pos; text: string }>();
  const [open, setOpen] = useState(false);

  if (!open) {
    return (
      <Button icon={<PlusOutlined />} onClick={() => setOpen(true)}>
        Mint a manual definition candidate
      </Button>
    );
  }

  return (
    <Card size="small" title="New manual candidate">
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 12 }}
        message="Editing means minting"
        description="Candidates are immutable. A manual edit inserts a new candidate and points the selection at it; the old row stays for audit."
      />
      <Form
        form={form}
        layout="vertical"
        onFinish={(values) => {
          mutations.mintDefinition.mutate(
            { pos: values.pos, text: values.text.trim() },
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
          name="pos"
          label="Part of speech"
          rules={[{ required: true, message: 'Pick a part of speech.' }]}
        >
          <Select
            style={{ maxWidth: 220 }}
            options={POS_OPTIONS.map((pos) => ({ value: pos, label: pos }))}
            placeholder="pos"
          />
        </Form.Item>
        <Form.Item
          name="text"
          label="Definition text (English only, base + target vocabulary)"
          rules={[
            { required: true, message: 'Definition text is required.' },
            { min: 8, message: 'Too short to be a usable definition.' },
          ]}
        >
          <Input.TextArea rows={3} placeholder="to make an action or process easier" />
        </Form.Item>
        <Space>
          <Button type="primary" htmlType="submit" loading={mutations.mintDefinition.isPending}>
            Mint candidate
          </Button>
          <Button onClick={() => setOpen(false)}>Cancel</Button>
        </Space>
      </Form>
    </Card>
  );
}

export function SensesTab({ detail, mutations }: { detail: WordDetail; mutations: Mutations }) {
  const hasOos = detail.word.blockers.includes('oos_pending');

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      {hasOos && (
        <Alert
          type="warning"
          showIcon
          message="A selected definition contains an out-of-scope token"
          description="The word stays blocked until the token is rewritten away or promoted to an auxiliary word. Open the OOV queue to resolve it."
        />
      )}
      {detail.definitions.length === 0 ? (
        <Empty description="No definition candidates have been fetched for this word yet." />
      ) : (
        detail.definitions.map((slot) => (
          <SenseSlot
            key={slot.pos}
            slot={slot}
            wordId={detail.word.word_id}
            mutations={mutations}
            hasOosBlocker={hasOos}
          />
        ))
      )}
      <MintDefinitionForm mutations={mutations} />
    </Space>
  );
}
