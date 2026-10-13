import { useCallback, useEffect, useRef, useState } from 'react';

export function useAsyncResource(fetcher, { enabled = true, pollIntervalMs } = {}) {
  const [state, setState] = useState({ data: null, error: null, isLoading: enabled });
  const [isRefreshing, setIsRefreshing] = useState(false);
  const requestId = useRef(0);

  const load = useCallback(async ({ background = false } = {}) => {
    if (!enabled) return;
    const id = ++requestId.current;
    const controller = new AbortController();
    if (background) setIsRefreshing(true);
    else setState((current) => ({ ...current, isLoading: true, error: null }));

    try {
      const data = await fetcher(controller.signal);
      if (id === requestId.current) setState({ data, error: null, isLoading: false });
    } catch (error) {
      if (id !== requestId.current || error?.name === 'AbortError') return;
      setState((current) => ({ data: background ? current.data : null, error, isLoading: false }));
    } finally {
      if (id === requestId.current) setIsRefreshing(false);
    }
  }, [enabled, fetcher]);

  useEffect(() => {
    if (!enabled) {
      setState({ data: null, error: null, isLoading: false });
      return undefined;
    }
    load();
    return () => { requestId.current += 1; };
  }, [enabled, load]);

  useEffect(() => {
    if (!enabled || !pollIntervalMs) return undefined;
    const tick = () => document.visibilityState === 'visible' && load({ background: true });
    const timer = setInterval(tick, pollIntervalMs);
    document.addEventListener('visibilitychange', tick);
    return () => {
      clearInterval(timer);
      document.removeEventListener('visibilitychange', tick);
    };
  }, [enabled, load, pollIntervalMs]);

  return {
    ...state,
    isRefreshing,
    refresh: useCallback(() => load({ background: true }), [load]),
    reload: useCallback(() => load(), [load]),
  };
}
