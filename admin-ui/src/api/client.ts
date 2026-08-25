import type { ApiErrorEnvelope, ExportGateFailure } from './types';

export const API_BASE = '/api';

/** Identity sent on mutations so the server can stamp `events.actor = admin:<user>`. */
export const ADMIN_USER = (import.meta.env.VITE_ADMIN_USER as string | undefined) ?? 'admin';

/**
 * Typed failure for every non-2xx response. Carries the decoded error envelope
 * when the server produced one, plus the raw body so callers can pull endpoint
 * specific payloads (export gate failures, for instance).
 */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly body: unknown;

  constructor(status: number, code: string, message: string, body: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.body = body;
  }

  get isConflict(): boolean {
    return this.status === 409;
  }

  get isNotFound(): boolean {
    return this.status === 404;
  }
}

function isErrorEnvelope(value: unknown): value is ApiErrorEnvelope {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = (value as { error?: unknown }).error;
  return (
    typeof candidate === 'object' &&
    candidate !== null &&
    typeof (candidate as { message?: unknown }).message === 'string'
  );
}

/**
 * POST /releases/export answers 409 with the gate failures. The contract says
 * "body lists failures" while the global envelope is `{error:{code,message}}`,
 * so we accept both a top-level `failures` array and `error.details.failures`.
 */
export function extractGateFailures(error: unknown): ExportGateFailure[] {
  if (!(error instanceof ApiError)) return [];
  const body = error.body;
  if (typeof body !== 'object' || body === null) return [];
  const topLevel = (body as { failures?: unknown }).failures;
  if (Array.isArray(topLevel)) return topLevel as ExportGateFailure[];
  const details = (body as ApiErrorEnvelope).error?.details;
  if (typeof details === 'object' && details !== null) {
    const nested = (details as { failures?: unknown }).failures;
    if (Array.isArray(nested)) return nested as ExportGateFailure[];
  }
  return [];
}

export type QueryValue = string | number | boolean | null | undefined;

/** Drops null/undefined/empty values so query strings stay canonical. */
export function buildQuery(params: Record<string, QueryValue> | undefined): string {
  if (!params) return '';
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '') continue;
    search.append(key, String(value));
  }
  const encoded = search.toString();
  return encoded ? `?${encoded}` : '';
}

export interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  query?: Record<string, QueryValue>;
  json?: unknown;
  formData?: FormData;
  signal?: AbortSignal;
}

async function decodeBody(response: Response): Promise<unknown> {
  const type = response.headers.get('content-type') ?? '';
  if (!type.includes('json')) {
    const text = await response.text();
    return text.length > 0 ? text : null;
  }
  try {
    return await response.json();
  } catch {
    return null;
  }
}

/**
 * Thin fetch wrapper: builds the URL, sends/receives JSON, and converts every
 * non-2xx response into an `ApiError` carrying the decoded error envelope.
 */
export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { method = 'GET', query, json, formData, signal } = options;

  const headers: Record<string, string> = { Accept: 'application/json' };
  let body: BodyInit | undefined;

  if (formData) {
    body = formData;
  } else if (json !== undefined) {
    headers['Content-Type'] = 'application/json';
    body = JSON.stringify(json);
  }
  if (method !== 'GET') headers['X-Admin-User'] = ADMIN_USER;

  const response = await fetch(`${API_BASE}${path}${buildQuery(query)}`, {
    method,
    headers,
    body,
    signal,
  });

  const decoded = await decodeBody(response);

  if (!response.ok) {
    if (isErrorEnvelope(decoded)) {
      throw new ApiError(
        response.status,
        decoded.error.code || String(response.status),
        decoded.error.message,
        decoded,
      );
    }
    throw new ApiError(
      response.status,
      String(response.status),
      typeof decoded === 'string' && decoded.length > 0
        ? decoded
        : `${response.status} ${response.statusText}`,
      decoded,
    );
  }

  return decoded as T;
}

/** Absolute URL for a content-addressed media file: `GET /api/media/{file_hash}`. */
export function mediaUrl(fileHash: string): string {
  return `${API_BASE}/media/${fileHash}`;
}
