import { useEffect } from 'react';
import { loadIndex, useAsync } from './api';
import type { Index } from './api';
import { Stream } from './pages/Stream';
import { Today } from './pages/Today';
import { UnitPage } from './pages/UnitPage';
import { href, parse, useHash } from './router';
import type { Route } from './router';
import { useProgress } from './store';
import { streak, streamProgress } from './stream';

function TopBar({ route, index }: { route: Route; index: Index | null }) {
  const progress = useProgress();
  if (route.name === 'stream' && index) {
    const { done, total } = streamProgress(progress, index);
    return (
      <header className="bar">
        <a className="brand" href={href.home()}>
          <b>Morpho</b>
        </a>
        <div className="meter">
          <span>Today</span>
          <div className="bar-track" role="progressbar" aria-label="Today's stream" aria-valuenow={done} aria-valuemax={total}>
            <i style={{ width: `${(done / Math.max(1, total)) * 100}%` }} />
          </div>
          <span className="count">
            {done} / {total}
          </span>
          <a className="btn pill" href={href.home()}>
            Pause
          </a>
        </div>
      </header>
    );
  }
  const days = streak(progress);
  return (
    <header className="bar">
      <a className="brand" href={href.home()}>
        <b>Morpho</b>
        <small>Graduate exam English{index ? ` · ${index.words.length.toLocaleString('en-US')} words` : ''}</small>
      </a>
      {days > 0 && (
        <span className="streak">
          <b>{days}</b> day streak
        </span>
      )}
    </header>
  );
}

function Page({ route, index }: { route: Route; index: Index }) {
  switch (route.name) {
    case 'home':
      return <Today index={index} />;
    case 'stream':
      return <Stream index={index} />;
    case 'unit':
      return <UnitPage index={index} unit={route.unit} focus={route.focus} />;
  }
}

export function App() {
  const hash = useHash();
  const route = parse(hash);
  const index = useAsync(loadIndex, []);
  const focus = route.name === 'unit' ? route.focus : null;

  useEffect(() => {
    if (focus === null) window.scrollTo(0, 0);
  }, [hash, focus]);

  return (
    <div className="app">
      <TopBar route={route} index={index.status === 'ok' ? index.data : null} />
      <main>
        {index.status === 'loading' && <p className="loading">Loading the word list&hellip;</p>}
        {index.status === 'error' && (
          <section className="notice">
            <h1>Could not load the release.</h1>
            <p>{index.error}</p>
            <p>
              The words come from the newest bundle in data/releases/ (or the one named by MORPHO_RELEASE), served by
              this app's dev server.
            </p>
            <button type="button" className="btn primary" onClick={() => window.location.reload()}>
              Try again
            </button>
          </section>
        )}
        {index.status === 'ok' && (
          <div className="stack">
            <Page route={route} index={index.data} />
          </div>
        )}
      </main>
      {index.status === 'ok' && (
        <footer className="foot">
          Release {index.data.contentVersion || index.data.release}
          {index.data.exportedAt ? ` · exported ${index.data.exportedAt}` : ''}
        </footer>
      )}
    </div>
  );
}
