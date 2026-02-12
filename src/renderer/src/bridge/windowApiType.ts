import type { GitBashPathInfo, TerminalConfig, UpgradeChannel } from '@shared/config/constant'
import type { LogLevel, LogSourceWithContext } from '@shared/config/logger'
import type {
  FileChangeEvent,
  LanClientEvent,
  LanFileCompleteMessage,
  LanHandshakeAckMessage,
  LocalTransferConnectPayload,
  LocalTransferState,
  MCPServerLogEntry,
  WebviewKeyEvent
} from '@shared/config/types'
import type { ExternalAppInfo } from '@shared/externalApp/types'
import type { SpanContext } from '@opentelemetry/api'
import type { SpanEntity, TokenUsage } from '@mcp-trace/trace-core'
import type {
  AddMemoryOptions,
  AssistantMessage,
  FileListResponse,
  FileMetadata,
  FileUploadResponse,
  GetApiServerStatusResult,
  KnowledgeBaseParams,
  KnowledgeItem,
  KnowledgeSearchResult,
  MCPServer,
  MemoryConfig,
  MemoryListOptions,
  MemorySearchOptions,
  Model,
  OcrProvider,
  OcrResult,
  Provider,
  RestartApiServerStatusResult,
  S3Config,
  Shortcut,
  StartApiServerStatusResult,
  StopApiServerStatusResult,
  SupportedOcrFile,
  ThemeMode,
  WebDavConfig
} from '@types'
import type {
  InstalledPlugin,
  InstallFromDirectoryOptions,
  InstallFromSourceResult,
  InstallFromZipOptions,
  InstallPluginOptions,
  PluginMetadata,
  PluginResult,
  UninstallPluginOptions,
  UninstallPluginPackageOptions,
  UninstallPluginPackageResult,
  WritePluginContentOptions
} from '@types/plugin'
import type { Notification } from '@types/notification'

export type OpenClawGatewayStatus = 'stopped' | 'starting' | 'running' | 'error'

export interface OpenClawHealthInfo {
  status: 'healthy' | 'unhealthy'
  gatewayPort: number
  uptime?: number
  version?: string
}

export interface OpenClawChannelInfo {
  id: string
  name: string
  type: string
  status: 'connected' | 'disconnected' | 'error'
}

export type WindowApi = {
  // App
  getAppInfo: () => Promise<any>
  getDiskInfo: (directoryPath: string) => Promise<{ free: number; size: number } | null>
  reload: () => Promise<void>
  quit: () => Promise<void>
  setProxy: (proxy: string | undefined, bypassRules?: string) => Promise<void>
  checkForUpdate: () => Promise<any>
  quitAndInstall: () => Promise<void>
  setLanguage: (lang: string) => Promise<void>
  setEnableSpellCheck: (isEnable: boolean) => Promise<void>
  setSpellCheckLanguages: (languages: string[]) => Promise<void>
  setLaunchOnBoot: (isActive: boolean) => Promise<void>
  setLaunchToTray: (isActive: boolean) => Promise<void>
  setTray: (isActive: boolean) => Promise<void>
  setTrayOnClose: (isActive: boolean) => Promise<void>
  setTestPlan: (isActive: boolean) => Promise<void>
  setTestChannel: (channel: UpgradeChannel) => Promise<void>
  setTheme: (theme: ThemeMode) => Promise<void>
  handleZoomFactor: (delta: number, reset?: boolean) => Promise<number>
  setAutoUpdate: (isActive: boolean) => Promise<void>
  select: (options: any) => Promise<any>
  hasWritePermission: (path: string) => Promise<boolean>
  resolvePath: (path: string) => Promise<string>
  isPathInside: (childPath: string, parentPath: string) => Promise<boolean>
  setAppDataPath: (path: string) => Promise<void>
  getDataPathFromArgs: () => Promise<string | null>
  copy: (oldPath: string, newPath: string, occupiedDirs?: string[]) => Promise<any>
  setStopQuitApp: (stop: boolean, reason: string) => Promise<void>
  flushAppData: () => Promise<void>
  isNotEmptyDir: (path: string) => Promise<boolean>
  relaunchApp: (options?: any) => Promise<void>
  openWebsite: (url: string) => Promise<void>
  getCacheSize: () => Promise<number>
  clearCache: () => Promise<void>
  logToMain: (source: LogSourceWithContext, level: LogLevel, message: string, data: any[]) => Promise<void>
  setFullScreen: (value: boolean) => Promise<void>
  isFullScreen: () => Promise<boolean>
  getSystemFonts: () => Promise<string[]>
  mockCrashRenderProcess: () => Promise<void>

  mac: {
    isProcessTrusted: () => Promise<boolean>
    requestProcessTrust: () => Promise<boolean>
  }

  notification: {
    send: (notification: Notification) => Promise<void>
  }

  system: {
    getDeviceType: () => Promise<'windows' | 'mac' | 'linux' | string>
    getHostname: () => Promise<string>
    getCpuName: () => Promise<string>
    checkGitBash: () => Promise<boolean>
    getGitBashPath: () => Promise<string | null>
    getGitBashPathInfo: () => Promise<GitBashPathInfo>
    setGitBashPath: (newPath: string | null) => Promise<boolean>
  }

  devTools: {
    toggle: () => Promise<void>
  }

  zip: {
    compress: (text: string) => Promise<any>
    decompress: (text: any) => Promise<any>
  }

  backup: {
    backup: (filename: string, content: string, path: string, skipBackupFile: boolean) => Promise<string>
    restore: (path: string) => Promise<string>
    backupToWebdav: (data: string, webdavConfig: WebDavConfig) => Promise<any>
    restoreFromWebdav: (webdavConfig: WebDavConfig) => Promise<any>
    listWebdavFiles: (webdavConfig: WebDavConfig) => Promise<any>
    checkConnection: (webdavConfig: WebDavConfig) => Promise<any>
    createDirectory: (webdavConfig: WebDavConfig, path: string, options?: any) => Promise<any>
    deleteWebdavFile: (fileName: string, webdavConfig: WebDavConfig) => Promise<any>
    backupToLocalDir: (data: string, fileName: string, localConfig: { localBackupDir?: string; skipBackupFile?: boolean }) => Promise<any>
    restoreFromLocalBackup: (fileName: string, localBackupDir?: string) => Promise<any>
    listLocalBackupFiles: (localBackupDir?: string) => Promise<any>
    deleteLocalBackupFile: (fileName: string, localBackupDir?: string) => Promise<any>
    checkWebdavConnection: (webdavConfig: WebDavConfig) => Promise<any>
    backupToS3: (data: string, s3Config: S3Config) => Promise<any>
    restoreFromS3: (s3Config: S3Config) => Promise<any>
    listS3Files: (s3Config: S3Config) => Promise<any>
    deleteS3File: (fileName: string, s3Config: S3Config) => Promise<any>
    checkS3Connection: (s3Config: S3Config) => Promise<any>
    createLanTransferBackup: (data: string) => Promise<string>
    deleteTempBackup: (filePath: string) => Promise<boolean>
  }

  file: {
    select: (options?: any) => Promise<FileMetadata[] | null>
    upload: (file: FileMetadata) => Promise<FileMetadata>
    delete: (fileId: string) => Promise<void>
    deleteDir: (dirPath: string) => Promise<void>
    deleteExternalFile: (filePath: string) => Promise<void>
    deleteExternalDir: (dirPath: string) => Promise<void>
    move: (path: string, newPath: string) => Promise<void>
    moveDir: (dirPath: string, newDirPath: string) => Promise<void>
    rename: (path: string, newName: string) => Promise<void>
    renameDir: (dirPath: string, newName: string) => Promise<void>
    read: (fileId: string, detectEncoding?: boolean) => Promise<string>
    readExternal: (filePath: string, detectEncoding?: boolean) => Promise<string>
    clear: (spanContext?: SpanContext) => Promise<void>
    get: (filePath: string) => Promise<FileMetadata | null>
    createTempFile: (fileName: string) => Promise<string>
    mkdir: (dirPath: string) => Promise<any>
    write: (filePath: string, data: Uint8Array | string) => Promise<void>
    writeWithId: (id: string, content: string) => Promise<void>
    open: (options?: any) => Promise<{ fileName: string; filePath: string; content?: Uint8Array | string; size: number } | null>
    openPath: (path: string) => Promise<void>
    save: (path: string, content: string | ArrayBufferView, options?: any) => Promise<string | null>
    selectFolder: (options?: any) => Promise<string | null>
    saveImage: (name: string, data: string) => Promise<FileMetadata>
    binaryImage: (fileId: string) => Promise<{ data: Uint8Array; mime: string }>
    base64Image: (fileId: string) => Promise<{ mime: string; base64: string; data: string }>
    saveBase64Image: (data: string) => Promise<FileMetadata>
    savePastedImage: (imageData: Uint8Array, extension?: string) => Promise<FileMetadata>
    download: (url: string, isUseContentType?: boolean) => Promise<any>
    copy: (fileId: string, destPath: string) => Promise<void>
    base64File: (fileId: string) => Promise<{ data: string; mime: string }>
    pdfInfo: (fileId: string) => Promise<number>
    getPathForFile: (file: File) => string
    openFileWithRelativePath: (file: FileMetadata) => Promise<void>
    isTextFile: (filePath: string) => Promise<boolean>
    isDirectory: (filePath: string) => Promise<boolean>
    getDirectoryStructure: (dirPath: string) => Promise<any>
    listDirectory: (dirPath: string, options?: any) => Promise<string[]>
    checkFileName: (dirPath: string, fileName: string, isFile: boolean) => Promise<{ safeName: string; exists: boolean }>
    validateNotesDirectory: (dirPath: string) => Promise<any>
    startFileWatcher: (dirPath: string, config?: any) => Promise<any>
    stopFileWatcher: () => Promise<void>
    pauseFileWatcher: () => Promise<void>
    resumeFileWatcher: () => Promise<void>
    batchUploadMarkdown: (filePaths: string[], targetPath: string) => Promise<any>
    onFileChange: (callback: (data: FileChangeEvent) => void) => () => void
    showInFolder: (path: string) => Promise<void>
  }

  fs: {
    read: (pathOrUrl: string, encoding?: string) => Promise<string | Uint8Array>
    readText: (pathOrUrl: string) => Promise<string>
  }

  export: {
    toWord: (markdown: string, fileName: string) => Promise<any>
  }

  obsidian: {
    getVaults: () => Promise<Array<{ path: string; name: string }>>
    getFolders: (vaultName: string) => Promise<Array<{ path: string; type: 'folder' | 'markdown'; name: string }>>
    getFiles: (vaultName: string) => Promise<Array<{ path: string; type: 'folder' | 'markdown'; name: string }>>
  }

  openPath: (path: string) => Promise<void>

  shortcuts: {
    update: (shortcuts: Shortcut[]) => Promise<void>
  }

  knowledgeBase: {
    create: (base: KnowledgeBaseParams, context?: SpanContext) => Promise<any>
    reset: (base: KnowledgeBaseParams) => Promise<any>
    delete: (id: string) => Promise<any>
    add: (args: { base: KnowledgeBaseParams; item: KnowledgeItem; userId?: string; forceReload?: boolean }) => Promise<any>
    remove: (args: { uniqueId: string; uniqueIds: string[]; base: KnowledgeBaseParams }) => Promise<any>
    search: (args: { search: string; base: KnowledgeBaseParams }, context?: SpanContext) => Promise<any>
    rerank: (args: { search: string; base: KnowledgeBaseParams; results: KnowledgeSearchResult[] }, context?: SpanContext) => Promise<any>
  }

  memory: {
    add: (messages: string | AssistantMessage[], options?: AddMemoryOptions) => Promise<any>
    search: (query: string, options: MemorySearchOptions) => Promise<any>
    list: (options?: MemoryListOptions) => Promise<any>
    delete: (id: string) => Promise<any>
    update: (id: string, memory: string, metadata?: Record<string, any>) => Promise<any>
    get: (id: string) => Promise<any>
    setConfig: (config: MemoryConfig) => Promise<any>
    deleteUser: (userId: string) => Promise<any>
    deleteAllMemoriesForUser: (userId: string) => Promise<any>
    getUsersList: () => Promise<any>
    migrateMemoryDb: () => Promise<any>
  }

  window: {
    setMinimumSize: (width: number, height: number) => Promise<any>
    resetMinimumSize: () => Promise<any>
    getSize: () => Promise<[number, number]>
  }

  fileService: {
    upload: (provider: Provider, file: FileMetadata) => Promise<FileUploadResponse>
    list: (provider: Provider) => Promise<FileListResponse>
    delete: (provider: Provider, fileId: string) => Promise<any>
    retrieve: (provider: Provider, fileId: string) => Promise<FileUploadResponse>
  }

  selectionMenu: {
    action: (action: string) => Promise<any>
  }

  vertexAI: {
    getAuthHeaders: (params: { projectId: string; serviceAccount?: { privateKey: string; clientEmail: string } }) => Promise<any>
    getAccessToken: (params: { projectId: string; serviceAccount?: { privateKey: string; clientEmail: string } }) => Promise<any>
    clearAuthCache: (projectId: string, clientEmail?: string) => Promise<any>
  }

  ovms: {
    isSupported: () => Promise<boolean>
    addModel: (modelName: string, modelId: string, modelSource: string, task: string) => Promise<any>
    stopAddModel: () => Promise<any>
    getModels: () => Promise<any>
    isRunning: () => Promise<boolean>
    getStatus: () => Promise<any>
    runOvms: () => Promise<any>
    stopOvms: () => Promise<any>
  }

  config: {
    set: (key: string, value: any, isNotify?: boolean) => Promise<any>
    get: (key: string) => Promise<any>
  }

  miniWindow: {
    show: () => Promise<any>
    hide: () => Promise<any>
    close: () => Promise<any>
    toggle: () => Promise<any>
    setPin: (isPinned: boolean) => Promise<any>
  }

  aes: {
    encrypt: (text: string, secretKey: string, iv: string) => Promise<string>
    decrypt: (encryptedData: string, iv: string, secretKey: string) => Promise<string>
  }

  mcp: {
    removeServer: (server: MCPServer) => Promise<any>
    restartServer: (server: MCPServer) => Promise<any>
    stopServer: (server: MCPServer) => Promise<any>
    listTools: (server: MCPServer, context?: SpanContext) => Promise<any>
    callTool: (args: { server: MCPServer; name: string; args: any; callId?: string }, context?: SpanContext) => Promise<any>
    listPrompts: (server: MCPServer) => Promise<any>
    getPrompt: (args: { server: MCPServer; name: string; args?: Record<string, any> }) => Promise<any>
    listResources: (server: MCPServer) => Promise<any>
    getResource: (args: { server: MCPServer; uri: string }) => Promise<any>
    getInstallInfo: () => Promise<any>
    checkMcpConnectivity: (server: any) => Promise<any>
    uploadDxt: (file: File) => Promise<any>
    abortTool: (callId: string) => Promise<any>
    getServerVersion: (server: MCPServer) => Promise<string | null>
    getServerLogs: (server: MCPServer) => Promise<MCPServerLogEntry[]>
    onServerLog: (callback: (log: MCPServerLogEntry & { serverId?: string }) => void) => () => void
  }

  python: {
    execute: (script: string, context?: Record<string, any>, timeout?: number) => Promise<any>
  }

  shell: {
    openExternal: (url: string, options?: any) => Promise<any>
  }

  copilot: {
    getAuthMessage: (headers?: Record<string, string>) => Promise<any>
    getCopilotToken: (device_code: string, headers?: Record<string, string>) => Promise<any>
    saveCopilotToken: (access_token: string) => Promise<any>
    getToken: (headers?: Record<string, string>) => Promise<any>
    logout: () => Promise<any>
    getUser: (token: string) => Promise<any>
  }

  cherryin: {
    saveToken: (accessToken: string, refreshToken?: string) => Promise<any>
    hasToken: () => Promise<boolean>
    getBalance: (apiHost: string) => Promise<any>
    logout: (apiHost: string) => Promise<any>
    startOAuthFlow: (oauthServer: string, apiHost?: string) => Promise<any>
    exchangeToken: (code: string, state: string) => Promise<any>
  }

  isBinaryExist: (name: string) => Promise<any>
  getBinaryPath: (name: string) => Promise<any>
  installUVBinary: () => Promise<any>
  installBunBinary: () => Promise<any>
  installOvmsBinary: () => Promise<any>

  protocol: {
    onReceiveData: (callback: (data: { url: string; params: any }) => void) => () => void
  }

  externalApps: {
    detectInstalled: () => Promise<ExternalAppInfo[]>
  }

  nutstore: {
    getSSOUrl: () => Promise<any>
    decryptToken: (token: string) => Promise<any>
    getDirectoryContents: (token: string, path: string) => Promise<any>
  }

  searchService: {
    openSearchWindow: (uid: string, show?: boolean) => Promise<any>
    closeSearchWindow: (uid: string) => Promise<any>
    openUrlInSearchWindow: (uid: string, url: string) => Promise<string>
  }

  webview: {
    setOpenLinkExternal: (webviewId: number, isExternal: boolean) => Promise<any>
    setSpellCheckEnabled: (webviewId: number, isEnable: boolean) => Promise<any>
    printToPDF: (webviewId: number) => Promise<any>
    saveAsHTML: (webviewId: number) => Promise<any>
    onFindShortcut: (callback: (payload: WebviewKeyEvent) => void) => () => void
  }

  storeSync: {
    subscribe: () => Promise<any>
    unsubscribe: () => Promise<any>
    onUpdate: (action: any) => Promise<any>
  }

  selection: {
    hideToolbar: () => Promise<any>
    writeToClipboard: (text: string) => Promise<any>
    determineToolbarSize: (width: number, height: number) => Promise<any>
    setEnabled: (enabled: boolean) => Promise<any>
    setTriggerMode: (triggerMode: string) => Promise<any>
    setFollowToolbar: (isFollowToolbar: boolean) => Promise<any>
    setRemeberWinSize: (isRemeberWinSize: boolean) => Promise<any>
    setFilterMode: (filterMode: string) => Promise<any>
    setFilterList: (filterList: string[]) => Promise<any>
    processAction: (actionItem: any, isFullScreen?: boolean) => Promise<any>
    closeActionWindow: () => Promise<any>
    minimizeActionWindow: () => Promise<any>
    pinActionWindow: (isPinned: boolean) => Promise<any>
    resizeActionWindow: (deltaX: number, deltaY: number, direction: string) => Promise<any>
  }

  agentTools: {
    respondToPermission: (payload: {
      requestId: string
      behavior: 'allow' | 'deny'
      updatedInput?: Record<string, unknown>
      message?: string
      updatedPermissions?: any[]
    }) => Promise<any>
  }

  quoteToMainWindow: (text: string) => Promise<any>
  setDisableHardwareAcceleration: (isDisable: boolean) => Promise<any>
  setUseSystemTitleBar: (isActive: boolean) => Promise<any>

  trace: {
    saveData: (topicId: string) => Promise<any>
    getData: (topicId: string, traceId: string, modelName?: string) => Promise<any>
    saveEntity: (entity: SpanEntity) => Promise<any>
    getEntity: (spanId: string) => Promise<any>
    bindTopic: (topicId: string, traceId: string) => Promise<any>
    tokenUsage: (spanId: string, usage: TokenUsage) => Promise<any>
    cleanHistory: (topicId: string, traceId: string, modelName?: string) => Promise<any>
    cleanTopic: (topicId: string, traceId?: string) => Promise<any>
    openWindow: (topicId: string, traceId: string, autoOpen?: boolean, modelName?: string) => Promise<any>
    setTraceWindowTitle: (title: string) => Promise<any>
    addEndMessage: (spanId: string, modelName: string, context: string) => Promise<any>
    cleanLocalData: () => Promise<any>
    addStreamMessage: (spanId: string, modelName: string, context: string, message: any) => Promise<any>
  }

  anthropic_oauth: {
    startOAuthFlow: () => Promise<any>
    completeOAuthWithCode: (code: string) => Promise<any>
    cancelOAuthFlow: () => Promise<any>
    getAccessToken: () => Promise<any>
    hasCredentials: () => Promise<any>
    clearCredentials: () => Promise<any>
  }

  codeTools: {
    run: (
      cliTool: string,
      model: string,
      directory: string,
      env: Record<string, string>,
      options?: { autoUpdateToLatest?: boolean; terminal?: string }
    ) => Promise<any>
    getAvailableTerminals: () => Promise<TerminalConfig[]>
    setCustomTerminalPath: (terminalId: string, path: string) => Promise<void>
    getCustomTerminalPath: (terminalId: string) => Promise<string | undefined>
    removeCustomTerminalPath: (terminalId: string) => Promise<void>
  }

  ocr: {
    ocr: (file: SupportedOcrFile, provider: OcrProvider) => Promise<OcrResult>
    listProviders: () => Promise<string[]>
  }

  cherryai: {
    generateSignature: (params: { method: string; path: string; query: string; body: Record<string, any> }) => Promise<any>
  }

  windowControls: {
    minimize: () => Promise<void>
    maximize: () => Promise<void>
    unmaximize: () => Promise<void>
    close: () => Promise<void>
    isMaximized: () => Promise<boolean>
    onMaximizedChange: (callback: (isMaximized: boolean) => void) => () => void
  }

  apiServer: {
    getStatus: () => Promise<GetApiServerStatusResult>
    start: () => Promise<StartApiServerStatusResult>
    restart: () => Promise<RestartApiServerStatusResult>
    stop: () => Promise<StopApiServerStatusResult>
    onReady: (callback: () => void) => () => void
  }

  claudeCodePlugin: {
    install: (options: InstallPluginOptions) => Promise<PluginResult<PluginMetadata>>
    uninstall: (options: UninstallPluginOptions) => Promise<PluginResult<void>>
    uninstallPackage: (options: UninstallPluginPackageOptions) => Promise<PluginResult<UninstallPluginPackageResult>>
    listInstalled: (agentId: string) => Promise<PluginResult<InstalledPlugin[]>>
    writeContent: (options: WritePluginContentOptions) => Promise<PluginResult<void>>
    installFromZip: (options: InstallFromZipOptions) => Promise<PluginResult<InstallFromSourceResult>>
    installFromDirectory: (options: InstallFromDirectoryOptions) => Promise<PluginResult<InstallFromSourceResult>>
  }

  localTransfer: {
    getState: () => Promise<LocalTransferState>
    startScan: () => Promise<LocalTransferState>
    stopScan: () => Promise<LocalTransferState>
    connect: (payload: LocalTransferConnectPayload) => Promise<LanHandshakeAckMessage>
    disconnect: () => Promise<void>
    onServicesUpdated: (callback: (state: LocalTransferState) => void) => () => void
    onClientEvent: (callback: (event: LanClientEvent) => void) => () => void
    sendFile: (filePath: string) => Promise<LanFileCompleteMessage>
    cancelTransfer: () => Promise<void>
  }

  openclaw: {
    checkInstalled: () => Promise<{ installed: boolean; path: string | null }>
    checkNpmAvailable: () => Promise<{ available: boolean; path: string | null }>
    checkGitAvailable: () => Promise<{ available: boolean; path: string | null }>
    getNodeDownloadUrl: () => Promise<string>
    install: () => Promise<{ success: boolean; message: string }>
    uninstall: () => Promise<{ success: boolean; message: string }>
    startGateway: (port?: number) => Promise<{ success: boolean; message: string }>
    stopGateway: () => Promise<{ success: boolean; message: string }>
    restartGateway: () => Promise<{ success: boolean; message: string }>
    getStatus: () => Promise<{ status: OpenClawGatewayStatus; port: number }>
    checkHealth: () => Promise<OpenClawHealthInfo>
    getDashboardUrl: () => Promise<string>
    syncConfig: (provider: Provider, primaryModel: Model) => Promise<{ success: boolean; message: string }>
    getChannels: () => Promise<OpenClawChannelInfo[]>
  }

  analytics: {
    trackTokenUsage: (data: any) => Promise<any>
  }

  // drome-specific extensions
  drome?: {
    migration: {
      detect: () => Promise<any>
      copyDataDir: (oldUserDataDir: string) => Promise<any>
    }
  }
}

