export function createInterruptionScope(signals = process) {
  let signal = null;
  let disposed = false;
  const subscribers = new Set();
  const handlers = new Map(['SIGINT', 'SIGTERM'].map(name => [name, () => {
    signal ??= name;
    for (const notify of subscribers) notify(signal);
  }]));
  for (const [name, handler] of handlers) signals.on(name, handler);
  return {
    get signal() { return signal; },
    subscribe(notify) {
      if (disposed) throw new Error('interruption scope already disposed');
      subscribers.add(notify);
      if (signal) notify(signal);
      return () => subscribers.delete(notify);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      subscribers.clear();
      for (const [name, handler] of handlers) signals.removeListener(name, handler);
    },
  };
}
