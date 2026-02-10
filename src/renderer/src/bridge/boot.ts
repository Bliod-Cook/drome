const isTauri = typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__ != null

async function init() {
  if (!window.electron) {
    window.electron = { ipcRenderer: null as any, process: { env: {} } }
  }
  if (!window.electron.process) {
    window.electron.process = { env: {} }
  }
  if (!window.electron.process.env) {
    window.electron.process.env = {}
  }
  window.electron.process.env.NODE_ENV = import.meta.env.DEV ? 'development' : 'production'

  const { createWindowApi } = await import('./api')
  const { IpcRendererShim } = await import('./ipcRendererShim')

  if (isTauri) {
    const [{ listen, once }, { invoke }] = await Promise.all([
      import('@tauri-apps/api/event'),
      import('@tauri-apps/api/core')
    ])

    const ipcRenderer = new IpcRendererShim({
      listen,
      once,
      invoke: async (channel, args) => invoke('ipc_invoke', { channel, args })
    })

    window.electron.ipcRenderer = ipcRenderer as any
    window.api = createWindowApi({ invoke: ipcRenderer.invoke.bind(ipcRenderer) })
    return
  }

  // Browser (non-Tauri) fallback: no-op invoke and in-memory event bus.
  const listeners = new Map<string, Set<(payload: any) => void>>()
  const on = (channel: string, handler: (payload: any) => void) => {
    const set = listeners.get(channel) ?? new Set()
    set.add(handler)
    listeners.set(channel, set)
    return () => set.delete(handler)
  }
  const emit = (channel: string, payload: any) => {
    listeners.get(channel)?.forEach((h) => h(payload))
  }

  const ipcRenderer = new IpcRendererShim({
    listen: async (event, handler) => {
      const off = on(event, (payload) => handler({ payload }))
      return off as any
    },
    once: async (event, handler) => {
      let off: any
      off = on(event, (payload) => {
        off?.()
        handler({ payload })
      })
      return off as any
    },
    invoke: async (_channel: string, _args: any[]) => undefined
  })

  window.electron.ipcRenderer = ipcRenderer as any
  window.api = createWindowApi({ invoke: ipcRenderer.invoke.bind(ipcRenderer) })

  ;(window as any).__DROME_EMIT__ = emit
}

void init()

