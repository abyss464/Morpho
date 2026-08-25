import { useEffect, useRef, useState } from 'react';
import { Button, Tooltip } from 'antd';
import { CaretRightOutlined, LoadingOutlined, PauseOutlined } from '@ant-design/icons';
import { mediaUrl } from '../api/client';

/**
 * Plays a content-addressed audio file straight from `GET /api/media/{hash}`.
 * One `<audio>` element per button; the mock serves a real (silent) Ogg/Opus
 * clip so duration, `ended` and error handling all behave for real.
 */
export function AudioButton({
  fileHash,
  disabled,
  title,
  size = 'small',
}: {
  fileHash: string | null;
  disabled?: boolean;
  title?: string;
  size?: 'small' | 'middle' | 'large';
}) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    return () => {
      audioRef.current?.pause();
      audioRef.current = null;
    };
  }, []);

  const unavailable = disabled || !fileHash;

  const toggle = () => {
    if (!fileHash) return;
    if (!audioRef.current) {
      const element = new Audio(mediaUrl(fileHash));
      element.addEventListener('ended', () => setPlaying(false));
      element.addEventListener('error', () => {
        setPlaying(false);
        setLoading(false);
      });
      element.addEventListener('playing', () => setLoading(false));
      audioRef.current = element;
    }
    const audio = audioRef.current;
    if (playing) {
      audio.pause();
      audio.currentTime = 0;
      setPlaying(false);
      return;
    }
    setLoading(true);
    setPlaying(true);
    void audio.play().catch(() => {
      setPlaying(false);
      setLoading(false);
    });
  };

  return (
    <Tooltip title={unavailable ? 'No audio file for this text yet' : (title ?? 'Play audio')}>
      <Button
        size={size}
        type={playing ? 'primary' : 'default'}
        disabled={unavailable}
        aria-label={title ?? 'Play audio'}
        icon={loading ? <LoadingOutlined /> : playing ? <PauseOutlined /> : <CaretRightOutlined />}
        onClick={toggle}
      />
    </Tooltip>
  );
}
