import { IpcChannel } from '@shared/IpcChannel'

export function createWindowApi(ipcRenderer: { invoke: (channel: string, ...args: any[]) => Promise<any> }) {
  const notImplemented = (name: string) => async () => {
    throw new Error(`Not implemented: window.api.${name}`)
  }

  return {
    __invoke: (channel: string, ...args: any[]) => ipcRenderer.invoke(channel, ...args),

    // App
    getAppInfo: () => ipcRenderer.invoke(IpcChannel.App_Info),
    getDataPathFromArgs: () => ipcRenderer.invoke(IpcChannel.App_GetDataPathFromArgs),
    reload: () => ipcRenderer.invoke(IpcChannel.App_Reload),
    quit: () => ipcRenderer.invoke(IpcChannel.App_Quit),
    relaunchApp: (options?: any) => ipcRenderer.invoke(IpcChannel.App_RelaunchApp, options),
    openWebsite: (url: string) => ipcRenderer.invoke(IpcChannel.Open_Website, url),
    openPath: (path: string) => ipcRenderer.invoke(IpcChannel.Open_Path, path),
    getDiskInfo: (directoryPath: string) => ipcRenderer.invoke(IpcChannel.App_GetDiskInfo, directoryPath),
    resolvePath: (path: string) => ipcRenderer.invoke(IpcChannel.App_ResolvePath, path),
    isPathInside: (childPath: string, parentPath: string) => ipcRenderer.invoke(IpcChannel.App_IsPathInside, childPath, parentPath),
    hasWritePermission: (path: string) => ipcRenderer.invoke(IpcChannel.App_HasWritePermission, path),
    select: (options?: any) => ipcRenderer.invoke(IpcChannel.App_Select, options),
    isNotEmptyDir: (path: string) => ipcRenderer.invoke(IpcChannel.App_IsNotEmptyDir, path),
    copy: (oldPath: string, newPath: string, occupiedDirs: string[] = []) =>
      ipcRenderer.invoke(IpcChannel.App_Copy, oldPath, newPath, occupiedDirs),
    setStopQuitApp: (stop: boolean, reason: string) => ipcRenderer.invoke(IpcChannel.App_SetStopQuitApp, stop, reason),
    flushAppData: () => ipcRenderer.invoke(IpcChannel.App_FlushAppData),
    setAppDataPath: (path: string) => ipcRenderer.invoke(IpcChannel.App_SetAppDataPath, path),

    setProxy: async (_proxy: string | undefined, _bypassRules?: string) => {},
    checkForUpdate: async () => ({ updateInfo: null }),
    quitAndInstall: notImplemented('quitAndInstall'),

    // Settings toggles (best-effort stubs for now)
    setLaunchOnBoot: async (_isActive: boolean) => {},
    setLaunchToTray: async (_isActive: boolean) => {},
    setTray: async (_isActive: boolean) => {},
    setTrayOnClose: async (_isActive: boolean) => {},
    setTestPlan: async (_isActive: boolean) => {},
    setTestChannel: async (_channel: any) => {},
    setAutoUpdate: async (_isActive: boolean) => {},
    setTheme: async (_theme: any) => {},
    handleZoomFactor: async (_delta: number, _reset: boolean = false) => 1,
    setDisableHardwareAcceleration: async (_isDisable: boolean) => {},
    setUseSystemTitleBar: async (_isActive: boolean) => {},

    // Window controls
    windowControls: {
      minimize: () => ipcRenderer.invoke(IpcChannel.Windows_Minimize),
      maximize: () => ipcRenderer.invoke(IpcChannel.Windows_Maximize),
      unmaximize: () => ipcRenderer.invoke(IpcChannel.Windows_Unmaximize),
      close: () => ipcRenderer.invoke(IpcChannel.Windows_Close),
      isMaximized: () => ipcRenderer.invoke(IpcChannel.Windows_IsMaximized),
      onMaximizedChange: (callback: (isMaximized: boolean) => void) => {
        const remove = (window as any).electron?.ipcRenderer?.on(IpcChannel.Windows_MaximizedChanged, (_: any, v: any) =>
          callback(Boolean(v))
        )
        return () => remove?.()
      }
    },

    window: {
      setMinimumSize: (width: number, height: number) => ipcRenderer.invoke(IpcChannel.Windows_SetMinimumSize, width, height),
      resetMinimumSize: () => ipcRenderer.invoke(IpcChannel.Windows_ResetMinimumSize),
      getSize: () => ipcRenderer.invoke(IpcChannel.Windows_GetSize)
    },

    // Dialog / FS
    file: {
      open: (options?: any) => ipcRenderer.invoke(IpcChannel.File_Open, options),
      openPath: (path: string) => ipcRenderer.invoke(IpcChannel.File_OpenPath, path),
      selectFolder: (options?: any) => ipcRenderer.invoke(IpcChannel.File_SelectFolder, options),
      save: (path: string, content: any, options?: any) => ipcRenderer.invoke(IpcChannel.File_Save, path, content, options),
      read: (fileId: string, detectEncoding?: boolean) => ipcRenderer.invoke(IpcChannel.File_Read, fileId, detectEncoding),
      write: (filePath: string, data: Uint8Array | string) => ipcRenderer.invoke(IpcChannel.File_Write, filePath, data),
      mkdir: (dirPath: string) => ipcRenderer.invoke(IpcChannel.File_Mkdir, dirPath),
      delete: (fileId: string) => ipcRenderer.invoke(IpcChannel.File_Delete, fileId),
      deleteDir: (dirPath: string) => ipcRenderer.invoke(IpcChannel.File_DeleteDir, dirPath),
      isDirectory: (filePath: string) => ipcRenderer.invoke(IpcChannel.File_IsDirectory, filePath),
      listDirectory: (dirPath: string, options?: any) => ipcRenderer.invoke(IpcChannel.File_ListDirectory, dirPath, options),
      showInFolder: (path: string) => ipcRenderer.invoke(IpcChannel.File_ShowInFolder, path),

      // Electron-only helper; TODO implement via Tauri file-drop events.
      getPathForFile: (_file: File) => ''
    },

    fs: {
      read: async (pathOrUrl: string, encoding?: string) => {
        const result = await ipcRenderer.invoke(IpcChannel.Fs_Read, pathOrUrl, encoding)
        if (encoding) return result
        if (result instanceof Uint8Array) return result
        if (Array.isArray(result)) return Uint8Array.from(result as number[])
        return new Uint8Array()
      },
      readText: (pathOrUrl: string) => ipcRenderer.invoke(IpcChannel.Fs_ReadText, pathOrUrl)
    },

    zip: {
      compress: (text: string) => ipcRenderer.invoke(IpcChannel.Zip_Compress, text),
      decompress: (bytes: any) => ipcRenderer.invoke(IpcChannel.Zip_Decompress, bytes)
    },

    backup: {
      backup: (filename: string, content: string, path: string, skipBackupFile: boolean) =>
        ipcRenderer.invoke(IpcChannel.Backup_Backup, filename, content, path, skipBackupFile),
      restore: (path: string) => ipcRenderer.invoke(IpcChannel.Backup_Restore, path)
    },

    config: {
      set: (key: string, value: any, isNotify: boolean = false) => ipcRenderer.invoke(IpcChannel.Config_Set, key, value, isNotify),
      get: (key: string) => ipcRenderer.invoke(IpcChannel.Config_Get, key)
    },

    // Migration (drome-specific)
    drome: {
      migration: {
        detect: () => ipcRenderer.invoke('drome:migration-detect'),
        copyDataDir: (oldUserDataDir: string) => ipcRenderer.invoke('drome:migration-copy-data', oldUserDataDir)
      }
    }
  }
}
