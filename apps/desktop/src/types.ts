import type { SessionSyncStatus } from "./pages/SessionManagementPage";

export type Lang = "zh" | "en";
export type ProviderMode = "list" | "form" | "official";
export type InstructionMode = "list" | "form";
export type PromptInjectionMode = "append" | "replace";
export type PromptEngine = "codex" | "claude" | "zcode" | "grok" | "kilo" | "pi";
export type ToolId = "codex" | "claude" | "grok" | "zcode" | "kilo" | "pi";
export type CodexInstructionStatus = "active" | "external" | "inactive" | "none";

export type ToolCapabilitySet = {
  providers: boolean;
  sessions: boolean;
  sessionSync: boolean;
  sessionDelete: boolean;
  skills: boolean;
  mcp: boolean;
  config: boolean;
  prompts: boolean;
};

export type ToolStatus = {
  id: ToolId;
  label: string;
  installed: boolean;
  version?: string | null;
  homeDir: string;
  configPath: string;
  configFormat: "toml" | "json" | string;
  configExists: boolean;
  authPath?: string | null;
  authExists: boolean;
  instructionPath: string;
  nativeInstructionPath: string;
  diagnosticPath?: string | null;
  instructionExists: boolean;
  instructionEnabled: boolean;
  model?: string | null;
  provider?: string | null;
  providerId?: string | null;
  notice?: string | null;
  capabilities: ToolCapabilitySet;
};

export type ToolConfigFile = {
  id: string;
  label: string;
  path: string;
  format: "toml" | "json" | string;
  exists: boolean;
  native: boolean;
  text: string;
};

export type ToolConfigBundle = {
  tool: ToolId;
  label: string;
  primaryFileId: string;
  files: ToolConfigFile[];
  notice?: string | null;
};

export type InstructionTemplate = {
  id: string;
  filename: string;
  title: string;
  subtitle: string;
  badge: string;
};

export type ProviderSummary = {
  id: string;
  name?: string;
  baseUrl?: string;
  wireApi?: string;
  requiresOpenaiAuth?: boolean;
  isCurrent: boolean;
};

export type SavedProvider = {
  appType?: ToolId;
  id: string;
  native?: boolean;
  available?: boolean;
  statusMessage?: string | null;
  models?: string[];
  providerName: string;
  baseUrl: string;
  model: string;
  apiKey?: string;
  hasApiKey?: boolean;
  tomlConfig?: string;
  wireApi: string;
  requiresOpenaiAuth: boolean;
};

export type SavedPrompt = {
  id: string;
  title: string;
  filename: string;
  content: string;
};

export type BuiltinPromptStatus = {
  id: string;
  filename: string;
  title: string;
  subtitle: string;
  badge: string;
  sourceUrl: string;
  cached: boolean;
  updated: boolean;
  contentSource: string;
  syncIssue?: "catalog" | "content" | null;
  checkedAt?: string | null;
  message: string;
};

export type BackupEntry = {
  id: string;
  action: string;
  createdAt: string;
  path: string;
  hadConfig: boolean;
  hadAuth: boolean;
  hadAgents?: boolean;
};

export type PromptBackupEntry = {
  id: string;
  engine: PromptEngine;
  action: string;
  createdAt: string;
  path: string;
  scope?: string | null;
  injectionMode?: PromptInjectionMode | null;
  fileCount: number;
};

export type PromptRestoreResult = {
  ok: boolean;
  message: string;
  backupId?: string;
  engine: PromptEngine;
};

export type CodexState = {
  codexDir: string;
  configPath: string;
  authPath: string;
  configExists: boolean;
  authExists: boolean;
  officialAuthAvailable: boolean;
  model?: string;
  modelProvider?: string;
  instructionFile?: string;
  instructionEnabled: boolean;
  instructionStatus: CodexInstructionStatus;
  inactiveInstructionFile?: string;
  instructionInjectionMode?: PromptInjectionMode;
  instructionTemplateKey?: string;
  agentsPath: string;
  activeSavedProviderId?: string;
  providers: ProviderSummary[];
  configText: string;
  authPreview?: unknown;
  authText: string;
  lastBackup?: BackupEntry;
};

export type ActionResult = {
  ok: boolean;
  message: string;
  backupId?: string;
  state: CodexState;
};

export type ClaudeRuntimeProfile = {
  path: string;
  exists: boolean;
  managed: boolean;
};

export type ClaudeRuntimeState = {
  supported: boolean;
  platform: "macos" | "windows" | "unsupported";
  shell?: string;
  promptPath: string;
  promptExists: boolean;
  profiles: ClaudeRuntimeProfile[];
  status: "active" | "inactive" | "partial" | "needs-repair" | "unsupported";
  active: boolean;
};

export type RuntimePreviewTarget = {
  label: string;
  path: string;
  operation: string;
  exists: boolean;
};

export type PromptRuntimePreview = {
  engine: PromptEngine;
  operation: "install" | "uninstall";
  title: string;
  summary: string;
  backupLocation: string;
  restartHint?: string;
  targets: RuntimePreviewTarget[];
};

export type ClaudeState = {
  claudeDir: string;
  memoryPath: string;
  memoryExists: boolean;
  instructionEnabled: boolean;
  instructionInjectionMode?: PromptInjectionMode;
  instructionTemplateKey?: string;
  activeInstructionTitle?: string;
  runtime: ClaudeRuntimeState;
};

export type ClaudeActionResult = {
  ok: boolean;
  message: string;
  backupId?: string;
  state: ClaudeState;
};

export type ZcodeState = {
  managedDir: string;
  systemFile: string;
  systemFileExists: boolean;
  instructionEnabled: boolean;
  instructionInjectionMode?: PromptInjectionMode;
  instructionTemplateKey?: string;
  zcodeApp?: string | null;
  zcodeRuntimeExists: boolean;
  runtimePatchable: boolean;
  agentOverrideSupported: boolean;
  zcodeRunning: boolean;
  activeInstructionTitle?: string;
};

export type ZcodeActionResult = {
  ok: boolean;
  message: string;
  backupId?: string;
  state: ZcodeState;
};

export type ZcodeDoctor = {
  managedDir: string;
  systemFile: string;
  systemFileExists: boolean;
  launcherExists: boolean;
  zcodeApp?: string | null;
  zcodeRuntimeExists: boolean;
  runtimePatchable: boolean;
  agentOverrideSupported: boolean;
  zcodeRunning: boolean;
};

export type ZcodeVerify = {
  systemFileExists: boolean;
  launcherExists: boolean;
  zcodeApp?: string | null;
  zcodeRuntimeExists: boolean;
  runtimePatchable: boolean;
  agentOverrideSupported: boolean;
  zcodeRunning: boolean;
};

export type GrokState = {
  grokDir: string;
  grokDirExists: boolean;
  agentsMdExists: boolean;
  configTomlExists: boolean;
  compatBlockInjected: boolean;
  activeHooksCount: number;
  disabledHooksCount: number;
  manifestExists: boolean;
  instructionEnabled: boolean;
  instructionInjectionMode?: PromptInjectionMode;
  instructionTemplateKey?: string;
  activeInstructionTitle?: string;
};

export type GrokActionResult = {
  ok: boolean;
  message: string;
  backupId?: string;
  state: GrokState;
};

export type KiloState = {
  kiloDir: string;
  kiloDirExists: boolean;
  agentsPath: string;
  agentsMdExists: boolean;
  manifestExists: boolean;
  originalSnapshotExists: boolean;
  instructionEnabled: boolean;
  instructionInjectionMode?: PromptInjectionMode;
  instructionTemplateKey?: string;
  activeInstructionTitle?: string;
};

export type KiloActionResult = {
  ok: boolean;
  message: string;
  backupId?: string;
  state: KiloState;
};

export type PiState = {
  piDir: string;
  piDirExists: boolean;
  agentsPath: string;
  agentsMdExists: boolean;
  manifestExists: boolean;
  originalSnapshotExists: boolean;
  instructionEnabled: boolean;
  instructionInjectionMode?: PromptInjectionMode;
  instructionTemplateKey?: string;
  activeInstructionTitle?: string;
};

export type PiActionResult = {
  ok: boolean;
  message: string;
  backupId?: string;
  state: PiState;
};

export type ImportResult = {
  imported: number;
  added: number;
  updated: number;
  merged: number;
  skipped: number;
  warnings: string[];
  providers: SavedProvider[];
};

export type ToolProviderActionResult = {
  ok: boolean;
  message: string;
  appType: ToolId;
  providerId: string;
  backupPath?: string | null;
};

export type AboutInfo = {
  appVersion: string;
  codexVersion?: string;
  codexDir: string;
  projectUrl: string;
  githubRepo: string;
  nativeUpdaterSupported: boolean;
  toolStatuses?: ToolStatus[];
};

export type ReleaseInfo = {
  status: "idle" | "checking" | "ok" | "error";
  latestVersion?: string;
  htmlUrl?: string;
  hasUpdate?: boolean;
  updateMethod?: "native" | "download";
};

export type AppUpdateInfo = {
  latestVersion: string;
  htmlUrl: string;
  hasUpdate: boolean;
};

export type ProviderConnectionResult = {
  ok: boolean;
  status?: number | null;
  message: string;
  durationMs: number;
};

export type ProviderModel = {
  id: string;
  created?: number | null;
};

export type ProviderModelsResult = {
  models: ProviderModel[];
  status: number;
  durationMs: number;
};

export type SessionSyncResult = {
  status: SessionSyncStatus;
  updatedRollouts: number;
  updatedThreads: number;
  backupDir: string;
};

export type SessionDeleteResult = {
  status: SessionSyncStatus;
  requestedSessions: number;
  deletedSessions: number;
  failedSessions: number;
  failureMessage?: string | null;
  deletedThreadRows: number;
  deletedRolloutFiles: number;
  deletedRelatedRows: number;
};

export type ManagedSkill = {
  id: string;
  name: string;
  description?: string | null;
  directory: string;
  enabled: boolean;
  source: string;
  path: string;
  contentHash?: string | null;
  updateStatus: string;
};

export type ManagedMcpServer = {
  id: string;
  name: string;
  transport: string;
  enabled: boolean;
  source: string;
  summary: string;
  command?: string | null;
  url?: string | null;
  configJson: unknown;
};

export type SkillsMcpState = {
  tool?: ToolId;
  toolLabel?: string;
  toolDir?: string;
  skillsDir?: string;
  configPath?: string;
  codexDir: string;
  codexSkillsDir: string;
  disabledSkillsDir: string;
  mcpAdapterInstalled?: boolean | null;
  skills: ManagedSkill[];
  mcpServers: ManagedMcpServer[];
  warnings: string[];
};

export type ToolSession = {
  id: string;
  title: string;
  summary?: string | null;
  cwd?: string | null;
  sourcePath?: string | null;
  createdAtMs?: number | null;
  updatedAtMs?: number | null;
  archived: boolean;
  resumeCommand?: string | null;
};

export type ToolSessionList = {
  tool: ToolId;
  root: string;
  readOnly: boolean;
  sessions: ToolSession[];
  warnings: string[];
};

export type SkillsMcpActionResult = {
  importedSkills: number;
  importedMcp: number;
  message: string;
  state: SkillsMcpState;
};

export type SkillsMcpImportPreview = {
  skills: ManagedSkill[];
  mcpServers: ManagedMcpServer[];
  warnings: string[];
};

export type McpIntegrationId =
  | "ida-pro-mcp"
  | "cheatengine-mcp"
  | "x64dbg-mcp"
  | "burp-suite-mcp";

export type McpIntegrationInstallInput = {
  integrationId: McpIntegrationId;
  sourcePath?: string | null;
  hostPath?: string | null;
  command?: string | null;
  endpoint?: string | null;
  mode?: "local" | "remote" | "direct" | "proxy" | null;
  sourceMode?: "managed" | "manual" | null;
};

export type McpHostInstallTarget = {
  path: string;
  source: string;
  operation: string;
  exists: boolean;
};

export type McpHostInstallPlan = {
  integrationId: McpIntegrationId;
  status: "ready" | "detected" | "missing" | "manual" | "remote" | string;
  hostName: string;
  hostPath?: string | null;
  targets: McpHostInstallTarget[];
  canRestore: boolean;
  message: string;
  nextStep?: string | null;
};

export type McpIntegrationInstallResult =
  | { ok: true }
  | { ok: false; error: string };

export type DiagnosticItem = {
  key: string;
  label: string;
  path?: string | null;
  status: "ok" | "missing" | "manual" | string;
  message: string;
};

export type StartupDiagnostics = {
  codexDir: string;
  needsManualSelect: boolean;
  summary: string;
  items: DiagnosticItem[];
};

export type SkinThemeColors = {
  background: string;
  panel: string;
  panelAlt: string;
  accent: string;
  accentAlt: string;
  secondary: string;
  highlight: string;
  text: string;
  muted: string;
  line: string;
};

export type SkinThemeSummary = {
  id: string;
  name: string;
  tagline: string;
  quote: string;
  image: string;
  imagePath: string;
  source: "builtin" | "imported" | string;
  enabled: boolean;
  directory: string;
  adaptive: boolean;
  surfaceOpacity: number;
  art: {
    focusX?: number | null;
    focusY?: number | null;
    safeArea?: "auto" | "left" | "right" | "center" | "none" | string | null;
    taskMode?: "auto" | "ambient" | "banner" | "off" | string | null;
  };
  colors: SkinThemeColors;
};

export type SkinRuntimeStatus = {
  supported: boolean;
  active: boolean;
  phase: "unsupported" | "inactive" | "active" | "paused" | "stale" | "unavailable" | "error" | string;
  port?: number | null;
  themeId?: string | null;
  message: string;
};

export type SkinCenterState = {
  skinsDir: string;
  currentThemeId?: string | null;
  currentThemePath?: string | null;
  themes: SkinThemeSummary[];
  runtime: SkinRuntimeStatus;
};

export type SkinActionResult = {
  message: string;
  state: SkinCenterState;
  restartRequired: boolean;
};

export type SkinExportResult = {
  path: string;
  message: string;
};
