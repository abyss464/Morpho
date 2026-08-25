import { useRef, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Col,
  Empty,
  Popconfirm,
  Row,
  Space,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import { CheckOutlined, DeleteOutlined, InboxOutlined } from '@ant-design/icons';
import { mediaUrl } from '../../api/client';
import type { ImageCandidate, WordDetail } from '../../api/types';
import { ScoreBadge, SourceBadge } from '../../components/StatusChips';
import { SlotApprovalControls } from './SlotHeader';
import type { useWordMutations } from '../../hooks/queries';

type Mutations = ReturnType<typeof useWordMutations>;

const MAX_UPLOAD_BYTES = 8 * 1024 * 1024;

function sourceRefLabel(candidate: ImageCandidate): string {
  if (!candidate.source_ref) return '—';
  if (candidate.source === 'sdxl') {
    try {
      const parsed = JSON.parse(candidate.source_ref) as { seed?: number; model?: string };
      return `seed ${parsed.seed ?? '?'} · ${parsed.model ?? 'sdxl'}`;
    } catch {
      return candidate.source_ref;
    }
  }
  return candidate.source_ref;
}

function ImageCard({
  candidate,
  isSelected,
  busy,
  onSelect,
  onReject,
}: {
  candidate: ImageCandidate;
  isSelected: boolean;
  busy: boolean;
  onSelect: () => void;
  onReject: () => void;
}) {
  const rejected = candidate.status === 'rejected';
  return (
    <Card
      size="small"
      className={`morpho-image-card morpho-candidate${isSelected ? ' morpho-candidate--selected' : ''}`}
      styles={{ body: { padding: 12, opacity: rejected ? 0.5 : 1 } }}
      cover={
        <img
          src={mediaUrl(candidate.file_hash)}
          alt={candidate.query_used ?? `candidate ${candidate.img_cand_id}`}
          loading="lazy"
        />
      }
    >
      <Space direction="vertical" size={8} style={{ width: '100%' }}>
        <Space size={6} wrap>
          <SourceBadge source={candidate.source} />
          <ScoreBadge score={candidate.auto_score} detail={candidate.score_detail} />
          {isSelected && (
            <Tag color="blue" style={{ margin: 0 }}>
              live
            </Tag>
          )}
          {rejected && (
            <Tag color="error" style={{ margin: 0 }}>
              rejected
            </Tag>
          )}
        </Space>
        <Tooltip title={candidate.license ?? 'No license recorded'}>
          <Typography.Text type="secondary" ellipsis style={{ fontSize: 12 }}>
            {candidate.license ?? 'unknown license'}
          </Typography.Text>
        </Tooltip>
        <Typography.Text type="secondary" ellipsis className="morpho-mono">
          {candidate.width}×{candidate.height} · {sourceRefLabel(candidate)}
        </Typography.Text>
        <Typography.Text type="secondary" ellipsis className="morpho-mono">
          {candidate.file_hash.slice(0, 18)}
        </Typography.Text>
        <Space size={6}>
          <Button
            size="small"
            icon={<CheckOutlined />}
            disabled={isSelected || rejected || busy}
            onClick={onSelect}
          >
            {isSelected ? 'Live' : 'Use this'}
          </Button>
          <Popconfirm
            title="Reject this image?"
            description="If it is live, selection falls back to the next candidate; with none left, the image lane re-derives."
            okText="Reject"
            okButtonProps={{ danger: true }}
            onConfirm={onReject}
            disabled={rejected}
          >
            <Button size="small" danger icon={<DeleteOutlined />} disabled={rejected || busy} />
          </Popconfirm>
        </Space>
      </Space>
    </Card>
  );
}

function UploadDropzone({ mutations }: { mutations: Mutations }) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const accept = (file: File | undefined) => {
    setError(null);
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      setError(`${file.type || 'That file'} is not an image.`);
      return;
    }
    if (file.size > MAX_UPLOAD_BYTES) {
      setError(`${(file.size / 1_048_576).toFixed(1)} MB exceeds the 8 MB limit.`);
      return;
    }
    mutations.uploadImage.mutate(file);
  };

  return (
    <Card size="small" title="Upload a manual image candidate">
      <div
        className={`morpho-dropzone${dragging ? ' morpho-dropzone--active' : ''}`}
        role="button"
        tabIndex={0}
        onClick={() => inputRef.current?.click()}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            inputRef.current?.click();
          }
        }}
        onDragOver={(event) => {
          event.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(event) => {
          event.preventDefault();
          setDragging(false);
          accept(event.dataTransfer.files[0]);
        }}
      >
        <Space direction="vertical" size={4}>
          <InboxOutlined style={{ fontSize: 26, opacity: 0.6 }} />
          <Typography.Text strong>Drop an image here, or click to choose one</Typography.Text>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            Stored content-addressed: the same bytes uploaded twice occupy one file.
          </Typography.Text>
        </Space>
        <input
          ref={inputRef}
          type="file"
          accept="image/*"
          hidden
          onChange={(event) => {
            accept(event.target.files?.[0]);
            event.target.value = '';
          }}
        />
      </div>
      {error && <Alert type="error" showIcon style={{ marginTop: 12 }} message={error} />}
      {mutations.uploadImage.isPending && (
        <Alert type="info" showIcon style={{ marginTop: 12 }} message="Hashing and storing…" />
      )}
    </Card>
  );
}

export function ImageTab({ detail, mutations }: { detail: WordDetail; mutations: Mutations }) {
  const selectedId = detail.image.selection?.img_cand_id ?? null;

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Card size="small" title="Live image">
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Alert
            type="info"
            showIcon
            message="Exactly one live image per word"
            description="The live image also serves as a distractor picture in other words' questions, so it must be unique and approved."
          />
          <SlotApprovalControls
            selection={detail.image.selection}
            busy={mutations.approve.isPending || mutations.unapprove.isPending}
            onApprove={() => mutations.approve.mutate({ kind: 'image', key: {} })}
            onUnapprove={() => mutations.unapprove.mutate({ kind: 'image', key: {} })}
          />
        </Space>
      </Card>

      {detail.image.candidates.length === 0 ? (
        <Empty description="No image candidates yet — the image lane is still fetching or has dead-lettered." />
      ) : (
        <Row gutter={[12, 12]}>
          {detail.image.candidates.map((candidate) => (
            <Col key={candidate.img_cand_id} xs={24} sm={12} md={8} xl={6}>
              <ImageCard
                candidate={candidate}
                isSelected={candidate.img_cand_id === selectedId}
                busy={mutations.busy}
                onSelect={() =>
                  mutations.select.mutate({
                    kind: 'image',
                    candId: candidate.img_cand_id,
                    key: {},
                  })
                }
                onReject={() =>
                  mutations.reject.mutate({ kind: 'image', candId: candidate.img_cand_id })
                }
              />
            </Col>
          ))}
        </Row>
      )}

      <UploadDropzone mutations={mutations} />
    </Space>
  );
}
