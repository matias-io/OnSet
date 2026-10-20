// The `window.onset` API the preparation screens and the live view use, on Tauri's IPC.
// Runs before the page's own scripts; Tauri's globals are looked up at call time, once they
// exist.
(() => {
  const core = () => window.__TAURI__.core;
  const events = () => window.__TAURI__.event;
  // The screens expect Error objects; Tauri rejects with the command's message.
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
  Object.defineProperty(window, 'onset', {
    value: Object.freeze({
      call: (method, params = {}) => invoke('service_call', { method, params }),
      pick: (kind) => invoke('pick_paths', { kind }),
      media: (id, stem) => invoke('media_url', { id, stem: stem ?? null }),
      previewMedia: (id, click) => invoke('preview_media', { id, click }),
      export: (params) => invoke('export_file', { params }),
      account: (action, params = {}) => invoke('chatgpt_account', { action, params }),
      onEvent: (callback) => subscribe('service:event', callback),
      onLive: (callback) => subscribe('onset:live', callback),
      openLive: () => invoke('open_live_window', {}),
      setLiveOnTop: (onTop) => invoke('set_live_on_top', { onTop }),
      providers: () => invoke('providers_status', {}),
      testProvider: (provider) => invoke('providers_test', { provider }),
      lookUpDetails: (providers, trackIds, web) =>
        invoke('ai_details', { providers, trackIds, web }),
      compareSets: (providers, brief, plans) => invoke('ai_compare_sets', { providers, brief, plans }),
      readStructure: (providers, trackId, audio) =>
        invoke('ai_read_structure', { providers, trackId, audio }),
      calibrationStatus: () => invoke('calibration_status', {}),
      startCalibration: () => invoke('calibration_start', {}),
      cancelCalibration: () => invoke('calibration_cancel', {}),
      onCalibration: (callback) => subscribe('onset:calibration', callback),
    }),
  });
})();
