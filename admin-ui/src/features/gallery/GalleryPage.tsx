import { useCallback, useEffect, useRef, useState } from 'react';
import { Link } from '@tanstack/react-router';
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
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
  Switch,
  Tag,
  Typography,
} from 'antd';
import { DeleteOutlined, EyeOutlined } from '@ant-design/icons';
import { mediaUrl } from '../../api/client';
import * as api from '../../api/endpoints';
import { qk } from '../../api/queryKeys';
import type { GalleryItem, GalleryQuery, GallerySort, ImageSource } from '../../api/types';
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

const SORT_OPTIONS: { value: GallerySort; label: string }[] = [
  { value: 'clip_asc', label: 'Worst match first' },
  { value: 'clip_desc', label: 'Best match first' },
];

export interface GallerySearch {
  source?: ImageSource;
  approved?: 'true' | 'false';
  q?: string;
  sort?: GallerySort;
}

export interface GalleryPageProps {
  search: GallerySearch;
  onSearchChange: (next: GallerySearch) => void;
}

/** `{"<word_id>": "<中文>"}` — machine-translated slot-1 sentences, pre-generated
 * offline (ops/translate_sentences.py) and bundled as a static asset so review
 * mode never depends on a live translation call. Fetched once and cached for
 * the session; a fetch failure just means the Chinese line reads "—". */
function useZhSentences() {
  const query = useQuery({
    queryKey: ['sentence-zh'],
    queryFn: async ({ signal }) => {
      const response = await fetch('/sentence-zh.json', { signal });
      if (!response.ok) return {} as Record<string, string>;
      return (await response.json()) as Record<string, string>;
    },
    staleTime: Infinity,
    gcTime: Infinity,
    retry: false,
  });
  return query.data ?? {};
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
      cover={<img src={mediaUrl(item.file_hash)} alt={item.lemma} loading="lazy" />}
    >
      <Space direction="vertical" size={6} style={{ width: '100%' }}>
        <Link
          to="/words/$wordId"
          params={{ wordId: String(item.word_id) }}
          search={{ tab: 'image' }}
        >
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
          <Tag
            color={item.clip_similarity !== null ? 'blue' : 'default'}
            title="CLIP similarity to the word's own sentence"
            style={{ margin: 0 }}
          >
            {item.clip_similarity !== null ? item.clip_similarity.toFixed(3) : '—'}
          </Tag>
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
          <Link
            to="/words/$wordId"
            params={{ wordId: String(item.word_id) }}
            search={{ tab: 'image' }}
          >
            <Button size="small" icon={<EyeOutlined />}>
              View
            </Button>
          </Link>
        </Flex>
      </Space>
    </Card>
  );
}

/** One row of "review mode": image left, word + both-language sentence +
 * clip score + actions right — built for fast worst-first triage rather than
 * the grid's dense browsing (backlog: gallery review mode). */
function GalleryReviewRow({
  item,
  zhSentence,
  onReject,
  rejecting,
}: {
  item: GalleryItem;
  zhSentence: string | undefined;
  onReject: () => void;
  rejecting: boolean;
}) {
  return (
    <Card size="small" className="morpho-gallery-review-row" styles={{ body: { padding: 12 } }}>
      <Flex gap={16} align="flex-start">
        <img
          src={mediaUrl(item.file_hash)}
          alt={item.lemma}
          loading="lazy"
          className="morpho-gallery-review-thumb"
        />
        <Flex vertical gap={6} flex="1 1 auto" style={{ minWidth: 0 }}>
          <Flex justify="space-between" align="center" wrap gap={8}>
            <Space size={8} wrap>
              <Link
                to="/words/$wordId"
                params={{ wordId: String(item.word_id) }}
                search={{ tab: 'image' }}
              >
                <Typography.Text strong>{item.lemma}</Typography.Text>
              </Link>
              <SourceBadge source={item.source} />
              <ApprovalTag approved={item.approved} />
            </Space>
            <Tag
              color={item.clip_similarity !== null ? 'blue' : 'default'}
              title="CLIP similarity to the word's own sentence"
              style={{ margin: 0 }}
            >
              {item.clip_similarity !== null ? item.clip_similarity.toFixed(3) : '—'}
            </Tag>
          </Flex>
          <Typography.Text>
            {item.slot1_sentence ?? (
              <Typography.Text type="secondary" italic>
                No slot-1 sentence selected.
              </Typography.Text>
            )}
          </Typography.Text>
          <Typography.Text type="secondary">{zhSentence ?? '—'}</Typography.Text>
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
            <Link
              to="/words/$wordId"
              params={{ wordId: String(item.word_id) }}
              search={{ tab: 'image' }}
            >
              <Button size="small" icon={<EyeOutlined />}>
                View
              </Button>
            </Link>
          </Flex>
        </Flex>
      </Flex>
    </Card>
  );
}

export function GalleryPage({ search, onSearchChange }: GalleryPageProps) {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const sentinelRef = useRef<HTMLDivElement>(null);
  const [reviewMode, setReviewMode] = useState(false);
  const zhSentences = useZhSentences();

  const apiQuery: GalleryQuery = {
    page: 1,
    page_size: PAGE_SIZE,
    source: search.source,
    approved: search.approved === undefined ? undefined : search.approved === 'true',
    q: search.q,
    sort: search.sort,
  };

  const { data, fetchNextPage, hasNextPage, isFetchingNextPage, isPending, isError, error } =
    useInfiniteQuery({
      queryKey: qk.galleryList(apiQuery),
      queryFn: ({ pageParam, signal }) => api.listGallery({ ...apiQuery, page: pageParam }, signal),
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
            <Typography.Text
              type="secondary"
              style={{ fontSize: 14, fontWeight: 400, marginLeft: 10 }}
            >
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
          <Select
            allowClear
            placeholder="Sort"
            style={{ width: 180 }}
            value={search.sort}
            options={SORT_OPTIONS}
            onChange={(value) => patch({ sort: value as GallerySearch['sort'] })}
            aria-label="Sort by semantic match"
          />
          <Flex align="center" gap={8} style={{ marginLeft: 'auto' }}>
            <Switch checked={reviewMode} onChange={setReviewMode} aria-label="Toggle review mode" />
            <Typography.Text>Review mode</Typography.Text>
          </Flex>
        </Flex>
      </Card>

      {isPending ? (
        <Flex justify="center" style={{ padding: 60 }}>
          <Spin size="large" />
        </Flex>
      ) : isError ? (
        <Empty description={errorMessage(error)} />
      ) : allItems.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No images match these filters." />
      ) : (
        <>
          {reviewMode ? (
            <Space direction="vertical" size={10} style={{ width: '100%' }}>
              {allItems.map((item) => (
                <GalleryReviewRow
                  key={item.img_cand_id}
                  item={item}
                  zhSentence={zhSentences[String(item.word_id)]}
                  onReject={() => rejectMutation.mutate(item.img_cand_id)}
                  rejecting={
                    rejectMutation.isPending && rejectMutation.variables === item.img_cand_id
                  }
                />
              ))}
            </Space>
          ) : (
            <Row gutter={[12, 12]}>
              {allItems.map((item) => (
                <Col key={item.img_cand_id} xs={12} sm={8} md={6} xl={4} xxl={3}>
                  <GalleryCard
                    item={item}
                    onReject={() => rejectMutation.mutate(item.img_cand_id)}
                    rejecting={
                      rejectMutation.isPending && rejectMutation.variables === item.img_cand_id
                    }
                  />
                </Col>
              ))}
            </Row>
          )}
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
