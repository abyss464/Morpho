import { useCallback, useEffect, useRef } from 'react';
import { Link } from '@tanstack/react-router';
import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  App,
  Button,
  Card,
  Col,
  Empty,
  Flex,
  Input,
  Popconfirm,
  Row,
  Select,
  Space,
  Spin,
  Tag,
  Typography,
} from 'antd';
import { DeleteOutlined, EyeOutlined } from '@ant-design/icons';
import { mediaUrl } from '../../api/client';
import * as api from '../../api/endpoints';
import { qk } from '../../api/queryKeys';
import type { GalleryItem, GalleryQuery, ImageSource } from '../../api/types';
import { ApprovalTag, SourceBadge } from '../../components/StatusChips';
import { errorMessage } from '../../lib/errors';
import './GalleryPage.css';

const PAGE_SIZE = 60;

const SOURCE_OPTIONS: { value: ImageSource; label: string }[] = [
  { value: 'unsplash', label: 'unsplash' },
  { value: 'pexels', label: 'pexels' },
  { value: 'pixabay', label: 'pixabay' },
  { value: 'wikimedia', label: 'wikimedia' },
  { value: 'openverse', label: 'openverse' },
  { value: 'sdxl', label: 'sdxl' },
  { value: 'codex', label: 'codex' },
  { value: 'manual', label: 'manual' },
];

const APPROVED_OPTIONS = [
  { value: 'true', label: 'Approved' },
  { value: 'false', label: 'Unapproved' },
];

export interface GallerySearch {
  source?: ImageSource;
  approved?: 'true' | 'false';
  q?: string;
}

export interface GalleryPageProps {
  search: GallerySearch;
  onSearchChange: (next: GallerySearch) => void;
}

function GalleryCard({
  item,
  onReject,
  rejecting,
}: {
  item: GalleryItem;
  onReject: () => void;
  rejecting: boolean;
}) {
  return (
    <Card
      hoverable
      size="small"
      className="morpho-gallery-card"
      cover={
        <img
          src={mediaUrl(item.file_hash)}
          alt={item.lemma}
          loading="lazy"
        />
      }
    >
      <Space direction="vertical" size={6} style={{ width: '100%' }}>
        <Link to="/words/$wordId" params={{ wordId: String(item.word_id) }} search={{ tab: 'image' }}>
          <Typography.Text strong>{item.lemma}</Typography.Text>
        </Link>
        <Space size={4} wrap>
          <SourceBadge source={item.source} />
          <ApprovalTag approved={item.approved} />
          {item.pinned && (
            <Tag color="gold" style={{ margin: 0 }}>
              pinned
            </Tag>
          )}
          {item.auto_score !== null && (
            <Tag style={{ margin: 0 }}>{item.auto_score.toFixed(2)}</Tag>
          )}
        </Space>
        <Flex gap={6}>
          <Popconfirm
            title="Reject this image?"
            description="Selection falls back to the next candidate; with none left, the image lane re-derives."
            okText="Reject"
            okButtonProps={{ danger: true }}
            onConfirm={onReject}
          >
            <Button size="small" danger icon={<DeleteOutlined />} loading={rejecting}>
              Reject
            </Button>
          </Popconfirm>
          <Link to="/words/$wordId" params={{ wordId: String(item.word_id) }} search={{ tab: 'image' }}>
            <Button size="small" icon={<EyeOutlined />}>
              View
            </Button>
          </Link>
        </Flex>
      </Space>
    </Card>
  );
}

export function GalleryPage({ search, onSearchChange }: GalleryPageProps) {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const sentinelRef = useRef<HTMLDivElement>(null);

  const apiQuery: GalleryQuery = {
    page: 1,
    page_size: PAGE_SIZE,
    source: search.source,
    approved: search.approved === undefined ? undefined : search.approved === 'true',
    q: search.q,
  };

  const {
    data,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isPending,
    isError,
    error,
  } = useInfiniteQuery({
    queryKey: qk.galleryList(apiQuery),
    queryFn: ({ pageParam, signal }) =>
      api.listGallery({ ...apiQuery, page: pageParam }, signal),
    initialPageParam: 1,
    getNextPageParam: (lastPage, allPages) => {
      const loaded = allPages.reduce((sum, page) => sum + page.items.length, 0);
      return loaded < lastPage.total ? allPages.length + 1 : undefined;
    },
  });

  const rejectMutation = useMutation({
    mutationFn: (candId: number) => api.rejectCandidate('image', candId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: qk.gallery() });
      void queryClient.invalidateQueries({ queryKey: qk.words() });
      void queryClient.invalidateQueries({ queryKey: qk.dashboard() });
      message.success('Image rejected.');
    },
    onError: (err) => message.error(errorMessage(err)),
  });

  const handleIntersect = useCallback(
    (entries: IntersectionObserverEntry[]) => {
      if (entries[0]?.isIntersecting && hasNextPage && !isFetchingNextPage) {
        void fetchNextPage();
      }
    },
    [hasNextPage, isFetchingNextPage, fetchNextPage],
  );

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(handleIntersect, { rootMargin: '400px' });
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [handleIntersect]);

  const patch = (partial: Partial<GallerySearch>) => onSearchChange({ ...search, ...partial });

  const allItems = data?.pages.flatMap((page) => page.items) ?? [];
  const total = data?.pages[0]?.total ?? 0;

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Flex justify="space-between" align="flex-end" wrap gap={12}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          Image gallery
          {total > 0 && (
            <Typography.Text type="secondary" style={{ fontSize: 14, fontWeight: 400, marginLeft: 10 }}>
              {total} image{total !== 1 ? 's' : ''}
            </Typography.Text>
          )}
        </Typography.Title>
      </Flex>

      <Card size="small" styles={{ body: { padding: 12 } }}>
        <Flex gap={10} wrap align="center">
          <Select
            allowClear
            placeholder="Source"
            style={{ width: 150 }}
            value={search.source}
            options={SOURCE_OPTIONS}
            onChange={(value) => patch({ source: value as ImageSource | undefined })}
            aria-label="Filter by source"
          />
          <Select
            allowClear
            placeholder="Approval"
            style={{ width: 150 }}
            value={search.approved}
            options={APPROVED_OPTIONS}
            onChange={(value) => patch({ approved: value as GallerySearch['approved'] })}
            aria-label="Filter by approval"
          />
          <Input.Search
            allowClear
            placeholder="Search lemma"
            defaultValue={search.q}
            style={{ width: 240 }}
            onSearch={(value) => patch({ q: value.trim() || undefined })}
            aria-label="Filter by lemma"
          />
        </Flex>
      </Card>

      {isPending ? (
        <Flex justify="center" style={{ padding: 60 }}>
          <Spin size="large" />
        </Flex>
      ) : isError ? (
        <Empty description={errorMessage(error)} />
      ) : allItems.length === 0 ? (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description="No images match these filters."
        />
      ) : (
        <>
          <Row gutter={[12, 12]}>
            {allItems.map((item) => (
              <Col key={item.img_cand_id} xs={12} sm={8} md={6} xl={4} xxl={3}>
                <GalleryCard
                  item={item}
                  onReject={() => rejectMutation.mutate(item.img_cand_id)}
                  rejecting={
                    rejectMutation.isPending &&
                    rejectMutation.variables === item.img_cand_id
                  }
                />
              </Col>
            ))}
          </Row>
          {isFetchingNextPage && (
            <Flex justify="center" style={{ padding: 20 }}>
              <Spin />
            </Flex>
          )}
          <div ref={sentinelRef} className="morpho-gallery-sentinel" />
        </>
      )}
    </Space>
  );
}
