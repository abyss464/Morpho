import { useEffect } from 'react';
import { loadIndex, useAsync } from './api';
import type { Index } from './api';
import { useNow } from './hooks';
import { Home } from './pages/Home';
import { Review } from './pages/Review';
import { StudyView, UnitDone } from './pages/StudyView';
import { UnitPage } from './pages/UnitPage';
import { href, parse, useHash } from './router';
import type { Route } from './router';
import { dueIds, useProgress } from './store';

function TopBar({ route, index }: { route: Route; index: Index | null }) {
  const progress = useProgress();
  const now = useNow(30000);
  const due = index ? dueIds(progress, now).filter((id) => index.pos.has(id)).length : 0;
  const reviewing = route.name === 'review';
  return (
    <header className="bar">
      <a className="brand" href={href.home()}>
        <b>Morpho</b>
        <small>
          Graduate exam English{index ? ` · ${index.words.length.toLocaleString('en-US')} words` : ''}
        </small>
      </a>
      <nav className="modes" aria-label="Mode">
        <a href={href.home()} aria-current={reviewing ? undefined : 'page'}>
          Learn
        </a>
        <a href={href.review()} aria-current={reviewing ? 'page' : undefined}>
          Review
          {due > 0 && (
            <span className="n" aria-label={`${due} due`}>
              {due}
            </span>
          )}
        </a>
      </nav>
    </header>
  );
}

function Page({ route, index }: { route: Route; index: Index }) {
  switch (route.name) {
    case 'home':
      return <Home index={index} />;
    case 'unit':
      return <UnitPage index={index} unit={route.unit} focus={route.focus} />;
    case 'study':
      return <StudyView index={index} unit={route.unit} at={route.index} />;
    case 'done':
      return <UnitDone index={index} unit={route.unit} />;
    case 'review':
      return <Review index={index} />;
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
