import type { UnlistenFn } from '@tauri-apps/api/event'

export type IpcRendererListener = (event: any, payload: any) => void

type ListenerRecord = {
  channel: string
  listener: IpcRendererListener
  unlistenPromise: Promise<UnlistenFn>
  unlisten?: UnlistenFn
  removed: boolean
}

export class IpcRendererShim {
  private listeners: ListenerRecord[] = []

  constructor(
    private readonly impl: {
      listen: (event: string, handler: (e: { payload: any }) => void) => Promise<UnlistenFn>
      once: (event: string, handler: (e: { payload: any }) => void) => Promise<UnlistenFn>
      invoke: (channel: string, args: any[]) => Promise<any>
    }
  ) {}

  on(channel: string, listener: IpcRendererListener): () => void {
    const unlistenPromise = this.impl.listen(channel, (e) => {
      listener(null, e.payload)
    })

    const rec: ListenerRecord = { channel, listener, unlistenPromise, removed: false }
    this.listeners.push(rec)

    unlistenPromise
      .then((unlisten) => {
        rec.unlisten = unlisten
        if (rec.removed) {
          try {
            unlisten()
          } catch {
            // ignore
          }
        }
      })
      .catch(() => {
        rec.removed = true
      })

    return () => this.off(channel, listener)
  }

  once(channel: string, listener: IpcRendererListener): () => void {
    const unlistenPromise = this.impl.once(channel, (e) => {
      listener(null, e.payload)
    })

    const rec: ListenerRecord = { channel, listener, unlistenPromise, removed: false }
    this.listeners.push(rec)

    unlistenPromise
      .then((unlisten) => {
        rec.unlisten = unlisten
        if (rec.removed) {
          try {
            unlisten()
          } catch {
            // ignore
          }
        }
      })
      .catch(() => {
        rec.removed = true
      })

    return () => this.off(channel, listener)
  }

  async invoke(channel: string, ...args: any[]): Promise<any> {
    return this.impl.invoke(channel, args)
  }

  addListener(channel: string, listener: IpcRendererListener): () => void {
    return this.on(channel, listener)
  }

  off(channel: string, listener: IpcRendererListener): void {
    this.removeListener(channel, listener)
  }

  removeListener(channel: string, listener: IpcRendererListener): void {
    const remaining: ListenerRecord[] = []
    for (const rec of this.listeners) {
      if (rec.channel === channel && rec.listener === listener) {
        rec.removed = true
        const unlisten = rec.unlisten
        if (unlisten) {
          try {
            unlisten()
          } catch {
            // ignore
          }
        } else {
          rec.unlistenPromise.then((u) => u()).catch(() => {})
        }
      } else {
        remaining.push(rec)
      }
    }
    this.listeners = remaining
  }

  removeAllListeners(channel: string): void {
    const remaining: ListenerRecord[] = []
    for (const rec of this.listeners) {
      if (rec.channel === channel) {
        try {
          rec.removed = true
          if (rec.unlisten) {
            rec.unlisten()
          } else {
            rec.unlistenPromise.then((u) => u()).catch(() => {})
          }
        } catch {
          // ignore
        }
      } else {
        remaining.push(rec)
      }
    }
    this.listeners = remaining
  }
}
