/**
 * Bulk approval over the word list.
 *
 * At syllabus scale the approval queue is thousands of rows deep, and every one
 * of them is a `POST /selections/{kind}/approve` that the console already knows
 * how to fire one slot at a time. Nothing new is asked of the API: this module
 * only decides, per word, *which* per-slot call to make — and, just as
 * importantly, when to make none at all.
 *
 * The decision needs the word's detail because the definition call is keyed by
 * `pos` and only the detail knows which sense is primary. Reading it first also
 * turns "nothing to do here" into an explicit, reportable skip instead of a 4xx.
 */

import type { ExampleSlotNumber, Pos, WordDetail } from '../../api/types';

/** Which slot a bulk run approves. One entry per approvable asset kind. */
export type BulkApproveKind = 'definition_primary' | 'example_slot_1' | 'image';

export interface BulkApproveAction {
  kind: BulkApproveKind;
  /** Button label; says exactly which slot is touched. */
  label: string;
  /** Sentence shown in the confirmation modal. */
  description: string;
  /** Blocker code whose presence means this word is waiting on this approval. */
  blocker: string;
}

export const BULK_ACTIONS: readonly BulkApproveAction[] = [
  {
    kind: 'definition_primary',
    label: 'Approve primary sense',
    description:
      'Approves the definition selection flagged is_primary. Other enabled senses are left alone.',
    blocker: 'sense_not_approved',
  },
  {
    kind: 'example_slot_1',
    label: 'Approve example slot 1',
    description: 'Approves the slot-1 example — the mode-1 sentence. Slots 2 and 3 are left alone.',
    blocker: 'example_not_approved',
  },
  {
    kind: 'image',
    label: 'Approve image',
    description: 'Approves the live image selection.',
    blocker: 'image_not_approved',
  },
] as const;

export function bulkAction(kind: BulkApproveKind): BulkApproveAction {
  const found = BULK_ACTIONS.find((action) => action.kind === kind);
  if (!found) throw new Error(`Unknown bulk approve kind: ${kind}`);
  return found;
}

/** What `resolveApprovalTarget` decided for one word. */
export type ApprovalTarget =
  | {
      action: 'approve';
      /** `kind` path segment of `POST /selections/{kind}/approve`. */
      selectionKind: 'definition' | 'example' | 'image';
      key: { pos?: Pos; slot?: ExampleSlotNumber };
    }
  | { action: 'skip'; reason: string };

/**
 * Pure decision for one word. Skips are the interesting half: a word with no
 * primary sense, or one somebody already approved, must not be turned into a
 * failed request the operator then has to read past.
 */
export function resolveApprovalTarget(kind: BulkApproveKind, detail: WordDetail): ApprovalTarget {
  if (kind === 'definition_primary') {
    const primary = detail.definitions.find((slot) => slot.selection?.is_primary);
    if (!primary?.selection) {
      return { action: 'skip', reason: 'No primary sense is selected.' };
    }
    if (!primary.selection.enabled) {
      return { action: 'skip', reason: 'The primary sense is disabled.' };
    }
    if (primary.selection.approved) {
      return { action: 'skip', reason: 'Primary sense is already approved.' };
    }
    return { action: 'approve', selectionKind: 'definition', key: { pos: primary.pos } };
  }

  if (kind === 'example_slot_1') {
    const slot = detail.examples.find((entry) => entry.slot === 1);
    if (!slot?.selection) {
      return { action: 'skip', reason: 'Slot 1 has no example selection.' };
    }
    if (slot.selection.approved) {
      return { action: 'skip', reason: 'Slot 1 is already approved.' };
    }
    return { action: 'approve', selectionKind: 'example', key: { slot: 1 } };
  }

  const image = detail.image.selection;
  if (!image) return { action: 'skip', reason: 'No image is selected.' };
  if (image.approved) return { action: 'skip', reason: 'The image is already approved.' };
  return { action: 'approve', selectionKind: 'image', key: {} };
}

/* ------------------------------------------------------------------ */
/* Run state                                                           */
/* ------------------------------------------------------------------ */

export type BulkItemStatus = 'pending' | 'running' | 'approved' | 'skipped' | 'failed';

export interface BulkApproveItem {
  word_id: number;
  lemma: string;
  status: BulkItemStatus;
  /** Skip reason or error message; empty while pending. */
  note: string;
}

export interface BulkApproveProgress {
  kind: BulkApproveKind;
  items: BulkApproveItem[];
  /** Index of the word currently in flight, or `items.length` once finished. */
  cursor: number;
  running: boolean;
  cancelled: boolean;
}

export interface BulkApproveTally {
  total: number;
  approved: number;
  skipped: number;
  failed: number;
  done: number;
}

export function tally(items: readonly BulkApproveItem[]): BulkApproveTally {
  let approved = 0;
  let skipped = 0;
  let failed = 0;
  for (const item of items) {
    if (item.status === 'approved') approved += 1;
    else if (item.status === 'skipped') skipped += 1;
    else if (item.status === 'failed') failed += 1;
  }
  return { total: items.length, approved, skipped, failed, done: approved + skipped + failed };
}
