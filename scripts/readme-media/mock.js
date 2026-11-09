// Stands in for the Tauri backend so the real UI renders in a browser.
// Answers come from window.__RM__.handlers[cmd](args) when given, else the
// placeholder of the command's return type (defaults.json). Events: the page
// calls window.__emit(event, payload).
(() => {
  const RM = window.__RM__;
  const label = RM.label || "main";
  const callbacks = new Map();
  const listeners = new Map();
  let next = 1;
  window.__TAURI_OS_PLUGIN_INTERNALS__ = {
    os_type: "windows",
    platform: "windows",
    family: "windows",
    version: "10.0.26200",
    arch: "x86_64",
    eol: "\r\n",
    exe_extension: "exe",
  };
  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label },
      currentWebview: { windowLabel: label, label },
    },
    plugins: {},
    transformCallback(cb, once) {
      const id = next++;
      callbacks.set(id, (d) => {
        if (once) callbacks.delete(id);
        return cb && cb(d);
      });
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    runCallback(id, d) {
      const c = callbacks.get(id);
      if (c) c(d);
    },
    callbacks,
    convertFileSrc(p) {
      return p;
    },
    async invoke(cmd, args) {
      if (cmd === "plugin:event|listen") {
        if (!listeners.has(args.event)) listeners.set(args.event, []);
        listeners.get(args.event).push(args.handler);
        return args.handler;
      }
      if (cmd === "plugin:event|unlisten") {
        const l = listeners.get(args.event) || [];
        const i = l.indexOf(args.eventId);
        if (i >= 0) l.splice(i, 1);
        return null;
      }
      if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
        window.__emit(args.event, args.payload);
        return null;
      }
      const h = RM.handlers && RM.handlers[cmd];
      if (h) return typeof h === "function" ? h(args) : structuredClone(h);
      if (cmd === "plugin:app|version") return "2.6.0";
      if (cmd === "plugin:app|name") return "Handy Tool";
      if (cmd in RM.commands) return structuredClone(RM.commands[cmd]);
      (window.__unknown = window.__unknown || new Set()).add(cmd);
      return null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(event, id) {
      const l = listeners.get(event) || [];
      const i = l.indexOf(id);
      if (i >= 0) l.splice(i, 1);
    },
  };
  window.__emit = (event, payload) => {
    for (const id of listeners.get(event) || []) {
      const c = callbacks.get(id);
      if (c) c({ event, id, payload });
    }
  };
})();
