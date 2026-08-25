import type { ReactNode } from 'react';
import type { UseQueryResult } from '@tanstack/react-query';
import { Button, Empty, Result, Skeleton } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import { ApiError } from '../api/client';
import { errorCode, errorMessage } from '../lib/errors';

export interface QueryStateProps<T> {
  query: UseQueryResult<T>;
  children: (data: T) => ReactNode;
  /** Rendered when the resolved data is considered empty. */
  isEmpty?: (data: T) => boolean;
  emptyText?: ReactNode;
  emptyExtra?: ReactNode;
  skeleton?: ReactNode;
  skeletonRows?: number;
}

/**
 * Single place where loading / error / empty are decided, so every page gets
 * the same three states without repeating the branches.
 */
export function QueryState<T>({
  query,
  children,
  isEmpty,
  emptyText = 'Nothing here yet.',
  emptyExtra,
  skeleton,
  skeletonRows = 5,
}: QueryStateProps<T>) {
  if (query.isPending) {
    return <>{skeleton ?? <Skeleton active paragraph={{ rows: skeletonRows }} />}</>;
  }

  if (query.isError) {
    const code = errorCode(query.error);
    return (
      <Result
        status={query.error instanceof ApiError && query.error.status === 404 ? '404' : 'error'}
        title={code ? `Request failed (${code})` : 'Request failed'}
        subTitle={errorMessage(query.error)}
        extra={
          <Button icon={<ReloadOutlined />} onClick={() => void query.refetch()}>
            Retry
          </Button>
        }
      />
    );
  }

  const data = query.data as T;
  if (isEmpty?.(data)) {
    return <Empty description={emptyText}>{emptyExtra}</Empty>;
  }

  return <>{children(data)}</>;
}
