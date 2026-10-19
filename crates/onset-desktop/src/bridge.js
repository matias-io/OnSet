// The `window.autocue` API Autocue's screens were written against (Electron's preload), on
// Tauri's IPC, plus `window.onset` for the live view. Runs before the page's own scripts;
// Tauri's globals are looked up at call time, once they exist.
(() => {
  const core = () => window.__TAURI__.core;
  const events = () => window.__TAURI__.event;
  // Electron rejected with Error objects; Tauri rejects with the command's message.
  const invoke = (cmd, args) =>
    core()
      .invoke(cmd, args)
      .catch((e) => {
        throw e instanceof Error ? e : new Error(typeof e === 'string' ? e : JSON.stringify(e));
      });
  const subscribe = (name, callback) => {
    let stop = null;
    let gone = false;
    events()
      .listen(name, (e) => callback(e.payload))
      .then((un) => (gone ? un() : (stop = un)));
    return () => {
      gone = true;
      if (stop) stop();
    };
  };
  Object.defineProperty(window, 'autocue', {
    value: Object.freeze({
      call: (method, params = {}) => invoke('autocue_call', { method, params }),
      pick: (kind) => invoke('autocue_pick', { kind }),
      media: (id, stem) => invoke('autocue_media', { id, stem: stem ?? null }),
      previewMedia: (id, click) => invoke('autocue_preview_media', { id, click }),
      export: (params) => invoke('autocue_export', { params }),
      account: (action, params = {}) => invoke('autocue_account', { action, params }),
      onEvent: (callback) => subscribe('autocue:event', callback),
    }),
  });
  Object.defineProperty(window, 'onset', {
    value: Object.freeze({
      onLive: (callback) => subscribe('onset:live', callback),
      openLive: () => invoke('open_live_window', {}),
      setLiveOnTop: (onTop) => invoke('set_live_on_top', { onTop }),
    }),
  });
})();
