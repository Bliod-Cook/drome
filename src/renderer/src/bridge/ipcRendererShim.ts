import type { UnlistenFn } from '@tauri-apps/api/event'

export type IpcRendererListener = (event: any, payload: any) => void

type ListenerRecord = {
  channel: string
  unlisten: UnlistenFn
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
    let removed = false
    const p = this.impl.listen(channel, (e) => {
      listener(null, e.payload)
    })
    const remover = () => {
      if (removed) return
      removed = true
      p.then((unlisten) => unlisten()).catch(() => {})
    }
    p.then((unlisten) => this.listeners.push({ channel, unlisten })).catch(() => {})
    return remover
  }

  once(channel: string, listener: IpcRendererListener): () => void {
    let removed = false
    const p = this.impl.once(channel, (e) => {
      listener(null, e.payload)
    })
    const remover = () => {
      if (removed) return
      removed = true
      p.then((unlisten) => unlisten()).catch(() => {})
    }
    p.then((unlisten) => this.listeners.push({ channel, unlisten })).catch(() => {})
    return remover
  }

  async invoke(channel: string, ...args: any[]): Promise<any> {
    return this.impl.invoke(channel, args)
  }

  removeAllListeners(channel: string): void {
    const remaining: ListenerRecord[] = []
    for (const rec of this.listeners) {
      if (rec.channel === channel) {
        try {
          rec.unlisten()
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

