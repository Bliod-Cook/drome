const isTauri = typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__ != null

function detectPlatform(): 'win32' | 'darwin' | 'linux' {
  const ua = (navigator.userAgent || '').toLowerCase()
  const p = (navigator.platform || '').toLowerCase()

  if (ua.includes('windows') || p.includes('win')) return 'win32'
  if (ua.includes('mac') || p.includes('mac')) return 'darwin'
  if (ua.includes('linux') || p.includes('linux')) return 'linux'

  // Tauri should always be one of these; default to linux.
  return 'linux'
}

function detectArch(): string {
  const ua = (navigator.userAgent || '').toLowerCase()

  if (ua.includes('arm64') || ua.includes('aarch64')) return 'arm64'
  if (ua.includes('x86_64') || ua.includes('amd64') || ua.includes('win64') || ua.includes('x64')) return 'x64'
  if (ua.includes('ia32') || ua.includes('x86')) return 'ia32'
  if (ua.includes('arm')) return 'arm'

  return 'unknown'
}

function createNotImplementedProxy(path: string[]): any {
  const name = `window.api.${path.join('.')}`
  const fn = (..._args: any[]) => Promise.reject(new Error(`Not implemented: ${name}`))
  return new Proxy(fn as any, {
    get(_target, prop) {
      if (prop === 'then' || prop === 'catch' || prop === 'finally') return undefined
      if (prop === Symbol.toStringTag) return 'NotImplemented'
      return createNotImplementedProxy([...path, String(prop)])
    },
    apply(_target, _thisArg, _args) {
      return Promise.reject(new Error(`Not implemented: ${name}`))
    }
  })
}

function createApiProxy<T extends object>(api: T): T {
  return new Proxy(api as any, {
    get(target, prop, receiver) {
      if (prop in target) return Reflect.get(target, prop, receiver)
      return createNotImplementedProxy([String(prop)])
    }
  })
}

async function init() {
  if (!window.electron) {
    window.electron = { ipcRenderer: null as any, process: { env: {}, platform: 'linux', arch: 'unknown' } }
  }
  if (!window.electron.process) {
    window.electron.process = { env: {}, platform: 'linux', arch: 'unknown' }
  }
  if (!window.electron.process.env) {
    window.electron.process.env = {}
  }

  // Synchronously provide Electron-like process metadata before any async imports.
  window.electron.process.platform = detectPlatform()
  window.electron.process.arch = detectArch()
  window.electron.process.env.NODE_ENV = import.meta.env.DEV ? 'development' : 'production'

  const { createWindowApi } = await import('./api')
  const { IpcRendererShim } = await import('./ipcRendererShim')

  if (isTauri) {
    const [{ listen, once }, { invoke }] = await Promise.all([
      import('@tauri-apps/api/event'),
      import('@tauri-apps/api/core')
    ])

    if (!window.__DROME_FILE_DROP_QUEUE__) {
      window.__DROME_FILE_DROP_QUEUE__ = []
    }

    // Tauri exposes file-drop paths via global events; keep a queue so renderer APIs
    // can map DOM File objects back to their native paths.
    void listen<string[]>('tauri://file-drop', (event) => {
      const paths = event.payload
      if (Array.isArray(paths)) {
        window.__DROME_FILE_DROP_QUEUE__?.push(...paths.filter((p): p is string => typeof p === 'string'))
      }
    })
    void listen('tauri://file-drop-cancelled', () => {
      window.__DROME_FILE_DROP_QUEUE__ = []
    })

    const ipcRenderer = new IpcRendererShim({
      listen,
      once,
      invoke: async (channel, args) => invoke('ipc_invoke', { channel, args })
    })

    window.electron.ipcRenderer = ipcRenderer as any
    window.api = createApiProxy(createWindowApi({ invoke: ipcRenderer.invoke.bind(ipcRenderer) }))
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
  window.api = createApiProxy(createWindowApi({ invoke: ipcRenderer.invoke.bind(ipcRenderer) }))

  ;(window as any).__DROME_EMIT__ = emit
}

try {
  await init()
} catch (error) {
  console.error('[bridge/boot] Failed to initialize renderer bridge', error)
}
