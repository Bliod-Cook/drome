export {}

declare global {
  interface Window {
    api: any
    electron: {
      ipcRenderer: any
      process?: {
        env?: Record<string, string | undefined>
      }
    }
  }
}

