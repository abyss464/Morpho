import { Button, Popconfirm, Space, Tooltip, Typography } from 'antd';
import { CheckOutlined, StopOutlined } from '@ant-design/icons';
import { ApprovalTag, PinnedMark } from '../../components/StatusChips';
import { formatTimestamp } from '../../lib/format';

export interface SlotSelectionMeta {
  selected_by: 'auto' | 'human';
  pinned: boolean;
  approved: boolean;
  approved_by: string | null;
  approved_at: string | null;
  selection_rev: number;
}

/**
 * The approve / un-approve control plus the provenance line shared by every
 * slot. Approval belongs to the (slot, candidate, content) triple, so the
 * approved-by/at line matters as much as the button.
 */
export function SlotApprovalControls({
  selection,
  busy,
  onApprove,
  onUnapprove,
  disabledReason,
}: {
  selection: SlotSelectionMeta | null;
  busy: boolean;
  onApprove: () => void;
  onUnapprove: () => void;
  disabledReason?: string;
}) {
  if (!selection) {
    return (
      <Typography.Text type="secondary" style={{ fontSize: 12 }}>
        Nothing selected in this slot.
      </Typography.Text>
    );
  }

  return (
    <Space size={8} wrap>
      <ApprovalTag approved={selection.approved} />
      <PinnedMark pinned={selection.pinned} />
      <Typography.Text type="secondary" style={{ fontSize: 12 }}>
        {selection.selected_by === 'human' ? 'human override' : 'auto-selected'} · rev{' '}
        {selection.selection_rev}
        {selection.approved && selection.approved_by
          ? ` · by ${selection.approved_by} on ${formatTimestamp(selection.approved_at ?? '')}`
          : ''}
      </Typography.Text>
      {selection.approved ? (
        <Popconfirm
          title="Withdraw approval?"
          description="The word leaves the shippable set until it is approved again."
          okText="Withdraw"
          onConfirm={onUnapprove}
        >
          <Button size="small" icon={<StopOutlined />} loading={busy}>
            Un-approve
          </Button>
        </Popconfirm>
      ) : (
        <Tooltip title={disabledReason}>
          <Button
            size="small"
            type="primary"
            icon={<CheckOutlined />}
            loading={busy}
            disabled={Boolean(disabledReason)}
            onClick={onApprove}
          >
            Approve
          </Button>
        </Tooltip>
      )}
    </Space>
  );
}
