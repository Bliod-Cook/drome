import type { WindowApi } from './windowApiType'

export {}

declare global {
  interface Window {
    api: WindowApi
    electron: {
      ipcRenderer: any
      process: {
        platform: 'win32' | 'darwin' | 'linux' | string
        arch: string
        env: Record<string, string | undefined>
      }
    }

    __DROME_FILE_DROP_QUEUE__?: string[]
  }
}
