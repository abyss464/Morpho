import { ApiError } from '../api/client';

/** Human-readable text for anything thrown by the API client or React. */
export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  if (error instanceof Error) return error.message;
  return 'Unexpected failure.';
}

export function errorCode(error: unknown): string | undefined {
  return error instanceof ApiError ? error.code : undefined;
}
