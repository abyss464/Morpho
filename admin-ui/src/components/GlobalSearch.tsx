import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { AutoComplete, Input, Space, Tag, Typography } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import { useWordList } from '../hooks/queries';
import { ReadyBadge } from './StatusChips';

function useDebounced<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [value, delayMs]);
  return debounced;
}

/**
 * Jump-to-word search in the header. `/` focuses it from anywhere, Enter opens
 * the highlighted result — the console is meant to be driven from the keyboard.
 */
export function GlobalSearch() {
  const navigate = useNavigate();
  const [term, setTerm] = useState('');
  const debounced = useDebounced(term.trim(), 200);

  // An empty box asks for nothing: `page_size: 0` still round-trips to morphod,
  // which clamps it to 1 and answers with a row nobody reads.
  const query = useWordList({ q: debounced, page: 1, page_size: 8 }, debounced.length > 0);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || target?.isContentEditable) return;
      event.preventDefault();
      document.getElementById('morpho-global-search')?.focus();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const options = useMemo(() => {
    if (debounced.length === 0) return [];
    return (query.data?.items ?? []).map((item) => ({
      value: String(item.word_id),
      label: (
        <Space style={{ width: '100%', justifyContent: 'space-between' }}>
          <Space size={8}>
            <Typography.Text strong>{item.lemma}</Typography.Text>
            <Tag style={{ margin: 0 }}>{item.role}</Tag>
          </Space>
          <ReadyBadge ready={item.ready} />
        </Space>
      ),
    }));
  }, [query.data, debounced]);

  const openWord = (wordId: string) => {
    setTerm('');
    void navigate({ to: '/words/$wordId', params: { wordId } });
  };

  return (
    <AutoComplete
      value={term}
      options={options}
      onSearch={setTerm}
      onSelect={openWord}
      style={{ width: 320 }}
      notFoundContent={
        debounced.length > 0 && !query.isFetching ? (
          <Typography.Text type="secondary">No word matches “{debounced}”.</Typography.Text>
        ) : null
      }
    >
      <Input
        id="morpho-global-search"
        allowClear
        prefix={<SearchOutlined />}
        suffix={
          <Typography.Text type="secondary" style={{ fontSize: 11 }}>
            /
          </Typography.Text>
        }
        placeholder="Jump to a word"
        aria-label="Search words"
        onPressEnter={() => {
          const first = query.data?.items[0];
          if (first) openWord(String(first.word_id));
        }}
      />
    </AutoComplete>
  );
}
