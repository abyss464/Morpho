import { Typography } from 'antd';

/** Renders `text` with `[start, end)` wrapped in the highlight span. */
export function HighlightRange({
  text,
  start,
  end,
  className = 'morpho-hl',
}: {
  text: string;
  start: number;
  end: number;
  className?: string;
}) {
  if (end <= start || start < 0 || end > text.length) {
    return <Typography.Text>{text}</Typography.Text>;
  }
  return (
    <Typography.Text>
      {text.slice(0, start)}
      <span className={className}>{text.slice(start, end)}</span>
      {text.slice(end)}
    </Typography.Text>
  );
}

/** Renders `text` with every whole-word occurrence of `token` highlighted. */
export function HighlightToken({
  text,
  token,
  className = 'morpho-oos-hl',
}: {
  text: string;
  token: string;
  className?: string;
}) {
  if (!token) return <Typography.Text>{text}</Typography.Text>;
  const escaped = token.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const parts = text.split(new RegExp(`(\\b${escaped}\\b)`, 'ig'));
  return (
    <Typography.Text>
      {parts.map((part, index) =>
        part.toLowerCase() === token.toLowerCase() ? (
          // Index keys are correct here: the array is a positional split of one
          // immutable string and never reorders.
          <span key={`${index}-${part}`} className={className}>
            {part}
          </span>
        ) : (
          part
        ),
      )}
    </Typography.Text>
  );
}
