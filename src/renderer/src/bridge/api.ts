import { IpcChannel } from '@shared/IpcChannel'
import type { SpanContext } from '@opentelemetry/api'

import type { WindowApi } from './windowApiType'

type IpcInvoke = (channel: string, ...args: any[]) => Promise<any>

export function createWindowApi(ipcRenderer: { invoke: IpcInvoke }): WindowApi {
  const invoke: IpcInvoke = ipcRenderer.invoke.bind(ipcRenderer)

  const safeInvoke = async <T>(channel: string, fallback: T, ...args: any[]): Promise<T> => {
    try {
      const result = await invoke(channel, ...args)
      return (result ?? fallback) as T
    } catch {
      return fallback
    }
  }

  const tracedInvoke = (channel: string, spanContext: SpanContext | undefined, ...args: any[]) => {
    if (spanContext) {
      return invoke(channel, ...args, { type: 'trace', context: spanContext })
    }
    return invoke(channel, ...args)
  }

  const getPathForFile = (file: File): string => {
    const direct = (file as any)?.path
    if (typeof direct === 'string') return direct
    const queued = window.__DROME_FILE_DROP_QUEUE__?.shift()
    return typeof queued === 'string' ? queued : ''
  }

  return {
    getAppInfo: () => invoke(IpcChannel.App_Info),
    getDiskInfo: (directoryPath: string) => invoke(IpcChannel.App_GetDiskInfo, directoryPath),
    reload: () => invoke(IpcChannel.App_Reload),
    quit: () => invoke(IpcChannel.App_Quit),
    setProxy: (proxy: string | undefined, bypassRules?: string) => safeInvoke(IpcChannel.App_Proxy, undefined, proxy, bypassRules),
    checkForUpdate: () => safeInvoke(IpcChannel.App_CheckForUpdate, { updateInfo: null } as any),
    quitAndInstall: () => safeInvoke(IpcChannel.App_QuitAndInstall, undefined),
    setLanguage: (lang: string) => safeInvoke(IpcChannel.App_SetLanguage, undefined, lang),
    setEnableSpellCheck: (isEnable: boolean) => safeInvoke(IpcChannel.App_SetEnableSpellCheck, undefined, isEnable),
    setSpellCheckLanguages: (languages: string[]) =>
      safeInvoke(IpcChannel.App_SetSpellCheckLanguages, undefined, languages),
    setLaunchOnBoot: (isActive: boolean) => safeInvoke(IpcChannel.App_SetLaunchOnBoot, undefined, isActive),
    setLaunchToTray: (isActive: boolean) => safeInvoke(IpcChannel.App_SetLaunchToTray, undefined, isActive),
    setTray: (isActive: boolean) => safeInvoke(IpcChannel.App_SetTray, undefined, isActive),
    setTrayOnClose: (isActive: boolean) => safeInvoke(IpcChannel.App_SetTrayOnClose, undefined, isActive),
    setTestPlan: (isActive: boolean) => safeInvoke(IpcChannel.App_SetTestPlan, undefined, isActive),
    setTestChannel: (channel: any) => safeInvoke(IpcChannel.App_SetTestChannel, undefined, channel),
    setTheme: (theme: any) => safeInvoke(IpcChannel.App_SetTheme, undefined, theme),
    handleZoomFactor: (delta: number, reset: boolean = false) =>
      safeInvoke(IpcChannel.App_HandleZoomFactor, 1, delta, reset),
    setAutoUpdate: (isActive: boolean) => safeInvoke(IpcChannel.App_SetAutoUpdate, undefined, isActive),
    select: (options: any) => invoke(IpcChannel.App_Select, options),
    hasWritePermission: (path: string) => invoke(IpcChannel.App_HasWritePermission, path),
    resolvePath: (path: string) => invoke(IpcChannel.App_ResolvePath, path),
    isPathInside: (childPath: string, parentPath: string) => invoke(IpcChannel.App_IsPathInside, childPath, parentPath),
    setAppDataPath: (path: string) => invoke(IpcChannel.App_SetAppDataPath, path),
    getDataPathFromArgs: () => invoke(IpcChannel.App_GetDataPathFromArgs),
    copy: (oldPath: string, newPath: string, occupiedDirs: string[] = []) => invoke(IpcChannel.App_Copy, oldPath, newPath, occupiedDirs),
    setStopQuitApp: (stop: boolean, reason: string) => invoke(IpcChannel.App_SetStopQuitApp, stop, reason),
    flushAppData: () => invoke(IpcChannel.App_FlushAppData),
    isNotEmptyDir: (path: string) => invoke(IpcChannel.App_IsNotEmptyDir, path),
    relaunchApp: (options?: any) => invoke(IpcChannel.App_RelaunchApp, options),
    openWebsite: (url: string) => invoke(IpcChannel.Open_Website, url),
    getCacheSize: () => safeInvoke(IpcChannel.App_GetCacheSize, 0),
    clearCache: () => safeInvoke(IpcChannel.App_ClearCache, undefined),
    logToMain: (source: any, level: any, message: string, data: any[]) =>
      safeInvoke(IpcChannel.App_LogToMain, undefined, source, level, message, data),
    setFullScreen: (value: boolean) => invoke(IpcChannel.App_SetFullScreen, value),
    isFullScreen: () => invoke(IpcChannel.App_IsFullScreen),
    getSystemFonts: () => safeInvoke(IpcChannel.App_GetSystemFonts, [] as string[]),
    mockCrashRenderProcess: () => safeInvoke(IpcChannel.APP_CrashRenderProcess, undefined),
    mac: {
      isProcessTrusted: () => safeInvoke(IpcChannel.App_MacIsProcessTrusted, false),
      requestProcessTrust: () => safeInvoke(IpcChannel.App_MacRequestProcessTrust, false),
    },
    notification: {
      send: (notification: any) => safeInvoke(IpcChannel.Notification_Send, undefined, notification),
    },
    system: {
      getDeviceType: () => invoke(IpcChannel.System_GetDeviceType),
      getHostname: () => invoke(IpcChannel.System_GetHostname),
      getCpuName: () => invoke(IpcChannel.System_GetCpuName),
      checkGitBash: () => invoke(IpcChannel.System_CheckGitBash),
      getGitBashPath: () => invoke(IpcChannel.System_GetGitBashPath),
      getGitBashPathInfo: () => invoke(IpcChannel.System_GetGitBashPathInfo),
      setGitBashPath: (newPath: string | null) => invoke(IpcChannel.System_SetGitBashPath, newPath),
    },
    devTools: {
      toggle: () => safeInvoke(IpcChannel.System_ToggleDevTools, undefined),
    },
    zip: {
      compress: (text: string) => invoke(IpcChannel.Zip_Compress, text),
      decompress: (text: any) => invoke(IpcChannel.Zip_Decompress, text),
    },
    backup: {
      backup: (filename: string, content: string, path: string, skipBackupFile: boolean) =>
        invoke(IpcChannel.Backup_Backup, filename, content, path, skipBackupFile),
      restore: (path: string) => invoke(IpcChannel.Backup_Restore, path),
      backupToWebdav: (data: string, webdavConfig: any) =>
        safeInvoke(IpcChannel.Backup_BackupToWebdav, false as any, data, webdavConfig),
      restoreFromWebdav: (webdavConfig: any) =>
        safeInvoke(IpcChannel.Backup_RestoreFromWebdav, '' as any, webdavConfig),
      listWebdavFiles: (webdavConfig: any) => safeInvoke(IpcChannel.Backup_ListWebdavFiles, [] as any, webdavConfig),
      checkConnection: (webdavConfig: any) => safeInvoke(IpcChannel.Backup_CheckConnection, false as any, webdavConfig),
      createDirectory: (webdavConfig: any, path: string, options?: any) =>
        safeInvoke(IpcChannel.Backup_CreateDirectory, false as any, webdavConfig, path, options),
      deleteWebdavFile: (fileName: string, webdavConfig: any) =>
        safeInvoke(IpcChannel.Backup_DeleteWebdavFile, false as any, fileName, webdavConfig),
      backupToLocalDir: (data: string, fileName: string, localConfig: any) =>
        invoke(IpcChannel.Backup_BackupToLocalDir, data, fileName, localConfig),
      restoreFromLocalBackup: (fileName: string, localBackupDir?: string) =>
        invoke(IpcChannel.Backup_RestoreFromLocalBackup, fileName, localBackupDir),
      listLocalBackupFiles: (localBackupDir?: string) => invoke(IpcChannel.Backup_ListLocalBackupFiles, localBackupDir),
      deleteLocalBackupFile: (fileName: string, localBackupDir?: string) =>
        invoke(IpcChannel.Backup_DeleteLocalBackupFile, fileName, localBackupDir),
      checkWebdavConnection: (webdavConfig: any) => safeInvoke(IpcChannel.Backup_CheckConnection, false as any, webdavConfig),
      backupToS3: (data: string, s3Config: any) =>
        safeInvoke(IpcChannel.Backup_BackupToS3, false as any, data, s3Config),
      restoreFromS3: (s3Config: any) =>
        safeInvoke(IpcChannel.Backup_RestoreFromS3, '' as any, s3Config),
      listS3Files: (s3Config: any) => safeInvoke(IpcChannel.Backup_ListS3Files, [] as any, s3Config),
      deleteS3File: (fileName: string, s3Config: any) =>
        safeInvoke(IpcChannel.Backup_DeleteS3File, false as any, fileName, s3Config),
      checkS3Connection: (s3Config: any) => safeInvoke(IpcChannel.Backup_CheckS3Connection, false as any, s3Config),
      createLanTransferBackup: (data: string) => invoke(IpcChannel.Backup_CreateLanTransferBackup, data),
      deleteTempBackup: (filePath: string) => invoke(IpcChannel.Backup_DeleteTempBackup, filePath),
    },
    file: {
      select: (options?: any) => invoke(IpcChannel.File_Select, options),
      upload: (file: any) => invoke(IpcChannel.File_Upload, file),
      delete: (fileId: string) => invoke(IpcChannel.File_Delete, fileId),
      deleteDir: (dirPath: string) => invoke(IpcChannel.File_DeleteDir, dirPath),
      deleteExternalFile: (filePath: string) => invoke(IpcChannel.File_DeleteExternalFile, filePath),
      deleteExternalDir: (dirPath: string) => invoke(IpcChannel.File_DeleteExternalDir, dirPath),
      move: (path: string, newPath: string) => invoke(IpcChannel.File_Move, path, newPath),
      moveDir: (dirPath: string, newDirPath: string) => invoke(IpcChannel.File_MoveDir, dirPath, newDirPath),
      rename: (path: string, newName: string) => invoke(IpcChannel.File_Rename, path, newName),
      renameDir: (dirPath: string, newName: string) => invoke(IpcChannel.File_RenameDir, dirPath, newName),
      read: (fileId: string, detectEncoding?: boolean) => invoke(IpcChannel.File_Read, fileId, detectEncoding),
      readExternal: (filePath: string, detectEncoding?: boolean) => invoke(IpcChannel.File_ReadExternal, filePath, detectEncoding),
      clear: (spanContext?: SpanContext) => invoke(IpcChannel.File_Clear, spanContext),
      get: (filePath: string) => invoke(IpcChannel.File_Get, filePath),
      createTempFile: (fileName: string) => invoke(IpcChannel.File_CreateTempFile, fileName),
      mkdir: (dirPath: string) => invoke(IpcChannel.File_Mkdir, dirPath),
      write: (filePath: string, data: Uint8Array | string) => invoke(IpcChannel.File_Write, filePath, data),
      writeWithId: (id: string, content: string) => invoke(IpcChannel.File_WriteWithId, id, content),
      open: async (options?: any) => {
        const file = await invoke(IpcChannel.File_Open, options)
        if (file && typeof file === 'object' && Array.isArray((file as any).content)) {
          ;(file as any).content = Uint8Array.from((file as any).content as number[])
        }
        return file
      },
      openPath: (path: string) => invoke(IpcChannel.File_OpenPath, path),
      save: (path: string, content: any, options?: any) => invoke(IpcChannel.File_Save, path, content, options),
      selectFolder: (options?: any) => invoke(IpcChannel.File_SelectFolder, options),
      saveImage: (name: string, data: string) => invoke(IpcChannel.File_SaveImage, name, data),
      binaryImage: async (fileId: string) => {
        const result = await invoke(IpcChannel.File_BinaryImage, fileId)
        if (result && typeof result === 'object' && Array.isArray((result as any).data)) {
          ;(result as any).data = Uint8Array.from((result as any).data as number[])
        }
        return result
      },
      base64Image: (fileId: string) => invoke(IpcChannel.File_Base64Image, fileId),
      saveBase64Image: (data: string) => invoke(IpcChannel.File_SaveBase64Image, data),
      savePastedImage: (imageData: Uint8Array, extension?: string) =>
        invoke(IpcChannel.File_SavePastedImage, imageData, extension),
      download: (url: string, isUseContentType?: boolean) =>
        invoke(IpcChannel.File_Download, url, isUseContentType),
      copy: (fileId: string, destPath: string) => invoke(IpcChannel.File_Copy, fileId, destPath),
      base64File: (fileId: string) => invoke(IpcChannel.File_Base64File, fileId),
      pdfInfo: (fileId: string) => invoke(IpcChannel.File_GetPdfInfo, fileId),
      getPathForFile,
      openFileWithRelativePath: (file: any) => invoke(IpcChannel.File_OpenWithRelativePath, file),
      isTextFile: (filePath: string) => invoke(IpcChannel.File_IsTextFile, filePath),
      isDirectory: (filePath: string) => invoke(IpcChannel.File_IsDirectory, filePath),
      getDirectoryStructure: (dirPath: string) => invoke(IpcChannel.File_GetDirectoryStructure, dirPath),
      listDirectory: (dirPath: string, options?: any) => invoke(IpcChannel.File_ListDirectory, dirPath, options),
      checkFileName: (dirPath: string, fileName: string, isFile: boolean) =>
        invoke(IpcChannel.File_CheckFileName, dirPath, fileName, isFile),
      validateNotesDirectory: (dirPath: string) => invoke(IpcChannel.File_ValidateNotesDirectory, dirPath),
      startFileWatcher: (dirPath: string, config?: any) => invoke(IpcChannel.File_StartWatcher, dirPath, config),
      stopFileWatcher: () => invoke(IpcChannel.File_StopWatcher),
      pauseFileWatcher: () => invoke(IpcChannel.File_PauseWatcher),
      resumeFileWatcher: () => invoke(IpcChannel.File_ResumeWatcher),
      batchUploadMarkdown: (filePaths: string[], targetPath: string) =>
        invoke(IpcChannel.File_BatchUploadMarkdown, filePaths, targetPath),
      onFileChange: (callback: (data: any) => void) => {
        const remove = window.electron.ipcRenderer.on('file-change', (_: any, payload: any) => {
          if (payload && typeof payload === 'object') callback(payload)
        })
        return () => remove?.()
      },
      showInFolder: (path: string) => invoke(IpcChannel.File_ShowInFolder, path),
    },
    fs: {
      read: async (pathOrUrl: string, encoding?: string) => {
        const result = await invoke(IpcChannel.Fs_Read, pathOrUrl, encoding)
        if (encoding) return result
        if (result instanceof Uint8Array) return result
        if (Array.isArray(result)) return Uint8Array.from(result as number[])
        return new Uint8Array()
      },
      readText: (pathOrUrl: string) => invoke(IpcChannel.Fs_ReadText, pathOrUrl),
    },
    export: {
      toWord: async (markdown: string, fileName: string) => {
        const { markdownToDocxBytes } = await import('./wordExport')
        const bytes = await markdownToDocxBytes(markdown)
        await invoke(IpcChannel.File_Save, `${fileName}.docx`, bytes, {
          filters: [{ name: 'Word Document', extensions: ['docx'] }],
        })
      },
    },
    obsidian: {
      getVaults: () => safeInvoke(IpcChannel.Obsidian_GetVaults, [] as any),
      getFolders: (vaultName: string) => safeInvoke(IpcChannel.Obsidian_GetFiles, [] as any, vaultName),
      getFiles: (vaultName: string) => safeInvoke(IpcChannel.Obsidian_GetFiles, [] as any, vaultName),
    },
    openPath: (path: string) => invoke(IpcChannel.Open_Path, path),
    shortcuts: {
      update: (shortcuts: any[]) => safeInvoke(IpcChannel.Shortcuts_Update, undefined, shortcuts),
    },
    knowledgeBase: {
      create: (base: any, context?: SpanContext) => tracedInvoke(IpcChannel.KnowledgeBase_Create, context, base),
      reset: (base: any) => safeInvoke(IpcChannel.KnowledgeBase_Reset, undefined, base),
      delete: (id: string) => safeInvoke(IpcChannel.KnowledgeBase_Delete, undefined, id),
      add: (args: any) => safeInvoke(IpcChannel.KnowledgeBase_Add, undefined, args),
      remove: (args: any) => safeInvoke(IpcChannel.KnowledgeBase_Remove, undefined, args),
      search: (args: any, context?: SpanContext) => tracedInvoke(IpcChannel.KnowledgeBase_Search, context, args),
      rerank: (args: any, context?: SpanContext) => tracedInvoke(IpcChannel.KnowledgeBase_Rerank, context, args),
    },
    memory: {
      add: (messages: any, options?: any) => safeInvoke(IpcChannel.Memory_Add, { memories: [], count: 0 } as any, messages, options),
      search: (query: string, options: any) => safeInvoke(IpcChannel.Memory_Search, { memories: [], count: 0 } as any, query, options),
      list: (options?: any) => safeInvoke(IpcChannel.Memory_List, { memories: [], count: 0 } as any, options),
      delete: (id: string) => safeInvoke(IpcChannel.Memory_Delete, undefined, id),
      update: (id: string, memory: string, metadata?: Record<string, any>) =>
        safeInvoke(IpcChannel.Memory_Update, undefined, id, memory, metadata),
      get: (id: string) => safeInvoke(IpcChannel.Memory_Get, null as any, id),
      setConfig: (config: any) => safeInvoke(IpcChannel.Memory_SetConfig, undefined, config),
      deleteUser: (userId: string) => safeInvoke(IpcChannel.Memory_DeleteUser, undefined, userId),
      deleteAllMemoriesForUser: (userId: string) => safeInvoke(IpcChannel.Memory_DeleteAllMemoriesForUser, undefined, userId),
      getUsersList: () => safeInvoke(IpcChannel.Memory_GetUsersList, [] as any),
      migrateMemoryDb: () => safeInvoke(IpcChannel.Memory_MigrateMemoryDb, undefined),
    },
    window: {
      setMinimumSize: (width: number, height: number) => invoke(IpcChannel.Windows_SetMinimumSize, width, height),
      resetMinimumSize: () => invoke(IpcChannel.Windows_ResetMinimumSize),
      getSize: () => invoke(IpcChannel.Windows_GetSize),
    },
    fileService: {
      upload: (provider: any, file: any) => safeInvoke(IpcChannel.FileService_Upload, { success: false } as any, provider, file),
      list: (provider: any) => safeInvoke(IpcChannel.FileService_List, { files: [] } as any, provider),
      delete: (provider: any, fileId: string) => safeInvoke(IpcChannel.FileService_Delete, { success: false } as any, provider, fileId),
      retrieve: (provider: any, fileId: string) =>
        safeInvoke(IpcChannel.FileService_Retrieve, { success: false } as any, provider, fileId),
    },
    selectionMenu: {
      action: (action: string) => safeInvoke('selection-menu:action', undefined as any, action),
    },
    vertexAI: {
      getAuthHeaders: (params: any) => safeInvoke(IpcChannel.VertexAI_GetAuthHeaders, {} as any, params),
      getAccessToken: (params: any) => safeInvoke(IpcChannel.VertexAI_GetAccessToken, {} as any, params),
      clearAuthCache: (projectId: string, clientEmail?: string) => safeInvoke(IpcChannel.VertexAI_ClearAuthCache, undefined as any, projectId, clientEmail),
    },
    ovms: {
      isSupported: () => safeInvoke(IpcChannel.Ovms_IsSupported, false),
      addModel: (modelName: string, modelId: string, modelSource: string, task: string) =>
        safeInvoke(IpcChannel.Ovms_AddModel, undefined, modelName, modelId, modelSource, task),
      stopAddModel: () => safeInvoke(IpcChannel.Ovms_StopAddModel, undefined),
      getModels: () => safeInvoke(IpcChannel.Ovms_GetModels, [] as any),
      isRunning: () => safeInvoke(IpcChannel.Ovms_IsRunning, false),
      getStatus: () => safeInvoke(IpcChannel.Ovms_GetStatus, null as any),
      runOvms: () => safeInvoke(IpcChannel.Ovms_RunOVMS, undefined),
      stopOvms: () => safeInvoke(IpcChannel.Ovms_StopOVMS, undefined),
    },
    config: {
      set: (key: string, value: any, isNotify: boolean = false) => invoke(IpcChannel.Config_Set, key, value, isNotify),
      get: (key: string) => invoke(IpcChannel.Config_Get, key),
    },
    miniWindow: {
      show: () => invoke(IpcChannel.MiniWindow_Show),
      hide: () => invoke(IpcChannel.MiniWindow_Hide),
      close: () => invoke(IpcChannel.MiniWindow_Close),
      toggle: () => invoke(IpcChannel.MiniWindow_Toggle),
      setPin: (isPinned: boolean) => invoke(IpcChannel.MiniWindow_SetPin, isPinned),
    },
    aes: {
      encrypt: (text: string, secretKey: string, iv: string) => invoke(IpcChannel.Aes_Encrypt, text, secretKey, iv),
      decrypt: (encryptedData: string, iv: string, secretKey: string) => invoke(IpcChannel.Aes_Decrypt, encryptedData, iv, secretKey),
    },
    mcp: {
      removeServer: (server: any) => safeInvoke(IpcChannel.Mcp_RemoveServer, undefined as any, server),
      restartServer: (server: any) => safeInvoke(IpcChannel.Mcp_RestartServer, undefined as any, server),
      stopServer: (server: any) => safeInvoke(IpcChannel.Mcp_StopServer, undefined as any, server),
      listTools: (server: any, context?: SpanContext) => tracedInvoke(IpcChannel.Mcp_ListTools, context, server),
      callTool: (args: any, context?: SpanContext) => tracedInvoke(IpcChannel.Mcp_CallTool, context, args),
      listPrompts: (server: any) => safeInvoke(IpcChannel.Mcp_ListPrompts, [] as any, server),
      getPrompt: (args: any) => safeInvoke(IpcChannel.Mcp_GetPrompt, null as any, args),
      listResources: (server: any) => safeInvoke(IpcChannel.Mcp_ListResources, [] as any, server),
      getResource: (args: any) => safeInvoke(IpcChannel.Mcp_GetResource, null as any, args),
      getInstallInfo: () => safeInvoke(IpcChannel.Mcp_GetInstallInfo, null as any),
      checkMcpConnectivity: (server: any) => safeInvoke(IpcChannel.Mcp_CheckConnectivity, { ok: false } as any, server),
      uploadDxt: async (file: File) => {
        const buffer = await file.arrayBuffer()
        return safeInvoke(IpcChannel.Mcp_UploadDxt, { success: false } as any, Array.from(new Uint8Array(buffer)), file.name)
      },
      abortTool: (callId: string) => safeInvoke(IpcChannel.Mcp_AbortTool, undefined as any, callId),
      getServerVersion: (server: any) => safeInvoke(IpcChannel.Mcp_GetServerVersion, null as any, server),
      getServerLogs: (server: any) => safeInvoke(IpcChannel.Mcp_GetServerLogs, [] as any, server),
      onServerLog: (callback: (log: any) => void) => {
        const remove = window.electron.ipcRenderer.on(IpcChannel.Mcp_ServerLog, (_: any, payload: any) => callback(payload))
        return () => remove?.()
      },
    },
    python: {
      execute: (script: string, context?: Record<string, any>, timeout?: number) =>
        safeInvoke(IpcChannel.Python_Execute, null as any, script, context, timeout),
    },
    shell: {
      openExternal: (url: string, _options?: any) => invoke(IpcChannel.Open_Website, url),
    },
    copilot: {
      getAuthMessage: (headers?: Record<string, string>) => safeInvoke(IpcChannel.Copilot_GetAuthMessage, null as any, headers),
      getCopilotToken: (device_code: string, headers?: Record<string, string>) =>
        safeInvoke(IpcChannel.Copilot_GetCopilotToken, null as any, device_code, headers),
      saveCopilotToken: (access_token: string) => safeInvoke(IpcChannel.Copilot_SaveCopilotToken, null as any, access_token),
      getToken: (headers?: Record<string, string>) => safeInvoke(IpcChannel.Copilot_GetToken, null as any, headers),
      logout: () => safeInvoke(IpcChannel.Copilot_Logout, null as any),
      getUser: (token: string) => safeInvoke(IpcChannel.Copilot_GetUser, null as any, token),
    },
    cherryin: {
      saveToken: (accessToken: string, refreshToken?: string) =>
        safeInvoke(IpcChannel.CherryIN_SaveToken, null as any, accessToken, refreshToken),
      hasToken: () => safeInvoke(IpcChannel.CherryIN_HasToken, false),
      getBalance: (apiHost: string) => safeInvoke(IpcChannel.CherryIN_GetBalance, null as any, apiHost),
      logout: (apiHost: string) => safeInvoke(IpcChannel.CherryIN_Logout, null as any, apiHost),
      startOAuthFlow: (oauthServer: string, apiHost?: string) =>
        safeInvoke(IpcChannel.CherryIN_StartOAuthFlow, null as any, oauthServer, apiHost),
      exchangeToken: (code: string, state: string) => safeInvoke(IpcChannel.CherryIN_ExchangeToken, null as any, code, state),
    },
    isBinaryExist: (name: string) => safeInvoke(IpcChannel.App_IsBinaryExist, false as any, name),
    getBinaryPath: (name: string) => safeInvoke(IpcChannel.App_GetBinaryPath, null as any, name),
    installUVBinary: () => safeInvoke(IpcChannel.App_InstallUvBinary, { success: false } as any),
    installBunBinary: () => safeInvoke(IpcChannel.App_InstallBunBinary, { success: false } as any),
    installOvmsBinary: () => safeInvoke(IpcChannel.App_InstallOvmsBinary, { success: false } as any),
    protocol: {
      onReceiveData: (callback: (data: { url: string; params: any }) => void) => {
        const remove = window.electron.ipcRenderer.on('protocol-data', (_: any, data: any) => callback(data))
        return () => remove?.()
      },
    },
    externalApps: {
      detectInstalled: () => safeInvoke(IpcChannel.ExternalApps_DetectInstalled, [] as any),
    },
    nutstore: {
      getSSOUrl: () => safeInvoke(IpcChannel.Nutstore_GetSsoUrl, null as any),
      decryptToken: (token: string) => safeInvoke(IpcChannel.Nutstore_DecryptToken, null as any, token),
      getDirectoryContents: (token: string, path: string) => safeInvoke(IpcChannel.Nutstore_GetDirectoryContents, null as any, token, path),
    },
    searchService: {
      openSearchWindow: (uid: string, show?: boolean) => safeInvoke(IpcChannel.SearchWindow_Open, undefined as any, uid, show),
      closeSearchWindow: (uid: string) => safeInvoke(IpcChannel.SearchWindow_Close, undefined as any, uid),
      openUrlInSearchWindow: (uid: string, url: string) => safeInvoke(IpcChannel.SearchWindow_OpenUrl, '' as any, uid, url),
    },
    webview: {
      setOpenLinkExternal: (webviewId: number, isExternal: boolean) =>
        safeInvoke(IpcChannel.Webview_SetOpenLinkExternal, undefined as any, webviewId, isExternal),
      setSpellCheckEnabled: (webviewId: number, isEnable: boolean) =>
        safeInvoke(IpcChannel.Webview_SetSpellCheckEnabled, undefined as any, webviewId, isEnable),
      printToPDF: (webviewId: number) => safeInvoke(IpcChannel.Webview_PrintToPDF, null as any, webviewId),
      saveAsHTML: (webviewId: number) => safeInvoke(IpcChannel.Webview_SaveAsHTML, null as any, webviewId),
      onFindShortcut: (callback: (payload: any) => void) => {
        const remove = window.electron.ipcRenderer.on(IpcChannel.Webview_SearchHotkey, (_: any, payload: any) => callback(payload))
        return () => remove?.()
      },
    },
    storeSync: {
      subscribe: () => safeInvoke(IpcChannel.StoreSync_Subscribe, undefined as any),
      unsubscribe: () => safeInvoke(IpcChannel.StoreSync_Unsubscribe, undefined as any),
      onUpdate: (action: any) => safeInvoke(IpcChannel.StoreSync_OnUpdate, undefined as any, action),
    },
    selection: {
      hideToolbar: () => safeInvoke(IpcChannel.Selection_ToolbarHide, undefined as any),
      writeToClipboard: (text: string) => safeInvoke(IpcChannel.Selection_WriteToClipboard, undefined as any, text),
      determineToolbarSize: (width: number, height: number) =>
        safeInvoke(IpcChannel.Selection_ToolbarDetermineSize, null as any, width, height),
      setEnabled: (enabled: boolean) => safeInvoke(IpcChannel.Selection_SetEnabled, undefined as any, enabled),
      setTriggerMode: (triggerMode: string) => safeInvoke(IpcChannel.Selection_SetTriggerMode, undefined as any, triggerMode),
      setFollowToolbar: (isFollowToolbar: boolean) => safeInvoke(IpcChannel.Selection_SetFollowToolbar, undefined as any, isFollowToolbar),
      setRemeberWinSize: (isRemeberWinSize: boolean) =>
        safeInvoke(IpcChannel.Selection_SetRemeberWinSize, undefined as any, isRemeberWinSize),
      setFilterMode: (filterMode: string) => safeInvoke(IpcChannel.Selection_SetFilterMode, undefined as any, filterMode),
      setFilterList: (filterList: string[]) => safeInvoke(IpcChannel.Selection_SetFilterList, undefined as any, filterList),
      processAction: (actionItem: any, isFullScreen: boolean = false) =>
        safeInvoke(IpcChannel.Selection_ProcessAction, null as any, actionItem, isFullScreen),
      closeActionWindow: () => safeInvoke(IpcChannel.Selection_ActionWindowClose, undefined as any),
      minimizeActionWindow: () => safeInvoke(IpcChannel.Selection_ActionWindowMinimize, undefined as any),
      pinActionWindow: (isPinned: boolean) => safeInvoke(IpcChannel.Selection_ActionWindowPin, undefined as any, isPinned),
      resizeActionWindow: (deltaX: number, deltaY: number, direction: string) =>
        safeInvoke(IpcChannel.Selection_ActionWindowResize, undefined as any, deltaX, deltaY, direction),
    },
    agentTools: {
      respondToPermission: (payload: any) => safeInvoke(IpcChannel.AgentToolPermission_Response, null as any, payload),
    },
    quoteToMainWindow: (text: string) => safeInvoke(IpcChannel.App_QuoteToMain, undefined as any, text),
    setDisableHardwareAcceleration: (isDisable: boolean) =>
      safeInvoke(IpcChannel.App_SetDisableHardwareAcceleration, undefined as any, isDisable),
    setUseSystemTitleBar: (isActive: boolean) => safeInvoke(IpcChannel.App_SetUseSystemTitleBar, undefined as any, isActive),
    trace: {
      saveData: (topicId: string) => invoke(IpcChannel.TRACE_SAVE_DATA, topicId),
      getData: (topicId: string, traceId: string, modelName?: string) => invoke(IpcChannel.TRACE_GET_DATA, topicId, traceId, modelName),
      saveEntity: (entity: any) => invoke(IpcChannel.TRACE_SAVE_ENTITY, entity),
      getEntity: (spanId: string) => invoke(IpcChannel.TRACE_GET_ENTITY, spanId),
      bindTopic: (topicId: string, traceId: string) => invoke(IpcChannel.TRACE_BIND_TOPIC, topicId, traceId),
      tokenUsage: (spanId: string, usage: any) => invoke(IpcChannel.TRACE_TOKEN_USAGE, spanId, usage),
      cleanHistory: (topicId: string, traceId: string, modelName?: string) => invoke(IpcChannel.TRACE_CLEAN_HISTORY, topicId, traceId, modelName),
      cleanTopic: (topicId: string, traceId?: string) => invoke(IpcChannel.TRACE_CLEAN_TOPIC, topicId, traceId),
      openWindow: (topicId: string, traceId: string, autoOpen?: boolean, modelName?: string) =>
        invoke(IpcChannel.TRACE_OPEN_WINDOW, topicId, traceId, autoOpen, modelName),
      setTraceWindowTitle: (title: string) => invoke(IpcChannel.TRACE_SET_TITLE, title),
      addEndMessage: (spanId: string, modelName: string, context: string) =>
        invoke(IpcChannel.TRACE_ADD_END_MESSAGE, spanId, modelName, context),
      cleanLocalData: () => invoke(IpcChannel.TRACE_CLEAN_LOCAL_DATA),
      addStreamMessage: (spanId: string, modelName: string, context: string, message: any) =>
        invoke(IpcChannel.TRACE_ADD_STREAM_MESSAGE, spanId, modelName, context, message),
    },
    anthropic_oauth: {
      startOAuthFlow: () => safeInvoke(IpcChannel.Anthropic_StartOAuthFlow, null as any),
      completeOAuthWithCode: (code: string) => safeInvoke(IpcChannel.Anthropic_CompleteOAuthWithCode, null as any, code),
      cancelOAuthFlow: () => safeInvoke(IpcChannel.Anthropic_CancelOAuthFlow, null as any),
      getAccessToken: () => safeInvoke(IpcChannel.Anthropic_GetAccessToken, null as any),
      hasCredentials: () => safeInvoke(IpcChannel.Anthropic_HasCredentials, false as any),
      clearCredentials: () => safeInvoke(IpcChannel.Anthropic_ClearCredentials, null as any),
    },
    codeTools: {
      run: (cliTool: string, model: string, directory: string, env: Record<string, string>, options?: any) =>
        safeInvoke(IpcChannel.CodeTools_Run, null as any, cliTool, model, directory, env, options),
      getAvailableTerminals: () => safeInvoke(IpcChannel.CodeTools_GetAvailableTerminals, [] as any),
      setCustomTerminalPath: (terminalId: string, path: string) =>
        safeInvoke(IpcChannel.CodeTools_SetCustomTerminalPath, undefined as any, terminalId, path),
      getCustomTerminalPath: (terminalId: string) =>
        safeInvoke(IpcChannel.CodeTools_GetCustomTerminalPath, undefined as any, terminalId),
      removeCustomTerminalPath: (terminalId: string) =>
        safeInvoke(IpcChannel.CodeTools_RemoveCustomTerminalPath, undefined as any, terminalId),
    },
    ocr: {
      ocr: (file: any, provider: any) => safeInvoke(IpcChannel.OCR_ocr, { text: '' } as any, file, provider),
      listProviders: () => safeInvoke(IpcChannel.OCR_ListProviders, [] as any),
    },
    cherryai: {
      generateSignature: (params: any) => safeInvoke(IpcChannel.Cherryai_GetSignature, null as any, params),
    },
    windowControls: {
      minimize: () => invoke(IpcChannel.Windows_Minimize),
      maximize: () => invoke(IpcChannel.Windows_Maximize),
      unmaximize: () => invoke(IpcChannel.Windows_Unmaximize),
      close: () => invoke(IpcChannel.Windows_Close),
      isMaximized: () => invoke(IpcChannel.Windows_IsMaximized),
      onMaximizedChange: (callback: (isMaximized: boolean) => void) => {
        const remove = window.electron.ipcRenderer.on(IpcChannel.Windows_MaximizedChanged, (_: any, v: any) => callback(Boolean(v)))
        return () => remove?.()
      },
    },
    apiServer: {
      getStatus: () => safeInvoke(IpcChannel.ApiServer_GetStatus, { running: false, config: null } as any),
      start: () => safeInvoke(IpcChannel.ApiServer_Start, { success: false, error: 'Not supported in Tauri yet' } as any),
      restart: () => safeInvoke(IpcChannel.ApiServer_Restart, { success: false, error: 'Not supported in Tauri yet' } as any),
      stop: () => safeInvoke(IpcChannel.ApiServer_Stop, { success: false, error: 'Not supported in Tauri yet' } as any),
      onReady: (callback: () => void) => {
        const remove = window.electron.ipcRenderer.on(IpcChannel.ApiServer_Ready, () => callback())
        return () => remove?.()
      },
    },
    claudeCodePlugin: {
      install: (options: any) => safeInvoke(IpcChannel.ClaudeCodePlugin_Install, { success: false } as any, options),
      uninstall: (options: any) => safeInvoke(IpcChannel.ClaudeCodePlugin_Uninstall, { success: false } as any, options),
      uninstallPackage: (options: any) =>
        safeInvoke(IpcChannel.ClaudeCodePlugin_UninstallPackage, { success: false } as any, options),
      listInstalled: (agentId: string) => safeInvoke(IpcChannel.ClaudeCodePlugin_ListInstalled, { success: false } as any, agentId),
      writeContent: (options: any) =>
        safeInvoke(IpcChannel.ClaudeCodePlugin_WriteContent, { success: false } as any, options),
      installFromZip: (options: any) =>
        safeInvoke(IpcChannel.ClaudeCodePlugin_InstallFromZip, { success: false } as any, options),
      installFromDirectory: (options: any) =>
        safeInvoke(IpcChannel.ClaudeCodePlugin_InstallFromDirectory, { success: false } as any, options),
    },
    localTransfer: {
      getState: () => safeInvoke(IpcChannel.LocalTransfer_ListServices, { services: [], isScanning: false, lastUpdatedAt: Date.now() } as any),
      startScan: () => safeInvoke(IpcChannel.LocalTransfer_StartScan, { services: [], isScanning: false, lastUpdatedAt: Date.now() } as any),
      stopScan: () => safeInvoke(IpcChannel.LocalTransfer_StopScan, { services: [], isScanning: false, lastUpdatedAt: Date.now() } as any),
      connect: (payload: any) => safeInvoke(IpcChannel.LocalTransfer_Connect, { type: 'handshake-ack' } as any, payload),
      disconnect: () => safeInvoke(IpcChannel.LocalTransfer_Disconnect, undefined as any),
      onServicesUpdated: (callback: (state: any) => void) => {
        const remove = window.electron.ipcRenderer.on(IpcChannel.LocalTransfer_ServicesUpdated, (_: any, payload: any) => callback(payload))
        return () => remove?.()
      },
      onClientEvent: (callback: (event: any) => void) => {
        const remove = window.electron.ipcRenderer.on(IpcChannel.LocalTransfer_ClientEvent, (_: any, payload: any) => callback(payload))
        return () => remove?.()
      },
      sendFile: (filePath: string) => safeInvoke(IpcChannel.LocalTransfer_SendFile, null as any, { filePath }),
      cancelTransfer: () => safeInvoke(IpcChannel.LocalTransfer_CancelTransfer, undefined as any),
    },
    openclaw: {
      checkInstalled: () => safeInvoke(IpcChannel.OpenClaw_CheckInstalled, { installed: false, path: null } as any),
      checkNpmAvailable: () => safeInvoke(IpcChannel.OpenClaw_CheckNpmAvailable, { available: false, path: null } as any),
      checkGitAvailable: () => safeInvoke(IpcChannel.OpenClaw_CheckGitAvailable, { available: false, path: null } as any),
      getNodeDownloadUrl: () => safeInvoke(IpcChannel.OpenClaw_GetNodeDownloadUrl, '' as any),
      install: () => safeInvoke(IpcChannel.OpenClaw_Install, { success: false, message: 'Not supported in Tauri yet' } as any),
      uninstall: () => safeInvoke(IpcChannel.OpenClaw_Uninstall, { success: false, message: 'Not supported in Tauri yet' } as any),
      startGateway: (port?: number) => safeInvoke(IpcChannel.OpenClaw_StartGateway, { success: false, message: 'Not supported in Tauri yet' } as any, port),
      stopGateway: () => safeInvoke(IpcChannel.OpenClaw_StopGateway, { success: false, message: 'Not supported in Tauri yet' } as any),
      restartGateway: () => safeInvoke(IpcChannel.OpenClaw_RestartGateway, { success: false, message: 'Not supported in Tauri yet' } as any),
      getStatus: () => safeInvoke(IpcChannel.OpenClaw_GetStatus, { status: 'stopped', port: 0 } as any),
      checkHealth: () => safeInvoke(IpcChannel.OpenClaw_CheckHealth, { status: 'unhealthy', gatewayPort: 0 } as any),
      getDashboardUrl: () => safeInvoke(IpcChannel.OpenClaw_GetDashboardUrl, '' as any),
      syncConfig: (provider: any, primaryModel: any) =>
        safeInvoke(IpcChannel.OpenClaw_SyncConfig, { success: false, message: 'Not supported in Tauri yet' } as any, provider, primaryModel),
      getChannels: () => safeInvoke(IpcChannel.OpenClaw_GetChannels, [] as any),
    },
    analytics: {
      trackTokenUsage: (data: any) => safeInvoke(IpcChannel.Analytics_TrackTokenUsage, undefined as any, data),
    },
    drome: {
      migration: {
        detect: () => invoke('drome:migration-detect'),
        copyDataDir: (oldUserDataDir: string) => invoke('drome:migration-copy-data', oldUserDataDir),
      },
    },
  }
}
