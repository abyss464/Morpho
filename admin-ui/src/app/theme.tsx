import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { ConfigProvider, App as AntdApp, theme as antdTheme } from 'antd';
import enUS from 'antd/locale/en_US';

export type ThemeMode = 'light' | 'dark';

const STORAGE_KEY = 'morpho.admin.theme';

interface ThemeContextValue {
  mode: ThemeMode;
  toggle: () => void;
  setMode: (mode: ThemeMode) => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  mode: 'light',
  toggle: () => {},
  setMode: () => {},
});

// eslint-disable-next-line react-refresh/only-export-components
export function useThemeMode(): ThemeContextValue {
  return useContext(ThemeContext);
}

function initialMode(): ThemeMode {
  if (typeof window === 'undefined') return 'light';
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'dark') return stored;
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/** Brand tokens shared by both algorithms; only the algorithm changes on toggle. */
const SHARED_TOKENS = {
  colorPrimary: '#3b6fd4',
  colorInfo: '#3b6fd4',
  colorSuccess: '#2f9e63',
  colorWarning: '#d99328',
  colorError: '#d4443b',
  borderRadius: 6,
  fontFamily:
    "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif",
};

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [mode, setMode] = useState<ThemeMode>(initialMode);

  useEffect(() => {
    window.localStorage.setItem(STORAGE_KEY, mode);
    document.documentElement.dataset.theme = mode;
    document.documentElement.style.colorScheme = mode;
  }, [mode]);

  const toggle = useCallback(
    () => setMode((current) => (current === 'dark' ? 'light' : 'dark')),
    [],
  );
  const value = useMemo<ThemeContextValue>(() => ({ mode, toggle, setMode }), [mode, toggle]);

  return (
    <ThemeContext.Provider value={value}>
      <ConfigProvider
        locale={enUS}
        theme={{
          algorithm: mode === 'dark' ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
          token: {
            ...SHARED_TOKENS,
            colorBgLayout: mode === 'dark' ? '#16181d' : '#f4f6fa',
          },
          components: {
            Layout: {
              siderBg: mode === 'dark' ? '#1c1f26' : '#ffffff',
              headerBg: mode === 'dark' ? '#1c1f26' : '#ffffff',
              headerHeight: 56,
            },
            Menu: { itemBg: 'transparent', subMenuItemBg: 'transparent' },
            Table: { headerBg: mode === 'dark' ? '#22252d' : '#fafbfd' },
            Card: { paddingLG: 20 },
          },
        }}
      >
        <AntdApp>{children}</AntdApp>
      </ConfigProvider>
    </ThemeContext.Provider>
  );
}
