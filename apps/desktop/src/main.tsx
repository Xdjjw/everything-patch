import React from "react";
import { flushSync } from "react-dom";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import {
  SessionManagementPage,
  type SessionPreview,
  type SessionSyncStatus,
} from "./pages/SessionManagementPage";
import { OverviewPage } from "./pages/OverviewPage";
import { AboutPage, SettingsPage, ToolConfigPage } from "./pages/UtilityPages";
import { PromptsPage } from "./pages/PromptsPage";
import { SkillsMcpPage } from "./pages/SkillsMcpPage";
import { SkinsPage } from "./pages/SkinsPage";
import { ProvidersPage, type ProviderCopy, type ProviderRow } from "./pages/ProvidersPage";
import { AppShell, type AppTab, type AppTheme } from "./components/AppShell";
import {
  AppToast,
  SkinRestartDialog,
  StartupWizardDialog,
  UpdateDialog,
} from "./components/AppDialogs";
import { PageTransition } from "./components/PageTransition";
import { cx } from "./components/ui";
import { appUpdater, useAppUpdater } from "./appUpdater";
import { useSkinCenter } from "./hooks/useSkinCenter";
import type {
  AboutInfo,
  ActionResult,
  AppUpdateInfo,
  BuiltinPromptStatus,
  ClaudeActionResult,
  ClaudeState,
  CodexState,
  GrokActionResult,
  GrokState,
  KiloActionResult,
  KiloState,
  PiActionResult,
  PiState,
  ImportResult,
  InstructionMode,
  InstructionTemplate,
  Lang,
  McpHostInstallPlan,
  McpIntegrationInstallInput,
  McpIntegrationInstallResult,
  ZcodeActionResult,
  ZcodeState,
  PromptEngine,
  PromptBackupEntry,
  PromptInjectionMode,
  PromptRestoreResult,
  ProviderConnectionResult,
  ProviderModel,
  ProviderModelsResult,
  ProviderMode,
  ReleaseInfo,
  SavedPrompt,
  SavedProvider,
  SessionDeleteResult,
  SessionSyncResult,
  SkillsMcpActionResult,
  SkillsMcpImportPreview,
  SkillsMcpState,
  StartupDiagnostics,
  ToolConfigBundle,
  ToolId,
  ToolProviderActionResult,
  ToolSessionList,
  ToolStatus,
} from "./types";
import { toolLabel } from "./components/ToolTabs";
import "./styles/base.css";
import "./styles/app-shell.css";
import "./styles/ui-primitives.css";
import "./styles/app-dialogs.css";
import "./styles/dark-theme.css";

type Tab = AppTab;

const LANG_KEY = "codexx.lang";
const THEME_KEY = "codexx.theme";
const STARTUP_WIZARD_SEEN_KEY = "codexx.startupWizardSeen";
const ACTIVE_PROVIDER_KEY = "codexx.activeProviderId";
const PROMPT_INJECTION_MODE_KEY = "codexx.promptInjectionMode";
const PROMPT_ENGINE_KEY = "codexx.promptEngine";
const ACTIVE_TOOL_KEY = "everything-patch.activeTool";
const FALLBACK_GITHUB_REPO = "Xdjjw/everything-patch";
const SKIN_CENTER_ENABLED = false;

const activeProviderKey = (tool: ToolId) => `${ACTIVE_PROVIDER_KEY}.${tool}`;

const promptInjectionModeKey = (engine: PromptEngine) =>
  `${PROMPT_INJECTION_MODE_KEY}.${engine}`;

function storedPromptInjectionMode(engine: PromptEngine): PromptInjectionMode {
  const stored = localStorage.getItem(promptInjectionModeKey(engine))
    || (engine === "codex" ? localStorage.getItem(PROMPT_INJECTION_MODE_KEY) : null);
  return stored === "replace" ? "replace" : "append";
}

function storedPromptEngine(): PromptEngine {
  const stored = localStorage.getItem(PROMPT_ENGINE_KEY);
  return stored === "claude" || stored === "zcode" || stored === "grok" || stored === "kilo" || stored === "pi"
    ? stored
    : "codex";
}

function storedTool(): ToolId {
  const stored = localStorage.getItem(ACTIVE_TOOL_KEY);
  return stored === "claude" || stored === "grok" || stored === "zcode" || stored === "kilo" || stored === "pi"
    ? stored
    : "codex";
}

type ThemeTransitionDocument = Document & {
  startViewTransition?: (update: () => void | Promise<void>) => { finished: Promise<void> };
};

const bundledInstructionTemplates: InstructionTemplate[] = [
  {
    id: "codex-keysmith",
    filename: "codex-keysmith.md",
    title: "Codex Keysmith 默认 Prompt",
    subtitle: "同步 codex-keysmith v0.1.3，离线内置",
    badge: "默认",
  },
  {
    id: "gpt5.5-unrestricted",
    filename: "gpt5.5-unrestricted.md",
    title: "gpt-5.5 unrestricted 破甲",
    subtitle: "方法：先让ai分析项目，分析完之后发【不直白的逆向】命令",
    badge: "兼容",
  },
  {
    id: "gpt5.4-unrestricted",
    filename: "gpt5.4-unrestricted.md",
    title: "gpt-5.4 unrestricted 破甲",
    subtitle: "方法：先让ai分析项目，分析完之后发【不直白的逆向】命令",
    badge: "兼容",
  },
  {
    id: "gpt5.5-jeli",
    filename: "gpt5.5-jeli.md",
    title: "gpt5.5-jeli.md",
    subtitle: "gpt5.5 大白话（80%场景）破甲",
    badge: "通用",
  },
  {
    id: "github-gpt-5-6-sol-unrestricted-33b86c71",
    filename: "gpt-5.6-sol-unrestricted.md",
    title: "gpt-5.6-sol-unrestricted.md",
    subtitle: "gpt5.6-sol 破甲提示词",
    badge: "内置",
  },
  {
    id: "github-3-0-b459e1e8",
    filename: "海鸥3.0破甲.md",
    title: "海鸥3.0破甲.md",
    subtitle: "测试生效：海鸥在线，你要整点薯条吗？",
    badge: "内置",
  },
];

const blankProviderForm: SavedProvider = {
  appType: "codex",
  id: "",
  providerName: "",
  baseUrl: "",
  model: "gpt-5.5",
  apiKey: "",
  tomlConfig: "",
  wireApi: "responses",
  requiresOpenaiAuth: false,
};

function blankProviderForTool(tool: ToolId): SavedProvider {
  return {
    ...blankProviderForm,
    appType: tool,
    model: tool === "claude" ? "claude-sonnet-4-5" : tool === "grok" ? "grok-4.5" : "gpt-5.5",
    wireApi: tool === "claude" ? "anthropic" : "responses",
  };
}

const blankPromptForm: SavedPrompt = {
  id: "",
  title: "",
  filename: "",
  content: "",
};

const dict = {
  zh: {
    appSubtitle: "多工具 · 指令 · 配置",
    manager: "多工具配置管理器",
    load: "加载",
    refresh: "刷新",
    nav: {
      dashboard: "概览",
      provider: "供应商",
      sessions: "会话管理",
      skillsMcp: "技能和MCP",
      skins: "皮肤中心",
      instruction: "指令提示词",
      toml: "TOML",
      settings: "设置",
      about: "关于",
    },
    dashboard: {
      config: "配置文件",
      found: "已找到",
      missing: "不存在",
      provider: "供应商",
      instruction: "指令提示词状态",
      enabled: "已启用",
      disabled: "未启用",
      auth: "认证文件",
      currentConfig: "当前 Codex 配置",
      liveStatus: "实时状态",
      dir: "目录",
      configPath: "配置",
      model: "模型",
      providerName: "供应商",
      instructionFile: "指令文件",
      notSet: "未设置",
      officialDefault: "官方默认",
    },
    provider: {
      title: "供应商列表",
      subtitle: "像 cc-switch 一样管理 Codex 第三方 API。点击卡片可切换，点击 + 添加新供应商。",
      add: "添加供应商",
      importCc: "从 cc-switch 导入",
      edit: "编辑",
      viewEdit: "编辑",
      remove: "删除",
      switch: "切换",
      current: "当前",
      official: "官方配置",
      noRouting: "不支持路由",
      authReady: "认证文件存在",
      authMissing: "未找到认证文件",
      detected: "从 TOML 检测",
      local: "本地保存",
      noProviders: "还没有供应商，点击右上角 + 添加。",
      officialEdit: "OpenAI Official 编辑",
      officialHint: "官方配置不使用第三方路由；这里可以编辑官方模式下的模型和完整 auth.json（ChatGPT 登录通常包含 access_token / refresh_token / id_token）。",
      officialUrl: "官方入口",
      formAdd: "添加新供应商",
      formEdit: "编辑供应商",
      formHint: "保存后会写入供应商列表，并同步写入 Codex live 配置。下方可预览将生成的 config.toml。",
      name: "供应商名称",
      baseUrl: "Base URL",
      model: "模型",
      wireApi: "Wire API",
      apiKey: "API Key",
      apiKeyPlaceholder: "留空则不覆盖 auth.json",
      requiresAuth: "requires_openai_auth",
      save: "保存到列表",
      saveAndSwitch: "保存",
      cancel: "返回列表",
    },
    instruction: {
      title: "一键管理指令提示词",
      desc: "启用时写入指令提示词文件并设置 model_instructions_file；禁用时只移除 DevConduit 管理的指令提示词字段并删除 md 文件。每次操作前都会创建备份。",
      enabled: "已启用",
      disabled: "未启用",
      unset: "model_instructions_file 未设置",
      enable: "启用",
      disable: "禁用 / 删除",
    },
    toml: {
      title: "当前 live TOML 配置",
      desc: "这里显示的是 Codex 当前正在使用的 ~/.codex/config.toml，不是本地保存的供应商模板。切换供应商后，这里会变成新写入的 live 配置。",
      loaded: "已读取",
      missingText: "# config.toml 不存在，执行切换或启用后会自动创建。",
    },
    backups: {
      title: "备份与撤回",
      empty: "还没有备份。首次写入前会自动创建。",
      restore: "恢复",
    },
    settings: {
      title: "设置",
      language: "界面语言",
      zh: "中文",
      en: "English",
      languageDesc: "默认中文，可随时切换。设置会保存在浏览器本地存储。",
      productName: "产品名",
      productDesc: "面向 Codex、Claude、ZCode、Grok、Kilo 与 Pi 的多工具配置和提示词管理器。",
    },
    loadingConfig: "正在读取 Codex 配置...",
    noAuth: "无 auth",
    authJson: "auth.json",
  },
  en: {
    appSubtitle: "Multi-tool · Prompts · Config",
    manager: "Multi-tool configuration manager",
    load: "Load",
    refresh: "Refresh",
    nav: {
      dashboard: "Overview",
      provider: "Provider",
      sessions: "Sessions",
      skillsMcp: "Skills & MCP",
      skins: "Skins",
      instruction: "Prompt",
      toml: "TOML",
      settings: "Settings",
      about: "About",
    },
    dashboard: {
      config: "Config",
      found: "Found",
      missing: "Missing",
      provider: "Provider",
      instruction: "Instruction Prompt",
      enabled: "Enabled",
      disabled: "Disabled",
      auth: "Auth",
      currentConfig: "Current Codex config",
      liveStatus: "Live status",
      dir: "Directory",
      configPath: "Config",
      model: "Model",
      providerName: "Provider",
      instructionFile: "Instruction",
      notSet: "Not set",
      officialDefault: "Official / Default",
    },
    provider: {
      title: "Provider list",
      subtitle: "Manage Codex third-party APIs like cc-switch. Click a row to switch; use + to add a provider.",
      add: "Add provider",
      importCc: "Import from cc-switch",
      edit: "Edit",
      viewEdit: "Edit",
      remove: "Delete",
      switch: "Switch",
      current: "Current",
      official: "Official",
      noRouting: "No routing",
      authReady: "Auth found",
      authMissing: "Auth missing",
      detected: "Detected from TOML",
      local: "Local",
      noProviders: "No provider yet. Click + to add one.",
      officialEdit: "OpenAI Official settings",
      officialHint: "Official mode does not use third-party routing. You can edit the official model and the full auth.json (ChatGPT login usually contains access_token / refresh_token / id_token).",
      officialUrl: "Official URL",
      formAdd: "Add provider",
      formEdit: "Edit provider",
      formHint: "Save writes the provider to the list and applies it to the Codex live config. The generated config.toml is previewed below.",
      name: "Provider name",
      baseUrl: "Base URL",
      model: "Model",
      wireApi: "Wire API",
      apiKey: "API Key",
      apiKeyPlaceholder: "Leave blank to keep auth.json unchanged",
      requiresAuth: "requires_openai_auth",
      save: "Save",
      saveAndSwitch: "Save",
      cancel: "Back",
    },
    instruction: {
      title: "Manage instruction prompt",
      desc: "Enable writes the instruction prompt file and sets model_instructions_file; disable removes DevConduit-managed instruction prompt config and deletes the md file. Every write creates a backup first.",
      enabled: "Enabled",
      disabled: "Disabled",
      unset: "model_instructions_file is not set",
      enable: "Enable",
      disable: "Disable / delete",
    },
    toml: {
      title: "Current live TOML config",
      desc: "This is the active ~/.codex/config.toml used by Codex, not a saved provider template. After switching providers, this page shows the newly written live config.",
      loaded: "Loaded",
      missingText: "# config.toml is missing. It will be created after switching or enabling instruction.",
    },
    backups: {
      title: "Backups & restore",
      empty: "No backups yet. A backup will be created before the first write.",
      restore: "Restore",
    },
    settings: {
      title: "Settings",
      language: "Language",
      zh: "中文",
      en: "English",
      languageDesc: "Chinese is the default. You can switch at any time; the setting is saved locally.",
      productName: "Product name",
      productDesc: "A multi-tool configuration and prompt manager for Codex, Claude, ZCode, Grok, Kilo, and Pi.",
    },
    loadingConfig: "Reading Codex config...",
    noAuth: "No auth",
    authJson: "auth.json",
  },
} as const;

function getProviderPageCopy(lang: Lang, tool: ToolId): ProviderCopy {
  const t = dict[lang];
  const isChinese = lang === "zh";
  const activeToolLabel = toolLabel(tool);
  const configName = tool === "claude"
    ? "settings.json (JSON)"
    : tool === "zcode"
      ? "cli/config.json (JSON)"
      : tool === "kilo"
        ? "kilo.jsonc (JSONC)"
        : tool === "pi"
          ? "models.json + settings.json (JSON)"
          : "config.toml (TOML)";
  return {
    eyebrow: "Provider",
    title: isChinese ? `${activeToolLabel} 供应商` : `${activeToolLabel} providers`,
    subtitle: tool === "zcode"
      ? (isChinese
        ? "读取 ZCode 原生供应商与模型配置。供应商和密钥仍由 ZCode 管理，DevConduit 只负责安全切换。"
        : "Read ZCode's native providers and models. Providers and secrets remain managed by ZCode; DevConduit only switches them safely.")
      : (isChinese
        ? `管理 ${activeToolLabel} 的第三方 API。供应商数据和启用状态按工具隔离。`
        : `Manage third-party APIs for ${activeToolLabel}. Provider data and activation are isolated by tool.`),
    importLabel: tool === "zcode"
      ? (isChinese ? "刷新原生配置" : "Refresh native config")
      : t.provider.importCc,
    addLabel: t.provider.add,
    noProviders: tool === "zcode"
      ? (isChinese ? "未在 ~/.zcode/v2/config.json 中找到原生供应商" : "No native providers found in ~/.zcode/v2/config.json")
      : t.provider.noProviders,
    currentLabel: isChinese ? "当前使用" : "Current",
    enableLabel: tool === "zcode"
      ? (isChinese ? "切换" : "Switch")
      : (isChinese ? "启用" : "Enable"),
    testLabel: isChinese ? "测试连接" : "Test connection",
    editLabel: t.provider.edit,
    removeLabel: t.provider.remove,
    deleteTitle: isChinese ? "删除供应商" : "Delete provider",
    deleteDescription: (providerName) => isChinese
      ? `“${providerName}”将从供应商列表中删除，此操作无法撤销。`
      : `“${providerName}” will be removed from the provider list. This cannot be undone.`,
    deleteCurrentDescription: (providerName) => isChinese
      ? `“${providerName}”当前正在使用。删除后不会自动切换供应商，确定继续吗？`
      : `“${providerName}” is currently active. Deleting it will not switch providers automatically. Continue?`,
    deleteCancelLabel: isChinese ? "取消" : "Cancel",
    deleteConfirmLabel: isChinese ? "确认删除" : "Delete",
    noBaseUrlLabel: "no base_url",
    officialEyebrow: "OpenAI Official",
    officialTitle: t.provider.officialEdit,
    officialHint: t.provider.officialHint,
    officialUrlLabel: t.provider.officialUrl,
    authPathLabel: "auth.json",
    officialCurrentLabel: t.provider.current,
    officialAuthLabel: "auth.json (JSON)",
    officialSaveLabel: isChinese ? "保存官方配置" : "Save official config",
    cancelLabel: t.provider.cancel,
    formEyebrow: "Provider",
    formAddTitle: t.provider.formAdd,
    formEditTitle: t.provider.formEdit,
    formHint: tool === "pi"
      ? (isChinese
        ? "先保存到 DevConduit 供应商列表；只有点击启用时才会合并写入 Pi 配置，任一文件写入失败都会恢复原状态。"
        : "Save to DevConduit first. Pi configuration is merged only when you enable the provider, and any failed file write restores the original state.")
      : t.provider.formHint,
    apiConfigTitle: isChinese ? "供应商 API 配置" : "Provider API configuration",
    apiConfigDescription: tool === "pi"
      ? (isChinese
        ? "配置 Pi Provider、模型和 API Key；密钥不会从 auth.json 读取，也不会在预览中显示明文。"
        : "Configure the Pi provider, model, and API key. auth.json is never read, and previews never reveal the key.")
      : (isChinese
        ? `在同一个页面管理 API、认证信息和 ${configName}。`
        : `Manage API, authentication, and ${configName} in one place.`),
    apiKeyLabel: t.provider.apiKey,
    apiKeyPlaceholder: tool === "pi"
      ? (isChinese ? "留空则保留已保存的 API Key" : "Leave blank to keep the saved API key")
      : t.provider.apiKeyPlaceholder,
    showApiKeyLabel: isChinese ? "显示 API Key" : "Show API key",
    hideApiKeyLabel: isChinese ? "隐藏 API Key" : "Hide API key",
    baseUrlLabel: isChinese ? "API 请求地址" : t.provider.baseUrl,
    nameLabel: t.provider.name,
    modelLabel: t.provider.model,
    fetchModelsLabel: isChinese ? "获取模型列表" : "Fetch models",
    fetchingModelsLabel: isChinese ? "获取中" : "Fetching",
    chooseModelLabel: (count) => isChinese ? `选择已获取的模型（${count}）` : `Choose a fetched model (${count})`,
    wireApiLabel: t.provider.wireApi,
    requiresAuthLabel: t.provider.requiresAuth,
    authPreviewTitle: tool === "claude"
      ? "env (JSON)"
      : tool === "grok"
        ? "api_key"
        : tool === "pi"
          ? "models.json (JSON)"
          : "auth.json (JSON)",
    authPreviewDescription: isChinese
      ? "预览保存时写入或保留的认证配置；API Key 留空不会覆盖现有认证。"
      : "Preview the authentication data. An empty API key keeps the current auth file.",
    tomlTitle: configName,
    tomlDescription: tool === "pi"
      ? (isChinese
        ? "只读预览启用后合并到 Pi 的字段；现有其他配置会保留，API Key 始终脱敏显示。"
        : "Read-only preview of the fields merged into Pi when enabled. Other settings are preserved and the API key is always redacted.")
      : (isChinese
        ? `这里保存供应商模板，只有启用供应商时才会写入 ${activeToolLabel} 当前配置。`
        : `This stores the provider template and writes it to ${activeToolLabel}'s live config only when enabled.`),
    resetTomlLabel: isChinese ? "重置生成" : "Reset",
    saveLabel: tool === "pi"
      ? (isChinese ? "保存供应商" : "Save provider")
      : t.provider.saveAndSwitch,
    savingLabel: isChinese ? "保存中..." : "Saving...",
  };
}

function providerId(name: string) {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || `provider-${Date.now()}`;
}

function isReservedCodexProviderId(id: string) {
  return ["openai", "amazon-bedrock", "ollama", "lmstudio", "oss"].includes(id.trim().toLowerCase());
}

function customProviderId(name: string) {
  const id = providerId(name);
  return isReservedCodexProviderId(id) ? `${id}-custom` : id;
}

function uniqueId(base: string, existingIds: Iterable<string>) {
  const used = new Set(Array.from(existingIds).map((id) => id.trim().toLowerCase()));
  const clean = providerId(base);
  let candidate = clean;
  let index = 2;
  while (used.has(candidate.toLowerCase())) {
    candidate = `${clean}-${index}`;
    index += 1;
  }
  return candidate;
}

function splitMarkdownFilename(filename: string) {
  const clean = filename.trim().replace(/[\/\\]+/g, "-") || "prompt.md";
  const stem = clean.replace(/\.md$/i, "") || "prompt";
  return { stem, filename: `${stem}.md` };
}

function uniquePromptFilename(filename: string, existingFilenames: Iterable<string>) {
  const used = new Set(Array.from(existingFilenames).map((name) => name.trim().toLowerCase()));
  const { stem } = splitMarkdownFilename(filename);
  let candidate = `${stem}.md`;
  let index = 2;
  while (used.has(candidate.toLowerCase())) {
    candidate = `${stem}-${index}.md`;
    index += 1;
  }
  return candidate;
}

function tomlEscape(value: string) {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

function extractOpenAiApiKey(authText?: string) {
  if (!authText?.trim()) return "";
  try {
    const parsed = JSON.parse(authText) as { OPENAI_API_KEY?: unknown };
    return typeof parsed.OPENAI_API_KEY === "string" ? parsed.OPENAI_API_KEY : "";
  } catch {
    return "";
  }
}

function normalizeProviderBaseUrl(value?: string | null) {
  const raw = (value || "").trim();
  if (!raw) return "";
  try {
    const parsed = new URL(raw);
    const credentials = parsed.username
      ? `${parsed.username}${parsed.password ? `:${parsed.password}` : ""}@`
      : "";
    const path = parsed.pathname.replace(/\/+$/, "");
    return `${parsed.protocol.toLowerCase()}//${credentials}${parsed.host.toLowerCase()}${path}${parsed.search}`;
  } catch {
    return raw.replace(/\/+$/, "");
  }
}

function normalizeProviderName(value?: string | null) {
  return (value || "").trim().replace(/\s+/gu, " ").toLowerCase();
}

function parseTomlStringValue(value: string) {
  const raw = value.trim();
  if (raw.startsWith('"')) {
    try {
      return JSON.parse(raw) as string;
    } catch {
      const end = raw.lastIndexOf('"');
      return end > 0 ? raw.slice(1, end) : raw.slice(1);
    }
  }
  if (raw.startsWith("'")) {
    const end = raw.lastIndexOf("'");
    return end > 0 ? raw.slice(1, end) : raw.slice(1);
  }
  return raw.replace(/\s+#.*$/, "").trim();
}

function extractTomlProviderApiKey(configText: string | undefined, providerId?: string) {
  if (!configText?.trim()) return "";
  const targetSection = providerId ? `model_providers.${providerId}` : "";
  let currentSection = "";
  let topLevelValue = "";
  let firstProviderValue = "";

  for (const line of configText.replace(/\r\n?/g, "\n").split("\n")) {
    const section = line.match(/^\s*\[([^\]]+)]\s*(?:#.*)?$/);
    if (section) {
      currentSection = section[1].trim();
      continue;
    }
    const token = line.match(/^\s*experimental_bearer_token\s*=\s*(.+?)\s*$/);
    if (!token) continue;
    const value = parseTomlStringValue(token[1]).trim();
    if (!value) continue;
    if (!currentSection) topLevelValue = value;
    if (currentSection.startsWith("model_providers.") && !firstProviderValue) firstProviderValue = value;
    if (targetSection && currentSection === targetSection) return value;
  }

  return topLevelValue || (!providerId ? firstProviderValue : "");
}

function savedProviderApiKey(provider: SavedProvider) {
  return (provider.apiKey || "").trim() || extractTomlProviderApiKey(provider.tomlConfig);
}

function providerIdentityKey(baseUrl?: string | null, apiKey?: string | null, providerName?: string | null) {
  const normalizedUrl = normalizeProviderBaseUrl(baseUrl);
  if (!normalizedUrl) return "";
  const normalizedKey = (apiKey || "").trim();
  return JSON.stringify([
    normalizedUrl,
    normalizedKey ? `key:${normalizedKey}` : `name:${normalizeProviderName(providerName)}`,
  ]);
}

function buildProviderTomlPreview(provider: SavedProvider, state: CodexState | null) {
  if (provider.appType === "claude") {
    const env: Record<string, string> = {
      ANTHROPIC_BASE_URL: provider.baseUrl.trim().replace(/\/+$/, "") || "https://example.com",
      ANTHROPIC_MODEL: provider.model.trim() || "claude-sonnet-4-5",
    };
    if (provider.apiKey?.trim() || provider.hasApiKey) env.ANTHROPIC_AUTH_TOKEN = "[REDACTED]";
    return JSON.stringify({ env }, null, 2);
  }
  if (provider.appType === "grok") {
    const model = provider.model.trim() || "grok-4.5";
    const apiKey = provider.apiKey?.trim() || provider.hasApiKey ? 'api_key = "[REDACTED]"\n' : "";
    return [
      "[models]",
      `default = "${tomlEscape(model)}"`,
      "",
      `[model.${tomlEscape(model)}]`,
      `model = "${tomlEscape(model)}"`,
      `base_url = "${tomlEscape(provider.baseUrl.trim().replace(/\/+$/, "") || "https://example.com/v1")}"`,
      `name = "${tomlEscape(provider.providerName.trim() || "your-provider")}"`,
      `${apiKey}api_backend = "${tomlEscape(provider.wireApi || "responses")}"`,
      "context_window = 500000",
    ].join("\n");
  }
  if (provider.appType === "pi") {
    const model = provider.model.trim() || "gpt-5.5";
    const id = provider.id.trim() || providerId(provider.providerName || "devconduit-provider");
    const normalizedWireApi = (provider.wireApi || "responses").toLowerCase();
    const api = normalizedWireApi.includes("anthropic")
      ? "anthropic-messages"
      : normalizedWireApi.includes("google") || normalizedWireApi.includes("gemini")
        ? "google-generative-ai"
        : normalizedWireApi.includes("response")
          ? "openai-responses"
          : "openai-completions";
    return JSON.stringify({
      "models.json": {
        providers: {
          [id]: {
            baseUrl: provider.baseUrl.trim().replace(/\/+$/, "") || "https://example.com/v1",
            api,
            ...(provider.apiKey?.trim() || provider.hasApiKey ? { apiKey: "[REDACTED]" } : {}),
            models: [{ id: model, name: model }],
          },
        },
      },
      "settings.json": {
        defaultProvider: id,
        defaultModel: model,
      },
    }, null, 2);
  }
  const model = provider.model.trim() || "gpt-5.5";
  const name = provider.providerName.trim() || "your-provider";
  // Codex live config follows cc-switch: all third-party providers are applied as `custom`.
  const providerKey = "custom";
  const baseUrl = provider.baseUrl.trim().replace(/\/+$/, "") || "https://example.com/v1";
  const wireApi = provider.wireApi || "responses";
  const source = state?.configText?.trimEnd() || "";
  const sourceLines = source ? source.split("\n") : [];
  const keptLines: string[] = [];
  let currentSection = "";
  let skippingCustomProvider = false;
  let hasReasoningEffort = false;

  for (const line of sourceLines) {
    const sectionMatch = line.match(/^\s*\[([^\]]+)]\s*$/);
    if (sectionMatch) {
      currentSection = sectionMatch[1].trim();
      skippingCustomProvider = currentSection === `model_providers.${providerKey}`;
      if (skippingCustomProvider) continue;
    }
    if (skippingCustomProvider) continue;

    if (!currentSection) {
      const keyMatch = line.match(/^\s*([A-Za-z0-9_-]+)\s*=/);
      const key = keyMatch?.[1];
      if (key === "model_provider" || key === "model") continue;
      if (key === "model_reasoning_effort") hasReasoningEffort = true;
    }
    keptLines.push(line);
  }

  const firstSectionIndex = keptLines.findIndex((line) => /^\s*\[[^\]]+]\s*$/.test(line));
  const rootLines = (firstSectionIndex === -1 ? keptLines : keptLines.slice(0, firstSectionIndex)).filter((line, index, lines) => {
    if (line.trim()) return true;
    return index > 0 && index < lines.length - 1;
  });
  const sectionLines = firstSectionIndex === -1 ? [] : keptLines.slice(firstSectionIndex).filter((line, index, lines) => {
    if (line.trim()) return true;
    return index > 0 && index < lines.length - 1;
  });

  const headerLines = [
    `model_provider = "${tomlEscape(providerKey)}"`,
    `model = "${tomlEscape(model)}"`,
  ];
  if (!hasReasoningEffort) {
    headerLines.push('model_reasoning_effort = "high"');
  }

  const providerLines = [
    `[model_providers.${providerKey}]`,
    `name = "${tomlEscape(name)}"`,
    `base_url = "${tomlEscape(baseUrl)}"`,
    `wire_api = "${tomlEscape(wireApi)}"`,
    `requires_openai_auth = ${provider.requiresOpenaiAuth ? "true" : "false"}`,
  ];

  return [
    ...headerLines,
    ...(rootLines.length ? ["", ...rootLines] : []),
    "",
    ...providerLines,
    ...(sectionLines.length ? ["", ...sectionLines] : []),
  ].join("\n");
}


function buildProviderAuthPreview(provider: SavedProvider) {
  const key = provider.apiKey?.trim();
  if (provider.appType === "claude") {
    return JSON.stringify({
      ANTHROPIC_AUTH_TOKEN: key || (provider.hasApiKey ? "[REDACTED]" : null),
    }, null, 2);
  }
  if (provider.appType === "grok") {
    return JSON.stringify({ api_key: key || (provider.hasApiKey ? "[REDACTED]" : null) }, null, 2);
  }
  return JSON.stringify({ OPENAI_API_KEY: key || (provider.hasApiKey ? "[REDACTED]" : null), auth_mode: key ? "apikey" : undefined }, null, 2);
}


function instructionIdFromPath(path: string | undefined, templates: InstructionTemplate[]) {
  if (!path) return "";
  const normalized = path.replace(/\\/g, "/");
  const found = templates.find((item) => normalized.toLowerCase().endsWith(item.filename.toLowerCase()));
  return found?.id || "custom";
}

function uniqueBuiltinPromptStatuses(statuses: BuiltinPromptStatus[]) {
  const otherToolPromptFilenames = new Set([
    "claude-project-rules.md",
    "grok-unrestricted.md",
    "zcode-system-role.md",
  ]);
  const sourcePriority: Record<string, number> = {
    unavailable: 0,
    bundled: 1,
    cache: 2,
    removed: 2,
    github: 3,
  };
  const seenIds = new Set<string>();
  const seenFilenames = new Set<string>();
  const selected = statuses
    .map((item, index) => ({ item, index }))
    .filter(({ item }) =>
      item.id.trim()
      && item.filename.trim()
      && !otherToolPromptFilenames.has(item.filename.trim().toLowerCase()),
    )
    .sort((a, b) =>
      (sourcePriority[b.item.contentSource] ?? -1) - (sourcePriority[a.item.contentSource] ?? -1)
      || a.index - b.index,
    )
    .filter(({ item }) => {
      const id = item.id.trim().toLowerCase();
      const filename = item.filename.trim().toLowerCase();
      if (seenIds.has(id) || seenFilenames.has(filename)) return false;
      seenIds.add(id);
      seenFilenames.add(filename);
      return true;
    });
  return selected.sort((a, b) => a.index - b.index).map(({ item }) => item);
}

function JsonPreview({ text }: { text: string }) {
  return (
    <pre className="toml-preview json-preview" aria-label="JSON preview">
      {text.split("\n").map((line, index) => (
        <div className="toml-line" key={index}>
          <span className="toml-line-no">{index + 1}</span>
          <code>{line}</code>
        </div>
      ))}
    </pre>
  );
}

function renderTomlValue(value: string, lineKey: string) {
  const parts = value.split(/("(?:\\.|[^"])*")/g);
  return parts.map((part, index) => {
    if (!part) return null;
    const key = `${lineKey}-v-${index}`;
    if (/^"(?:\\.|[^"])*"$/.test(part)) {
      return <span className="toml-string" key={key}>{part}</span>;
    }
    const boolParts = part.split(/\b(true|false)\b/g);
    return boolParts.map((piece, boolIndex) => {
      if (piece === "true" || piece === "false") {
        return <span className="toml-bool" key={`${key}-b-${boolIndex}`}>{piece}</span>;
      }
      return <React.Fragment key={`${key}-t-${boolIndex}`}>{piece}</React.Fragment>;
    });
  });
}

function renderTomlLine(line: string, index: number) {
  const key = `toml-${index}`;
  if (line.trim().startsWith("#")) {
    return <span className="toml-comment">{line}</span>;
  }
  if (/^\s*\[[^\]]+\]\s*$/.test(line)) {
    return <span className="toml-section">{line}</span>;
  }
  const eqIndex = line.indexOf("=");
  if (eqIndex > -1) {
    const left = line.slice(0, eqIndex);
    const right = line.slice(eqIndex + 1);
    return (
      <>
        <span className="toml-key">{left}</span>
        <span className="toml-eq">=</span>
        {renderTomlValue(right, key)}
      </>
    );
  }
  return <>{line}</>;
}

function TomlPreview({ text }: { text: string }) {
  return (
    <pre className="toml-preview" aria-label="TOML preview">
      {text.split("\n").map((line, index) => (
        <div className="toml-line" key={index}>
          <span className="toml-line-no">{index + 1}</span>
          <code>{renderTomlLine(line, index)}</code>
        </div>
      ))}
    </pre>
  );
}

function PlainPreview({ text }: { text: string }) {
  return (
    <pre className="toml-preview plain-preview" aria-label="Text preview">
      {text.split("\n").map((line, index) => (
        <div className="toml-line" key={index}>
          <span className="toml-line-no">{index + 1}</span>
          <code>{line}</code>
        </div>
      ))}
    </pre>
  );
}

type LoadResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

async function settleLoad<T>(promise: Promise<T>): Promise<LoadResult<T>> {
  try {
    return { ok: true, data: await promise };
  } catch (error) {
    return { ok: false, error: String(error) };
  }
}

function App() {
  const initialLang = (localStorage.getItem(LANG_KEY) as Lang | null) || "zh";
  const [lang, setLang] = React.useState<Lang>(initialLang === "en" ? "en" : "zh");
  const [theme, setTheme] = React.useState<AppTheme>(() =>
    localStorage.getItem(THEME_KEY) === "dark" ? "dark" : "light",
  );
  const t = dict[lang];
  const updater = useAppUpdater();
  const isMacRuntime = navigator.userAgent.toLowerCase().includes("mac");
  const skinCenterEnabled = SKIN_CENTER_ENABLED;
  const [tab, setTab] = React.useState<Tab>("dashboard");
  const [activeTool, setActiveTool] = React.useState<ToolId>(storedTool);
  const [toolStatuses, setToolStatuses] = React.useState<ToolStatus[]>([]);
  const [toolConfig, setToolConfig] = React.useState<ToolConfigBundle | null>(null);
  const [configFileId, setConfigFileId] = React.useState("");
  const [visitedTabs, setVisitedTabs] = React.useState<Set<Tab>>(() => new Set(["dashboard"]));
  const [providerMode, setProviderMode] = React.useState<ProviderMode>("list");
  const [instructionMode, setInstructionMode] = React.useState<InstructionMode>("list");
  const [promptInjectionModes, setPromptInjectionModes] = React.useState<Record<PromptEngine, PromptInjectionMode>>(() => ({
    codex: storedPromptInjectionMode("codex"),
    claude: storedPromptInjectionMode("claude"),
    zcode: storedPromptInjectionMode("zcode"),
    grok: storedPromptInjectionMode("grok"),
    kilo: storedPromptInjectionMode("kilo"),
    pi: storedPromptInjectionMode("pi"),
  }));
  const [skillsMcpTab, setSkillsMcpTab] = React.useState<"mcp" | "skills">("mcp");
  const [editingProviderId, setEditingProviderId] = React.useState<string | null>(null);
  const [editingPromptId, setEditingPromptId] = React.useState<string | null>(null);
  const [savedProviders, setSavedProviders] = React.useState<SavedProvider[]>([]);
  const [activeProviderId, setActiveProviderId] = React.useState(() => {
    const tool = storedTool();
    return localStorage.getItem(activeProviderKey(tool))
      || (tool === "codex" ? localStorage.getItem(ACTIVE_PROVIDER_KEY) : null)
      || "";
  });
  const [savedPrompts, setSavedPrompts] = React.useState<SavedPrompt[]>([]);
  const [builtinPromptStatus, setBuiltinPromptStatus] = React.useState<BuiltinPromptStatus[]>([]);
  const [aboutInfo, setAboutInfo] = React.useState<AboutInfo | null>(null);
  const [releaseInfo, setReleaseInfo] = React.useState<ReleaseInfo>({ status: "idle" });
  const [updatePromptOpen, setUpdatePromptOpen] = React.useState(false);
  const [sessionStatus, setSessionStatus] = React.useState<SessionSyncStatus | null>(null);
  const [toolSessionList, setToolSessionList] = React.useState<ToolSessionList | null>(null);
  const [skillsMcpState, setSkillsMcpState] = React.useState<SkillsMcpState | null>(null);
  const [skillsMcpImportPreview, setSkillsMcpImportPreview] = React.useState<SkillsMcpImportPreview | null>(null);
  const [skillsMcpImportOpen, setSkillsMcpImportOpen] = React.useState(false);
  const [startupDiagnostics, setStartupDiagnostics] = React.useState<StartupDiagnostics | null>(null);
  const [startupWizardOpen, setStartupWizardOpen] = React.useState(() => localStorage.getItem(STARTUP_WIZARD_SEEN_KEY) !== "1");
  const [startupClosing, setStartupClosing] = React.useState(false);
  const [sessionQuery, setSessionQuery] = React.useState("");
  const deferredSessionQuery = React.useDeferredValue(sessionQuery);
  const [sessionGroupByCwd, setSessionGroupByCwd] = React.useState(false);
  const [showInternalSessions, setShowInternalSessions] = React.useState(false);
  const [selectedSessionIds, setSelectedSessionIds] = React.useState<string[]>([]);
  const [sessionDeleteConfirmOpen, setSessionDeleteConfirmOpen] = React.useState(false);
  const [sessionDeleteBusy, setSessionDeleteBusy] = React.useState(false);
  const [sessionDeleteSafetyConfirmed, setSessionDeleteSafetyConfirmed] = React.useState(false);
  const [state, setState] = React.useState<CodexState | null>(null);
  const [configDir, setConfigDir] = React.useState("");
  const [loading, setLoading] = React.useState(false);
  const [toast, setToast] = React.useState<string>("");
  const [error, setError] = React.useState<string>("");
  const [providerForm, setProviderForm] = React.useState<SavedProvider>(() => blankProviderForTool(storedTool()));
  const [providerTomlDraft, setProviderTomlDraft] = React.useState("");
  const [providerTomlDirty, setProviderTomlDirty] = React.useState(false);
  const [providerApiKeyVisible, setProviderApiKeyVisible] = React.useState(false);
  const [providerTestingId, setProviderTestingId] = React.useState("");
  const [availableProviderModels, setAvailableProviderModels] = React.useState<ProviderModel[]>([]);
  const [providerModelsLoading, setProviderModelsLoading] = React.useState(false);
  const [actionBusy, setActionBusy] = React.useState<string>("");
  const [promptSyncing, setPromptSyncing] = React.useState(false);
  const [promptCatalogReady, setPromptCatalogReady] = React.useState(false);
  const [promptForm, setPromptForm] = React.useState<SavedPrompt>(blankPromptForm);
  const [officialForm, setOfficialForm] = React.useState({ model: "gpt-5.5", authJson: "" });
  const [promptModeHelpOpen, setPromptModeHelpOpen] = React.useState(false);
  const [promptEngine, setPromptEngine] = React.useState<PromptEngine>(storedPromptEngine);
  const [promptBackups, setPromptBackups] = React.useState<PromptBackupEntry[]>([]);
  const [promptBackupsOpen, setPromptBackupsOpen] = React.useState(false);
  const [promptBackupsLoading, setPromptBackupsLoading] = React.useState(false);
  const [promptRestoreBusyId, setPromptRestoreBusyId] = React.useState("");
  const [claudeState, setClaudeState] = React.useState<ClaudeState | null>(null);
  const [claudeSavedPrompts, setClaudeSavedPrompts] = React.useState<SavedPrompt[]>([]);
  const [claudeBuiltinStatus, setClaudeBuiltinStatus] = React.useState<BuiltinPromptStatus[]>([]);
  const [zcodeState, setZcodeState] = React.useState<ZcodeState | null>(null);
  const [zcodeSavedPrompts, setZcodeSavedPrompts] = React.useState<SavedPrompt[]>([]);
  const [zcodeBuiltinStatus, setZcodeBuiltinStatus] = React.useState<BuiltinPromptStatus[]>([]);
  const [grokState, setGrokState] = React.useState<GrokState | null>(null);
  const [grokSavedPrompts, setGrokSavedPrompts] = React.useState<SavedPrompt[]>([]);
  const [grokBuiltinStatus, setGrokBuiltinStatus] = React.useState<BuiltinPromptStatus[]>([]);
  const [kiloState, setKiloState] = React.useState<KiloState | null>(null);
  const [kiloSavedPrompts, setKiloSavedPrompts] = React.useState<SavedPrompt[]>([]);
  const [kiloBuiltinStatus, setKiloBuiltinStatus] = React.useState<BuiltinPromptStatus[]>([]);
  const [piState, setPiState] = React.useState<PiState | null>(null);
  const [piSavedPrompts, setPiSavedPrompts] = React.useState<SavedPrompt[]>([]);
  const [piBuiltinStatus, setPiBuiltinStatus] = React.useState<BuiltinPromptStatus[]>([]);
  const activeToolRef = React.useRef(activeTool);
  activeToolRef.current = activeTool;
  const autoUpdateCheckedRef = React.useRef(false);
  const promptImportRef = React.useRef<HTMLInputElement | null>(null);
  const skillZipImportRef = React.useRef<HTMLInputElement | null>(null);
  const providerTomlEditorRef = React.useRef<HTMLTextAreaElement | null>(null);
  const providerModelsRequestRef = React.useRef(0);
  const refreshRequestRef = React.useRef(0);
  const sessionsRequestRef = React.useRef(0);
  const skillsMcpRequestRef = React.useRef(0);
  const toolConfigRequestRef = React.useRef(0);
  const promptModeHelpRef = React.useRef<HTMLDivElement | null>(null);
  const promptRefreshRequestRef = React.useRef(0);
  const promptRefreshInFlightRef = React.useRef<Promise<BuiltinPromptStatus[]> | null>(null);
  const promptAutoRefreshAttemptedRef = React.useRef(false);
  const promptCatalogReadyRef = React.useRef(false);
  const promptModeSyncedRef = React.useRef<Partial<Record<PromptEngine, string>>>({});
  const skillsMcpLoadedRef = React.useRef(false);
  const skinShutdownAttemptedRef = React.useRef(false);
  const themeTransitionTimerRef = React.useRef<number | null>(null);
  const promptInjectionMode = promptInjectionModes[promptEngine];
  const setPromptInjectionMode = React.useCallback((mode: PromptInjectionMode) => {
    setPromptInjectionModes((current) => ({ ...current, [promptEngine]: mode }));
  }, [promptEngine]);
  const {
    state: skinCenterState,
    restartRequest: skinRestartRequest,
    pauseBusy: skinPauseBusy,
    zipInputRef: skinZipImportRef,
    imageInputRef: skinImageInputRef,
    refresh: refreshSkinCenter,
    importZip: importSkinThemeZip,
    createFromImage: createSkinThemeFromImage,
    updateSettings: updateSkinThemeSettings,
    apply: enableSkinTheme,
    pause: pauseSkinTheme,
    confirmRestart: confirmSkinRestart,
    closeRestart: closeSkinRestart,
    exportTheme: exportSkinTheme,
  } = useSkinCenter({
    enabled: skinCenterEnabled,
    lang,
    tab,
    ready: Boolean(state),
    setActionBusy,
    setError,
    setToast,
  });
  const providerTomlPreview = React.useMemo(() => buildProviderTomlPreview(providerForm, state), [providerForm, state]);
  const providerAuthPreview = React.useMemo(() => buildProviderAuthPreview(providerForm), [providerForm]);
  const selectedToolConfigFile = React.useMemo(
    () => toolConfig?.files.find((file) => file.id === configFileId)
      || toolConfig?.files.find((file) => file.id === toolConfig.primaryFileId)
      || toolConfig?.files[0],
    [configFileId, toolConfig],
  );
  const activeBuiltinTemplateId = state?.instructionTemplateKey?.startsWith("builtin:")
    ? state.instructionTemplateKey.slice("builtin:".length)
    : "";
  const instructionTemplates = React.useMemo<InstructionTemplate[]>(() => {
    if (!builtinPromptStatus.length) return bundledInstructionTemplates;
    return builtinPromptStatus
      .filter((item) => item.contentSource !== "removed" || item.id === activeBuiltinTemplateId)
      .map(({ id, filename, title, subtitle, badge }) => ({ id, filename, title, subtitle, badge }));
  }, [activeBuiltinTemplateId, builtinPromptStatus]);
  const missingActiveBuiltinTemplateId = activeBuiltinTemplateId
    && !instructionTemplates.some((item) => item.id === activeBuiltinTemplateId)
    ? activeBuiltinTemplateId
    : "";
  const currentInstructionId = instructionIdFromPath(state?.instructionFile, instructionTemplates);
  const releaseStatusLabel = React.useMemo(() => {
    if (updater.state.phase === "downloading") return lang === "zh" ? "下载中" : "Downloading";
    if (updater.state.phase === "installing") return lang === "zh" ? "安装中" : "Installing";
    if (updater.state.phase === "ready") return lang === "zh" ? "等待重启" : "Restart required";
    if (releaseInfo.status === "checking") return lang === "zh" ? "检查中" : "Checking";
    if (releaseInfo.status === "error") return lang === "zh" ? "失败" : "Failed";
    if (releaseInfo.hasUpdate) return lang === "zh" ? "有更新" : "Update found";
    if (releaseInfo.status === "ok") return lang === "zh" ? "已是最新" : "Up to date";
    return lang === "zh" ? "未检查" : "Idle";
  }, [lang, releaseInfo.hasUpdate, releaseInfo.status, updater.state.phase]);

  React.useEffect(() => {
    localStorage.setItem(LANG_KEY, lang);
  }, [lang]);

  React.useLayoutEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  React.useEffect(() => () => {
    if (themeTransitionTimerRef.current !== null) {
      window.clearTimeout(themeTransitionTimerRef.current);
    }
    document.documentElement.classList.remove("cx-theme-view-transition", "cx-theme-fallback-transition");
  }, []);

  const toggleTheme = React.useCallback(() => {
    const nextTheme: AppTheme = theme === "dark" ? "light" : "dark";
    const root = document.documentElement;
    const commitTheme = () => {
      flushSync(() => setTheme(nextTheme));
    };

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      commitTheme();
      return;
    }

    const transitionDocument = document as ThemeTransitionDocument;
    if (typeof transitionDocument.startViewTransition === "function") {
      root.classList.remove("cx-theme-fallback-transition");
      root.classList.add("cx-theme-view-transition");
      try {
        const transition = transitionDocument.startViewTransition(commitTheme);
        const clearTransitionClass = () => root.classList.remove("cx-theme-view-transition");
        void transition.finished.then(clearTransitionClass, clearTransitionClass);
        return;
      } catch {
        root.classList.remove("cx-theme-view-transition");
      }
    }

    root.classList.add("cx-theme-fallback-transition");
    commitTheme();
    if (themeTransitionTimerRef.current !== null) {
      window.clearTimeout(themeTransitionTimerRef.current);
    }
    themeTransitionTimerRef.current = window.setTimeout(() => {
      root.classList.remove("cx-theme-fallback-transition");
      themeTransitionTimerRef.current = null;
    }, 260);
  }, [theme]);

  React.useEffect(() => {
    (Object.keys(promptInjectionModes) as PromptEngine[]).forEach((engine) => {
      localStorage.setItem(promptInjectionModeKey(engine), promptInjectionModes[engine]);
    });
    localStorage.setItem(PROMPT_INJECTION_MODE_KEY, promptInjectionModes.codex);
  }, [promptInjectionModes]);

  React.useEffect(() => {
    localStorage.setItem(PROMPT_ENGINE_KEY, promptEngine);
  }, [promptEngine]);

  React.useEffect(() => {
    localStorage.setItem(ACTIVE_TOOL_KEY, activeTool);
  }, [activeTool]);

  React.useEffect(() => {
    sessionsRequestRef.current += 1;
    skillsMcpRequestRef.current += 1;
    toolConfigRequestRef.current += 1;
    setProviderMode("list");
    setEditingProviderId(null);
    setProviderForm(blankProviderForTool(activeTool));
    setProviderTomlDraft("");
    setProviderTomlDirty(false);
    setProviderApiKeyVisible(false);
    setSavedProviders([]);
    setActiveProviderId(
      localStorage.getItem(activeProviderKey(activeTool))
      || (activeTool === "codex" ? localStorage.getItem(ACTIVE_PROVIDER_KEY) : null)
      || "",
    );
    setAvailableProviderModels([]);
    setProviderModelsLoading(false);
    setToolConfig(null);
    setConfigFileId("");
    setToolSessionList(null);
    setSessionStatus(null);
    setSessionQuery("");
    setSelectedSessionIds([]);
    setSessionDeleteConfirmOpen(false);
    setSessionDeleteSafetyConfirmed(false);
    setSkillsMcpState(null);
    setSkillsMcpImportPreview(null);
    setSkillsMcpImportOpen(false);
    setActionBusy("");
    skillsMcpLoadedRef.current = false;
  }, [activeTool]);

  React.useEffect(() => {
    if (error) setToast("");
  }, [error]);

  React.useEffect(() => {
    if (!promptModeHelpOpen) return undefined;
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && promptModeHelpRef.current?.contains(target)) return;
      setPromptModeHelpOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPromptModeHelpOpen(false);
    };
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [promptModeHelpOpen]);

  React.useLayoutEffect(() => {
    if (providerMode !== "form") return;
    const editor = providerTomlEditorRef.current;
    if (!editor) return;
    editor.style.height = "0px";
    editor.style.height = `${Math.max(560, editor.scrollHeight)}px`;
  }, [providerMode, providerTomlDraft]);

  React.useEffect(() => {
    const engines: Array<{
      engine: PromptEngine;
      scope?: string;
      mode?: PromptInjectionMode;
    }> = [
      { engine: "codex", scope: state?.codexDir, mode: state?.instructionInjectionMode },
      { engine: "claude", scope: claudeState?.claudeDir, mode: claudeState?.instructionInjectionMode },
      { engine: "zcode", scope: zcodeState?.managedDir, mode: zcodeState?.instructionInjectionMode },
      { engine: "grok", scope: grokState?.grokDir, mode: grokState?.instructionInjectionMode },
      { engine: "kilo", scope: kiloState?.kiloDir, mode: kiloState?.instructionInjectionMode },
      { engine: "pi", scope: piState?.piDir, mode: piState?.instructionInjectionMode },
    ];
    const pending = engines.filter(({ engine, scope }) =>
      Boolean(scope) && promptModeSyncedRef.current[engine] !== scope,
    );
    if (!pending.length) return;
    pending.forEach(({ engine, scope }) => {
      promptModeSyncedRef.current[engine] = scope;
    });
    setPromptInjectionModes((current) => {
      let changed = false;
      const next = { ...current };
      pending.forEach(({ engine, mode }) => {
        if (mode && next[engine] !== mode) {
          next[engine] = mode;
          changed = true;
        }
      });
      return changed ? next : current;
    });
  }, [
    claudeState?.claudeDir,
    claudeState?.instructionInjectionMode,
    grokState?.grokDir,
    grokState?.instructionInjectionMode,
    kiloState?.instructionInjectionMode,
    kiloState?.kiloDir,
    piState?.instructionInjectionMode,
    piState?.piDir,
    state?.codexDir,
    state?.instructionInjectionMode,
    zcodeState?.instructionInjectionMode,
    zcodeState?.managedDir,
  ]);

  React.useEffect(() => {
    setVisitedTabs((tabs) => {
      if (tabs.has(tab)) return tabs;
      const next = new Set(tabs);
      next.add(tab);
      return next;
    });
  }, [tab]);

  React.useEffect(() => {
    if (skinCenterEnabled || skinShutdownAttemptedRef.current) return;
    skinShutdownAttemptedRef.current = true;
    void invoke("pause_skin_theme").catch((error) => {
      setError(lang === "zh"
        ? `皮肤中心已暂时关闭，但未能自动停用现有皮肤：${String(error)}`
        : `The skin center is temporarily disabled, but the active skin could not be stopped: ${String(error)}`);
    });
  }, [lang, skinCenterEnabled]);

  React.useEffect(() => {
    if (providerMode === "form" && !providerTomlDirty) {
      setProviderTomlDraft(providerTomlPreview);
    }
  }, [providerMode, providerTomlDirty, providerTomlPreview]);

  const currentProvider = state?.providers.find((p) => p.isCurrent);
  const activeToolStatus = toolStatuses.find((item) => item.id === activeTool);
  // `state` 只描述 Codex。概览页必须优先读各工具自己的 toolStatus，
  // 否则 Codex 的模型/供应商会串到 Claude / Grok / ZCode 的概览卡片上。
  const codexState = activeTool === "codex" ? state : null;
  const liveProviderId = (state?.modelProvider || "openai").trim();
  const liveCustomProvider = React.useMemo(() => (state?.providers || []).find((item) => item.id === "custom"), [state?.providers]);
  const liveProviderApiKey = React.useMemo(() => {
    const configKey = extractTomlProviderApiKey(state?.configText, liveProviderId);
    const authKey = extractOpenAiApiKey(state?.authText).trim();
    return configKey || authKey;
  }, [liveProviderId, state?.authText, state?.configText]);
  const inferredActiveProviderId = React.useMemo(() => {
    if (activeTool !== "codex") return "";
    if (liveProviderId !== "custom") return "";
    const liveIdentity = providerIdentityKey(liveCustomProvider?.baseUrl, liveProviderApiKey, liveCustomProvider?.name || liveCustomProvider?.id);
    if (!liveIdentity) return "";
    const identityMatches = savedProviders.filter((item) =>
      providerIdentityKey(item.baseUrl, savedProviderApiKey(item), item.providerName) === liveIdentity,
    );
    const remembered = identityMatches.find((item) => item.id === activeProviderId);
    if (remembered) return remembered.id;
    const backendMatch = identityMatches.find((item) => item.id === state?.activeSavedProviderId);
    return backendMatch?.id || identityMatches[0]?.id || "";
  }, [activeProviderId, activeTool, liveCustomProvider?.baseUrl, liveCustomProvider?.id, liveCustomProvider?.name, liveProviderApiKey, liveProviderId, savedProviders, state?.activeSavedProviderId]);
  const effectiveActiveProviderId = activeTool === "codex"
    ? (liveProviderId === "custom" ? inferredActiveProviderId : liveProviderId)
    : activeProviderId;
  const currentInstructionPath = (state?.instructionFile || "").replace(/\\/g, "/");
  const currentInstructionFilename = currentInstructionPath.split("/").pop() || "";
  const activeInstructionTitle = React.useMemo(() => {
    const templateKey = state?.instructionTemplateKey || "";
    if (templateKey.startsWith("builtin:")) {
      const id = templateKey.slice("builtin:".length);
      return instructionTemplates.find((item) => item.id === id)?.title || id;
    }
    if (templateKey.startsWith("saved:")) {
      const id = templateKey.slice("saved:".length);
      return savedPrompts.find((item) => item.id === id)?.title || id;
    }
    return savedPrompts.find((item) => item.filename === currentInstructionFilename)?.title
      || instructionTemplates.find((item) => item.filename === currentInstructionFilename)?.title
      || currentInstructionFilename
      || (lang === "zh" ? "当前提示词" : "Current prompt");
  }, [currentInstructionFilename, instructionTemplates, lang, savedPrompts, state?.instructionTemplateKey]);

  // ─── Claude 派生值 ─────────────────────────────────────────────────────
  const claudeInstructionTemplates = React.useMemo<InstructionTemplate[]>(() => {
    return claudeBuiltinStatus
      .filter((item) => item.contentSource !== "removed")
      .map(({ id, filename, title, subtitle, badge }) => ({ id, filename, title, subtitle, badge }));
  }, [claudeBuiltinStatus]);
  const claudeActiveBuiltinTemplateId = claudeState?.instructionTemplateKey?.startsWith("builtin:")
    ? claudeState.instructionTemplateKey.slice("builtin:".length)
    : "";
  const claudeActiveInstructionTitle = React.useMemo(() => {
    const title = claudeState?.activeInstructionTitle;
    if (title) return title;
    const templateKey = claudeState?.instructionTemplateKey || "";
    if (templateKey.startsWith("saved:")) {
      const id = templateKey.slice("saved:".length);
      return claudeSavedPrompts.find((item) => item.id === id)?.title || id;
    }
    return lang === "zh" ? "当前提示词" : "Current prompt";
  }, [claudeSavedPrompts, claudeState?.activeInstructionTitle, claudeState?.instructionTemplateKey, lang]);
  const claudeManagedSavedPromptId = claudeState?.instructionTemplateKey?.startsWith("saved:")
    ? claudeState.instructionTemplateKey.slice("saved:".length)
    : null;

  // ─── ZCode 派生值 ─────────────────────────────────────────────────────
  const zcodeInstructionTemplates = React.useMemo<InstructionTemplate[]>(() => {
    return zcodeBuiltinStatus
      .filter((item) => item.contentSource !== "removed")
      .map(({ id, filename, title, subtitle, badge }) => ({ id, filename, title, subtitle, badge }));
  }, [zcodeBuiltinStatus]);
  const zcodeActiveInstructionTitle = zcodeState?.activeInstructionTitle
    || (lang === "zh" ? "当前提示词" : "Current prompt");
  const zcodeActiveBuiltinTemplateId = zcodeState?.instructionTemplateKey?.startsWith("builtin:")
    ? zcodeState.instructionTemplateKey.slice("builtin:".length)
    : "";
  const zcodeManagedSavedPromptId = zcodeState?.instructionTemplateKey?.startsWith("saved:")
    ? zcodeState.instructionTemplateKey.slice("saved:".length)
    : null;

  // ─── Grok 派生值 ──────────────────────────────────────────────────────
  const grokInstructionTemplates = React.useMemo<InstructionTemplate[]>(() => {
    return grokBuiltinStatus
      .filter((item) => item.contentSource !== "removed")
      .map(({ id, filename, title, subtitle, badge }) => ({ id, filename, title, subtitle, badge }));
  }, [grokBuiltinStatus]);
  const grokActiveInstructionTitle = grokState?.activeInstructionTitle
    || (lang === "zh" ? "当前提示词" : "Current prompt");
  const grokActiveBuiltinTemplateId = grokState?.instructionTemplateKey?.startsWith("builtin:")
    ? grokState.instructionTemplateKey.slice("builtin:".length)
    : "";
  const grokManagedSavedPromptId = grokState?.instructionTemplateKey?.startsWith("saved:")
    ? grokState.instructionTemplateKey.slice("saved:".length)
    : null;
  // ─── Kilo 派生值 ──────────────────────────────────────────────────────
  const kiloInstructionTemplates = React.useMemo<InstructionTemplate[]>(() => {
    return kiloBuiltinStatus
      .filter((item) => item.contentSource !== "removed")
      .map(({ id, filename, title, subtitle, badge }) => ({ id, filename, title, subtitle, badge }));
  }, [kiloBuiltinStatus]);
  const kiloActiveInstructionTitle = kiloState?.activeInstructionTitle
    || (lang === "zh" ? "当前提示词" : "Current prompt");
  const kiloActiveBuiltinTemplateId = kiloState?.instructionTemplateKey?.startsWith("builtin:")
    ? kiloState.instructionTemplateKey.slice("builtin:".length)
    : "";
  const kiloManagedSavedPromptId = kiloState?.instructionTemplateKey?.startsWith("saved:")
    ? kiloState.instructionTemplateKey.slice("saved:".length)
    : null;
  // ─── Pi 派生值 ────────────────────────────────────────────────────────
  const piInstructionTemplates = React.useMemo<InstructionTemplate[]>(() => {
    return piBuiltinStatus
      .filter((item) => item.contentSource !== "removed")
      .map(({ id, filename, title, subtitle, badge }) => ({ id, filename, title, subtitle, badge }));
  }, [piBuiltinStatus]);
  const piActiveInstructionTitle = piState?.activeInstructionTitle
    || (lang === "zh" ? "当前提示词" : "Current prompt");
  const piActiveBuiltinTemplateId = piState?.instructionTemplateKey?.startsWith("builtin:")
    ? piState.instructionTemplateKey.slice("builtin:".length)
    : "";
  const piManagedSavedPromptId = piState?.instructionTemplateKey?.startsWith("saved:")
    ? piState.instructionTemplateKey.slice("saved:".length)
    : null;
  const activePromptInjectionMode = promptEngine === "claude"
    ? claudeState?.instructionInjectionMode
    : promptEngine === "zcode"
      ? zcodeState?.instructionInjectionMode
      : promptEngine === "grok"
        ? grokState?.instructionInjectionMode
        : promptEngine === "kilo"
          ? kiloState?.instructionInjectionMode
          : promptEngine === "pi"
            ? piState?.instructionInjectionMode
            : state?.instructionInjectionMode;

  const canonicalSavedProviders = React.useMemo(() => {
    const groups = new Map<string, SavedProvider[]>();
    savedProviders.forEach((provider) => {
      const identity = providerIdentityKey(provider.baseUrl, savedProviderApiKey(provider), provider.providerName);
      const key = provider.native ? `native:${provider.id}` : identity || `id:${provider.id}`;
      const group = groups.get(key);
      if (group) group.push(provider);
      else groups.set(key, [provider]);
    });
    return Array.from(groups.values()).map((group) =>
      group.find((item) => item.id === effectiveActiveProviderId)
      || group.find((item) => item.id === activeProviderId)
      || group[0],
    );
  }, [activeProviderId, effectiveActiveProviderId, savedProviders]);

  const detectedRows = React.useMemo<ProviderRow[]>(() => {
    if (activeTool !== "codex") return [];
    return (state?.providers || []).map((p) => {
      return {
        id: `detected-${p.id}`,
        source: "detected" as const,
        providerName: p.name || p.id,
        baseUrl: p.baseUrl || "",
        model: state?.model || "gpt-5.5",
        apiKey: undefined,
        wireApi: p.wireApi || "responses",
        requiresOpenaiAuth: p.requiresOpenaiAuth ?? false,
        isCurrent: p.isCurrent,
      };
    });
  }, [activeTool, state?.model, state?.providers]);

  const localRows = React.useMemo<ProviderRow[]>(() => {
    const activeStatus = toolStatuses.find((status) => status.id === activeTool);
    return canonicalSavedProviders.map((p) => ({
      ...p,
      source: p.native ? "native" as const : "local" as const,
      isCurrent: p.native
        ? activeStatus?.providerId === p.id
        : activeTool === "codex"
          ? effectiveActiveProviderId === p.id
          : activeProviderId === p.id
            || activeStatus?.providerId === p.id
            || activeStatus?.provider === p.providerName
            || activeStatus?.provider === p.baseUrl,
    }));
  }, [activeProviderId, activeTool, canonicalSavedProviders, effectiveActiveProviderId, toolStatuses]);

  const providerRows = React.useMemo<ProviderRow[]>(() => {
    if (activeTool !== "codex") return localRows;
    const officialRow = {
      id: "openai-official",
      source: "official" as const,
      providerName: "OpenAI Official",
      baseUrl: "https://chatgpt.com/codex",
      model: state?.model || "official",
      apiKey: "",
      wireApi: "official",
      requiresOpenaiAuth: false,
      isCurrent: !state?.modelProvider || state.modelProvider === "openai",
    };
    const seen = new Set<string>();
    const rows: ProviderRow[] = [officialRow];
    localRows.forEach((row) => {
      const key = providerIdentityKey(row.baseUrl, savedProviderApiKey(row), row.providerName);
      if (key) seen.add(key);
      rows.push(row);
    });
    detectedRows.forEach((row) => {
      if (row.id === "detected-custom" && inferredActiveProviderId) return;
      const key = providerIdentityKey(row.baseUrl, row.apiKey, row.providerName);
      if (key && seen.has(key)) return;
      if (key) seen.add(key);
      rows.push(row);
    });
    return rows;
  }, [activeTool, detectedRows, inferredActiveProviderId, localRows, state?.model, state?.modelProvider]);

  const findLocalProviderForRow = React.useCallback((row: ProviderRow) => {
    if (row.source === "official") return undefined;
    return canonicalSavedProviders.find((item) =>
      row.source === "local" || row.source === "native"
        ? item.id === row.id
        : providerIdentityKey(item.baseUrl, savedProviderApiKey(item), item.providerName)
          === providerIdentityKey(row.baseUrl, row.apiKey, row.providerName),
    );
  }, [canonicalSavedProviders]);

  const providerPageRows = React.useMemo<ProviderRow[]>(() => providerRows.map((row) => {
    const local = row.source === "official"
      ? undefined
      : canonicalSavedProviders.find((item) =>
        row.source === "local" || row.source === "native"
          ? item.id === row.id
          : providerIdentityKey(item.baseUrl, savedProviderApiKey(item), item.providerName)
            === providerIdentityKey(row.baseUrl, row.apiKey, row.providerName),
      );
    return {
      id: row.id,
      source: row.source,
      providerName: row.providerName,
      baseUrl: row.baseUrl,
      model: row.model,
      models: row.models,
      available: row.available,
      statusMessage: row.statusMessage,
      apiKey: row.apiKey,
      wireApi: row.wireApi,
      requiresOpenaiAuth: row.requiresOpenaiAuth,
      isCurrent: row.isCurrent,
      sourceLabel: row.source === "official"
        ? (lang === "zh" ? "Codex 登录" : "Codex login")
        : row.source === "native"
          ? (lang === "zh" ? "ZCode 原生" : "ZCode native")
          : undefined,
      editable: row.source !== "native" && (row.source === "official" || Boolean(local) || row.source === "detected"),
      deletable: row.source !== "native" && Boolean(local),
      testable: row.source !== "official" && row.source !== "native",
      testingKey: `${row.source}-${row.id}`,
    };
  }), [canonicalSavedProviders, lang, providerRows]);

  const displayedSessionStatus = React.useMemo<SessionSyncStatus | null>(() => {
    if (activeTool === "codex") return sessionStatus;
    if (!toolSessionList) return null;
    const sessions: SessionPreview[] = toolSessionList.sessions.map((item) => ({
      id: item.id,
      title: item.title,
      summary: item.summary,
      modelProvider: toolLabel(activeTool),
      model: null,
      cwd: item.cwd,
      rolloutPath: item.sourcePath,
      updatedAtMs: item.updatedAtMs ?? item.createdAtMs,
      archived: item.archived,
      hasUserEvent: true,
      isSubagent: false,
      needsSync: false,
    }));
    return {
      codexDir: toolSessionList.root,
      targetProvider: toolLabel(activeTool),
      rolloutFiles: sessions.length,
      sessionMetaCount: sessions.length,
      mismatchedRollouts: 0,
      mismatchedSessionMeta: 0,
      sqliteDbs: 0,
      sqliteThreads: sessions.length,
      topLevelThreads: sessions.length,
      subagentThreads: 0,
      mismatchedThreads: 0,
      needsSync: false,
      backupDir: null,
      warnings: toolSessionList.warnings,
      sessions,
    };
  }, [activeTool, sessionStatus, toolSessionList]);

  const visibleSessions = React.useMemo(
    () => (displayedSessionStatus?.sessions || []).filter((item) => showInternalSessions || !item.isSubagent),
    [displayedSessionStatus?.sessions, showInternalSessions],
  );

  const filteredSessions = React.useMemo(() => {
    const query = deferredSessionQuery.trim().toLowerCase();
    if (!query) return visibleSessions;
    return visibleSessions.filter((item) => [item.title, item.summary, item.cwd, item.rolloutPath, item.modelProvider, item.model, item.id]
      .filter(Boolean)
      .some((value) => String(value).toLowerCase().includes(query)));
  }, [deferredSessionQuery, visibleSessions]);

  const allSessionsByCwd = React.useMemo(() => {
    const groups = new Map<string, SessionPreview[]>();
    for (const item of visibleSessions) {
      const key = item.cwd || (lang === "zh" ? "未记录工作目录" : "No workspace recorded");
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(item);
    }
    return groups;
  }, [lang, visibleSessions]);

  const groupedSessions = React.useMemo(() => {
    const groups = new Map<string, SessionPreview[]>();
    if (!sessionGroupByCwd) {
      groups.set(lang === "zh" ? "全部会话" : "All sessions", filteredSessions);
      return Array.from(groups.entries());
    }
    for (const item of filteredSessions) {
      const key = item.cwd || (lang === "zh" ? "未记录工作目录" : "No workspace recorded");
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(item);
    }
    return Array.from(groups.entries()).sort((a, b) => b[1].length - a[1].length);
  }, [filteredSessions, lang, sessionGroupByCwd]);

  const sessionRolloutMismatchCount = displayedSessionStatus?.mismatchedRollouts ?? 0;
  const sessionIndexMismatchCount = displayedSessionStatus?.mismatchedThreads ?? 0;
  const sessionHasMismatches = activeTool === "codex" && Boolean(displayedSessionStatus?.needsSync);
  const sessionTargetProvider = displayedSessionStatus?.targetProvider || state?.modelProvider || "openai";
  const sessionTargetLabel = activeTool === "codex"
    ? canonicalSavedProviders.find((item) => item.id === effectiveActiveProviderId)?.providerName
      || currentProvider?.name
      || sessionTargetProvider
    : toolSessionList?.root || activeToolStatus?.homeDir || toolLabel(activeTool);
  const previewSessionSyncCount = new Set(
    (displayedSessionStatus?.sessions || []).filter((item) => item.needsSync).map((item) => item.id),
  ).size;
  const sessionSyncCount = sessionHasMismatches
    ? Math.max(1, previewSessionSyncCount, sessionRolloutMismatchCount, sessionIndexMismatchCount)
    : 0;
  const sessionVisibleTotal = showInternalSessions
    ? (displayedSessionStatus?.topLevelThreads ?? 0) + (displayedSessionStatus?.subagentThreads ?? 0)
    : (displayedSessionStatus?.topLevelThreads ?? 0);
  const sessionPreviewTruncated = sessionVisibleTotal > visibleSessions.length;
  const selectedSessionSet = React.useMemo(() => new Set(selectedSessionIds), [selectedSessionIds]);
  const selectedSessions = React.useMemo(
    () => (displayedSessionStatus?.sessions || []).filter((item) => selectedSessionSet.has(item.id)),
    [displayedSessionStatus?.sessions, selectedSessionSet],
  );

  React.useEffect(() => {
    setSelectedSessionIds((ids) => ids.filter((id) => (displayedSessionStatus?.sessions || []).some((item) => item.id === id)));
  }, [displayedSessionStatus?.sessions]);

  React.useEffect(() => {
    if (sessionDeleteConfirmOpen && selectedSessions.length === 0) {
      setSessionDeleteConfirmOpen(false);
    }
  }, [selectedSessions.length, sessionDeleteConfirmOpen]);

  const call = React.useCallback(async <T,>(fn: () => Promise<T>, success?: (data: T) => void) => {
    setLoading(true);
    setError("");
    try {
      const data = await fn();
      success?.(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const refresh = React.useCallback(async () => {
    const requestId = refreshRequestRef.current + 1;
    refreshRequestRef.current = requestId;
    const tool = activeTool;
    const resolvedConfigDir = configDir || null;
    setLoading(true);
    setError("");

    const [
      next,
      providerList,
      promptList,
      promptStatus,
      about,
      claudeNext,
      claudePromptList,
      claudeBuiltin,
      zcodeNext,
      zcodePromptList,
      zcodeBuiltin,
      grokNext,
      grokPromptList,
      grokBuiltin,
      kiloNext,
      kiloPromptList,
      kiloBuiltin,
      piNext,
      piPromptList,
      piBuiltin,
      nextToolStatuses,
    ] = await Promise.all([
      settleLoad(invoke<CodexState>("get_codex_state", { configDir: resolvedConfigDir })),
      settleLoad(invoke<SavedProvider[]>("list_saved_providers", { appType: tool })),
      settleLoad(invoke<SavedPrompt[]>("list_saved_prompts")),
      settleLoad(invoke<BuiltinPromptStatus[]>("get_builtin_prompt_status")),
      settleLoad(invoke<AboutInfo>("get_about_info", { configDir: resolvedConfigDir })),
      settleLoad(invoke<ClaudeState>("get_claude_state")),
      settleLoad(invoke<SavedPrompt[]>("list_claude_prompts")),
      settleLoad(invoke<BuiltinPromptStatus[]>("get_claude_builtin_prompt_status")),
      settleLoad(invoke<ZcodeState>("get_zcode_state")),
      settleLoad(invoke<SavedPrompt[]>("list_zcode_prompts")),
      settleLoad(invoke<BuiltinPromptStatus[]>("get_zcode_builtin_prompt_status")),
      settleLoad(invoke<GrokState>("get_grok_state")),
      settleLoad(invoke<SavedPrompt[]>("list_grok_prompts")),
      settleLoad(invoke<BuiltinPromptStatus[]>("get_grok_builtin_prompt_status")),
      settleLoad(invoke<KiloState>("get_kilo_state")),
      settleLoad(invoke<SavedPrompt[]>("list_kilo_prompts")),
      settleLoad(invoke<BuiltinPromptStatus[]>("get_kilo_builtin_prompt_status")),
      settleLoad(invoke<PiState>("get_pi_state")),
      settleLoad(invoke<SavedPrompt[]>("list_pi_prompts")),
      settleLoad(invoke<BuiltinPromptStatus[]>("get_pi_builtin_prompt_status")),
      settleLoad(invoke<ToolStatus[]>("get_tool_statuses", { configDir: resolvedConfigDir })),
    ]);

    if (requestId !== refreshRequestRef.current || tool !== activeToolRef.current) {
      return;
    }

    if (next.ok) setState(next.data);
    if (providerList.ok) setSavedProviders(providerList.data);
    if (promptList.ok) setSavedPrompts(promptList.data);
    if (promptStatus.ok) setBuiltinPromptStatus(uniqueBuiltinPromptStatuses(promptStatus.data));
    if (about.ok) setAboutInfo(about.data);
    if (claudeNext.ok) setClaudeState(claudeNext.data);
    if (claudePromptList.ok) setClaudeSavedPrompts(claudePromptList.data);
    if (claudeBuiltin.ok) setClaudeBuiltinStatus(claudeBuiltin.data);
    if (zcodeNext.ok) setZcodeState(zcodeNext.data);
    if (zcodePromptList.ok) setZcodeSavedPrompts(zcodePromptList.data);
    if (zcodeBuiltin.ok) setZcodeBuiltinStatus(zcodeBuiltin.data);
    if (grokNext.ok) setGrokState(grokNext.data);
    if (grokPromptList.ok) setGrokSavedPrompts(grokPromptList.data);
    if (grokBuiltin.ok) setGrokBuiltinStatus(grokBuiltin.data);
    if (kiloNext.ok) setKiloState(kiloNext.data);
    if (kiloPromptList.ok) setKiloSavedPrompts(kiloPromptList.data);
    if (kiloBuiltin.ok) setKiloBuiltinStatus(kiloBuiltin.data);
    if (piNext.ok) setPiState(piNext.data);
    if (piPromptList.ok) setPiSavedPrompts(piPromptList.data);
    if (piBuiltin.ok) setPiBuiltinStatus(piBuiltin.data);
    if (nextToolStatuses.ok) setToolStatuses(nextToolStatuses.data);

    const failures = [
      next,
      providerList,
      promptList,
      promptStatus,
      about,
      claudeNext,
      claudePromptList,
      claudeBuiltin,
      zcodeNext,
      zcodePromptList,
      zcodeBuiltin,
      grokNext,
      grokPromptList,
      grokBuiltin,
      kiloNext,
      kiloPromptList,
      kiloBuiltin,
      piNext,
      piPromptList,
      piBuiltin,
      nextToolStatuses,
    ].flatMap((result) => result.ok ? [] : [result.error]);
    if (failures.length) {
      setError(`${lang === "zh" ? "部分数据读取失败" : "Some data could not be loaded"}: ${failures.join("; ")}`);
    }

    const sessionRequestId = sessionsRequestRef.current + 1;
    sessionsRequestRef.current = sessionRequestId;
    const [diagnostics, sessions] = await Promise.all([
      settleLoad(invoke<StartupDiagnostics>("get_startup_diagnostics", { configDir: resolvedConfigDir })),
      tool === "codex"
        ? settleLoad<SessionSyncStatus | ToolSessionList>(
          invoke<SessionSyncStatus>("get_session_sync_status", {
            configDir: resolvedConfigDir,
            targetProvider: null,
          }),
        )
        : settleLoad<SessionSyncStatus | ToolSessionList>(
          invoke<ToolSessionList>("get_tool_sessions", { tool, configDir: resolvedConfigDir }),
        ),
    ]);
    if (
      requestId === refreshRequestRef.current
      && sessionRequestId === sessionsRequestRef.current
      && tool === activeToolRef.current
    ) {
      if (diagnostics.ok) setStartupDiagnostics(diagnostics.data);
      if (sessions.ok) {
        if (tool === "codex") {
          setSessionStatus(sessions.data as SessionSyncStatus);
          setToolSessionList(null);
        } else {
          setToolSessionList(sessions.data as ToolSessionList);
          setSessionStatus(null);
        }
      }
    }
    if (requestId === refreshRequestRef.current) {
      setLoading(false);
    }
  }, [activeTool, configDir, lang]);

  const loadPromptBackups = React.useCallback(async (engine: PromptEngine) => {
    setPromptBackupsLoading(true);
    setError("");
    try {
      const entries = await invoke<PromptBackupEntry[]>("list_prompt_backups", {
        engine,
        configDir: engine === "codex" ? configDir || null : null,
      });
      setPromptBackups(entries);
    } catch (e) {
      setError(String(e));
    } finally {
      setPromptBackupsLoading(false);
    }
  }, [configDir]);

  const openPromptBackups = React.useCallback(() => {
    setPromptBackupsOpen(true);
    void loadPromptBackups(promptEngine);
  }, [loadPromptBackups, promptEngine]);

  const closePromptBackups = React.useCallback(() => {
    if (promptRestoreBusyId) return;
    setPromptBackupsOpen(false);
  }, [promptRestoreBusyId]);

  const restorePromptBackup = React.useCallback(async (backupId: string) => {
    setPromptRestoreBusyId(backupId);
    setError("");
    try {
      const result = await invoke<PromptRestoreResult>("restore_prompt_backup", {
        engine: promptEngine,
        configDir: promptEngine === "codex" ? configDir || null : null,
        backupId,
      });
      setToast(result.message);
      await refresh();
      await loadPromptBackups(promptEngine);
    } catch (e) {
      setError(String(e));
    } finally {
      setPromptRestoreBusyId("");
    }
  }, [configDir, loadPromptBackups, promptEngine, refresh]);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  React.useEffect(() => {
    setPromptBackupsOpen(false);
    setPromptBackups([]);
    setPromptRestoreBusyId("");
  }, [promptEngine]);

  React.useEffect(() => {
    if (activeTool !== "codex") return;
    if (!state) return;
    if (liveProviderId !== "custom") {
      if (activeProviderId) {
        localStorage.removeItem(ACTIVE_PROVIDER_KEY);
        localStorage.removeItem(activeProviderKey("codex"));
        setActiveProviderId("");
      }
      return;
    }
    if (!savedProviders.length) return;
    if (inferredActiveProviderId && inferredActiveProviderId !== activeProviderId) {
      localStorage.setItem(ACTIVE_PROVIDER_KEY, inferredActiveProviderId);
      localStorage.setItem(activeProviderKey("codex"), inferredActiveProviderId);
      setActiveProviderId(inferredActiveProviderId);
      return;
    }
    if (activeProviderId && !savedProviders.some((item) => item.id === activeProviderId)) {
      localStorage.removeItem(ACTIVE_PROVIDER_KEY);
      localStorage.removeItem(activeProviderKey("codex"));
      setActiveProviderId("");
    }
  }, [activeProviderId, activeTool, inferredActiveProviderId, liveProviderId, savedProviders, state]);

  const handleActionResult = (result: ActionResult) => {
    setState(result.state);
    setToast(result.message);
    const resolvedConfigDir = configDir || null;
    void Promise.all([
      invoke<SavedPrompt[]>("list_saved_prompts"),
      invoke<SavedProvider[]>("list_saved_providers"),
      invoke<SessionSyncStatus>("get_session_sync_status", { configDir: resolvedConfigDir, targetProvider: null }),
    ])
      .then(([promptList, providerList, sessions]) => {
        setSavedPrompts(promptList);
        setSavedProviders(providerList);
        setSessionStatus(sessions);
      })
      .catch(() => undefined);
  };

  const switchInstructionTemplate = (templateId: string) =>
    call(
      () => invoke<ActionResult>("enable_instruction_template", {
        configDir: configDir || null,
        templateId,
        injectionMode: promptInjectionModes.codex,
      }),
      handleActionResult,
    );

  const disableInstruction = () =>
    call(
      () => invoke<ActionResult>("disable_instruction", { configDir: configDir || null, deleteFile: true }),
      handleActionResult,
    );

  const disableExternalInstruction = () =>
    call(
      () => invoke<ActionResult>("disable_external_instruction", { configDir: configDir || null }),
      handleActionResult,
    );

  const openAddPrompt = () => {
    setEditingPromptId(null);
    setPromptForm({ ...blankPromptForm });
    setInstructionMode("form");
  };

  const openEditPrompt = (prompt: SavedPrompt) => {
    setEditingPromptId(prompt.id);
    setPromptForm(prompt);
    setInstructionMode("form");
  };

  const normalizedPromptForm = (): SavedPrompt => {
    const existing = savedPrompts.filter((item) => item.id !== editingPromptId);
    const requestedFilename = promptForm.filename.trim() || `${providerId(promptForm.title || "prompt")}.md`;
    const filename = editingPromptId ? requestedFilename : uniquePromptFilename(requestedFilename, existing.map((item) => item.filename));
    return {
      ...promptForm,
      id: editingPromptId || uniqueId(promptForm.id || promptForm.title || filename, existing.map((item) => item.id)),
      title: promptForm.title.trim(),
      filename,
      content: promptForm.content,
    };
  };

  const savePromptOnly = () =>
    call(
      async () => {
        await invoke<SavedPrompt>("save_prompt", { prompt: normalizedPromptForm() });
        return invoke<SavedPrompt[]>("list_saved_prompts");
      },
      (promptList) => {
        setSavedPrompts(promptList);
        setInstructionMode("list");
        setEditingPromptId(null);
        setToast(lang === "zh" ? "提示词已保存" : "Prompt saved");
      },
    );

  const enableSavedPrompt = (id: string) =>
    call(() => invoke<ActionResult>("enable_saved_prompt", {
      configDir: configDir || null,
      id,
      injectionMode: promptInjectionModes.codex,
    }), handleActionResult);

  const removeSavedPrompt = (id: string) =>
    call(
      async () => {
        await invoke<void>("delete_saved_prompt", { id });
        return invoke<SavedPrompt[]>("list_saved_prompts");
      },
      (promptList) => {
        setSavedPrompts(promptList);
        setToast(lang === "zh" ? "提示词已删除" : "Prompt deleted");
      },
    );

  const importPromptMd = async (file?: File | null) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".md")) {
      setError(lang === "zh" ? "请选择 .md 提示词文件" : "Please choose a .md prompt file");
      return;
    }
    setActionBusy("importPrompt");
    setLoading(true);
    setError("");
    try {
      const content = await file.text();
      const title = file.name.replace(/\.md$/i, "");
      const filename = uniquePromptFilename(file.name, savedPrompts.map((item) => item.filename));
      await invoke<SavedPrompt>("save_prompt", {
        prompt: {
          id: uniqueId(title, savedPrompts.map((item) => item.id)),
          title: filename.replace(/\.md$/i, ""),
          filename,
          content,
        },
      });
      const promptList = await invoke<SavedPrompt[]>("list_saved_prompts");
      setSavedPrompts(promptList);
      setToast(lang === "zh" ? `已导入提示词：${file.name}` : `Prompt imported: ${file.name}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setActionBusy("");
      if (promptImportRef.current) promptImportRef.current.value = "";
    }
  };

  const refreshBuiltinPrompts = async ({ quiet = false }: { quiet?: boolean } = {}) => {
    const requestId = ++promptRefreshRequestRef.current;
    if (!quiet) promptAutoRefreshAttemptedRef.current = true;
    if (!quiet) setError("");
    try {
      const existingRequest = promptRefreshInFlightRef.current;
      const request = existingRequest || invoke<BuiltinPromptStatus[]>("refresh_builtin_prompts", { configDir: configDir || null });
      if (!existingRequest) {
        promptRefreshInFlightRef.current = request;
        setPromptSyncing(true);
        const clearRequest = () => {
          if (promptRefreshInFlightRef.current !== request) return;
          promptRefreshInFlightRef.current = null;
          setPromptSyncing(false);
        };
        void request.then(clearRequest, clearRequest);
      }
      const list = await request;
      if (requestId !== promptRefreshRequestRef.current) return;
      const uniqueList = uniqueBuiltinPromptStatuses(list);
      const catalogFailed = uniqueList.some((item) => item.syncIssue === "catalog");
      const contentFetchFailures = uniqueList.filter((item) =>
        item.contentSource === "unavailable" || item.syncIssue === "content",
      ).length;
      if (!catalogFailed) {
        promptCatalogReadyRef.current = true;
        setPromptCatalogReady(true);
        setBuiltinPromptStatus(uniqueList);
      } else if (!promptCatalogReadyRef.current) {
        setBuiltinPromptStatus(uniqueList);
      }
      const updated = uniqueList.filter((item) => item.updated).length;
      if (!quiet) {
        setToast(catalogFailed
          ? promptCatalogReadyRef.current
            ? (lang === "zh" ? "在线模板库暂时不可用，已保留当前列表" : "Online templates are unavailable; keeping the current list")
            : (lang === "zh" ? "在线模板库暂时不可用，已使用本地模板" : "Online templates are unavailable; using local templates")
          : contentFetchFailures > 0
            ? (lang === "zh" ? `模板目录已同步，${contentFetchFailures} 个模板暂用本地内容` : `Template catalog synced; ${contentFetchFailures} template(s) are using local content`)
          : updated > 0
            ? (lang === "zh" ? `已同步 ${updated} 个提示词模板` : `${updated} prompt template(s) synced`)
            : (lang === "zh" ? "提示词模板已是最新" : "Prompt templates are up to date"));
      }
    } catch (e) {
      if (requestId === promptRefreshRequestRef.current) {
        if (!quiet) setError(String(e));
      }
    }
  };

  // ─── Claude 指令回调 ───────────────────────────────────────────────────
  // Claude 不走 configDir，直接操作 ~/.claude；无 replace/append 模式区分。
  const handleClaudeActionResult = (result: ClaudeActionResult) => {
    setClaudeState(result.state);
    setToast(result.message);
    void Promise.all([
      invoke<SavedPrompt[]>("list_claude_prompts"),
      invoke<BuiltinPromptStatus[]>("get_claude_builtin_prompt_status"),
    ])
      .then(([promptList, builtin]) => {
        setClaudeSavedPrompts(promptList);
        setClaudeBuiltinStatus(builtin);
      })
      .catch(() => undefined);
  };

  const switchClaudeTemplate = (templateId: string) =>
    call(
      () => invoke<ClaudeActionResult>("enable_claude_instruction", {
        templateId,
        injectionMode: promptInjectionModes.claude,
      }),
      handleClaudeActionResult,
    );

  const disableClaudeInstruction = () =>
    call(
      () => invoke<ClaudeActionResult>("disable_claude_instruction", { deleteFile: true }),
      handleClaudeActionResult,
    );

  const installClaudeRuntime = () =>
    call(
      () => invoke<ClaudeActionResult>("install_claude_runtime"),
      handleClaudeActionResult,
    );

  const uninstallClaudeRuntime = () =>
    call(
      () => invoke<ClaudeActionResult>("uninstall_claude_runtime"),
      handleClaudeActionResult,
    );

  const enableClaudeSavedPrompt = (id: string) =>
    call(
      () => invoke<ClaudeActionResult>("enable_claude_saved_prompt", {
        id,
        injectionMode: promptInjectionModes.claude,
      }),
      handleClaudeActionResult,
    );

  const normalizedClaudePromptForm = (): SavedPrompt => {
    const existing = claudeSavedPrompts.filter((item) => item.id !== editingPromptId);
    const requestedFilename = promptForm.filename.trim() || `${providerId(promptForm.title || "prompt")}.md`;
    const filename = editingPromptId ? requestedFilename : uniquePromptFilename(requestedFilename, existing.map((item) => item.filename));
    return {
      ...promptForm,
      id: editingPromptId || uniqueId(promptForm.id || promptForm.title || filename, existing.map((item) => item.id)),
      title: promptForm.title.trim(),
      filename,
      content: promptForm.content,
    };
  };

  const saveClaudePromptOnly = () =>
    call(
      async () => {
        await invoke<SavedPrompt>("save_claude_prompt", { prompt: normalizedClaudePromptForm() });
        return invoke<SavedPrompt[]>("list_claude_prompts");
      },
      (promptList) => {
        setClaudeSavedPrompts(promptList);
        setInstructionMode("list");
        setEditingPromptId(null);
        setToast(lang === "zh" ? "Claude 提示词已保存" : "Claude prompt saved");
      },
    );

  const removeClaudeSavedPrompt = (id: string) =>
    call(
      async () => {
        await invoke<void>("delete_claude_prompt", { id });
        return invoke<SavedPrompt[]>("list_claude_prompts");
      },
      (promptList) => {
        setClaudeSavedPrompts(promptList);
        setToast(lang === "zh" ? "提示词已删除" : "Prompt deleted");
      },
    );

  const importClaudePromptMd = async (file?: File | null) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".md")) {
      setError(lang === "zh" ? "请选择 .md 提示词文件" : "Please choose a .md prompt file");
      return;
    }
    setActionBusy("importPrompt");
    setLoading(true);
    setError("");
    try {
      const content = await file.text();
      const title = file.name.replace(/\.md$/i, "");
      const filename = uniquePromptFilename(file.name, claudeSavedPrompts.map((item) => item.filename));
      await invoke<SavedPrompt>("save_claude_prompt", {
        prompt: {
          id: uniqueId(title, claudeSavedPrompts.map((item) => item.id)),
          title: filename.replace(/\.md$/i, ""),
          filename,
          content,
        },
      });
      const promptList = await invoke<SavedPrompt[]>("list_claude_prompts");
      setClaudeSavedPrompts(promptList);
      setToast(lang === "zh" ? `已导入提示词：${file.name}` : `Prompt imported: ${file.name}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setActionBusy("");
      if (promptImportRef.current) promptImportRef.current.value = "";
    }
  };

  // ─── ZCode 指令回调 ───────────────────────────────────────────────────
  const handleZcodeActionResult = (result: ZcodeActionResult) => {
    setZcodeState(result.state);
    setToast(result.message);
    void Promise.all([
      invoke<SavedPrompt[]>("list_zcode_prompts"),
      invoke<BuiltinPromptStatus[]>("get_zcode_builtin_prompt_status"),
    ])
      .then(([promptList, builtin]) => {
        setZcodeSavedPrompts(promptList);
        setZcodeBuiltinStatus(builtin);
      })
      .catch(() => undefined);
  };

  const installZcodeInstruction = (templateId: string) =>
    call(
      () => invoke<ZcodeActionResult>("install_zcode_instruction", {
        templateId,
        injectionMode: promptInjectionModes.zcode,
      }),
      handleZcodeActionResult,
    );

  const uninstallZcodeInstruction = () =>
    call(
      () => invoke<ZcodeActionResult>("uninstall_zcode_instruction"),
      handleZcodeActionResult,
    );

  const installZcodeSavedPrompt = (id: string) =>
    call(
      () => invoke<ZcodeActionResult>("install_zcode_saved_prompt", {
        id,
        injectionMode: promptInjectionModes.zcode,
      }),
      handleZcodeActionResult,
    );

  const normalizedZcodePromptForm = (): SavedPrompt => {
    const existing = zcodeSavedPrompts.filter((item) => item.id !== editingPromptId);
    const requestedFilename = promptForm.filename.trim() || `${providerId(promptForm.title || "prompt")}.md`;
    const filename = editingPromptId ? requestedFilename : uniquePromptFilename(requestedFilename, existing.map((item) => item.filename));
    return {
      ...promptForm,
      id: editingPromptId || uniqueId(promptForm.id || promptForm.title || filename, existing.map((item) => item.id)),
      title: promptForm.title.trim(),
      filename,
      content: promptForm.content,
    };
  };

  const saveZcodePromptOnly = () =>
    call(
      async () => {
        await invoke<SavedPrompt>("save_zcode_prompt", { prompt: normalizedZcodePromptForm() });
        return invoke<SavedPrompt[]>("list_zcode_prompts");
      },
      (promptList) => {
        setZcodeSavedPrompts(promptList);
        setInstructionMode("list");
        setEditingPromptId(null);
        setToast(lang === "zh" ? "ZCode 提示词已保存" : "ZCode prompt saved");
      },
    );

  const removeZcodeSavedPrompt = (id: string) =>
    call(
      async () => {
        await invoke<void>("delete_zcode_prompt", { id });
        return invoke<SavedPrompt[]>("list_zcode_prompts");
      },
      (promptList) => {
        setZcodeSavedPrompts(promptList);
        setToast(lang === "zh" ? "提示词已删除" : "Prompt deleted");
      },
    );

  const importZcodePromptMd = async (file?: File | null) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".md")) {
      setError(lang === "zh" ? "请选择 .md 提示词文件" : "Please choose a .md prompt file");
      return;
    }
    setActionBusy("importPrompt");
    setLoading(true);
    setError("");
    try {
      const content = await file.text();
      const title = file.name.replace(/\.md$/i, "");
      const filename = uniquePromptFilename(file.name, zcodeSavedPrompts.map((item) => item.filename));
      await invoke<SavedPrompt>("save_zcode_prompt", {
        prompt: {
          id: uniqueId(title, zcodeSavedPrompts.map((item) => item.id)),
          title: filename.replace(/\.md$/i, ""),
          filename,
          content,
        },
      });
      const promptList = await invoke<SavedPrompt[]>("list_zcode_prompts");
      setZcodeSavedPrompts(promptList);
      setToast(lang === "zh" ? `已导入提示词：${file.name}` : `Prompt imported: ${file.name}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setActionBusy("");
      if (promptImportRef.current) promptImportRef.current.value = "";
    }
  };

  // ─── Grok 指令回调 ────────────────────────────────────────────────────
  const handleGrokActionResult = (result: GrokActionResult) => {
    setGrokState(result.state);
    setToast(result.message);
    void Promise.all([
      invoke<SavedPrompt[]>("list_grok_prompts"),
      invoke<BuiltinPromptStatus[]>("get_grok_builtin_prompt_status"),
    ])
      .then(([promptList, builtin]) => {
        setGrokSavedPrompts(promptList);
        setGrokBuiltinStatus(builtin);
      })
      .catch(() => undefined);
  };

  const installGrokInstruction = (templateId: string) =>
    call(
      () => invoke<GrokActionResult>("install_grok_instruction", {
        templateId,
        injectionMode: promptInjectionModes.grok,
      }),
      handleGrokActionResult,
    );

  const uninstallGrokInstruction = () =>
    call(
      () => invoke<GrokActionResult>("uninstall_grok_instruction"),
      handleGrokActionResult,
    );

  const installGrokSavedPrompt = (id: string) =>
    call(
      () => invoke<GrokActionResult>("install_grok_saved_prompt", {
        id,
        injectionMode: promptInjectionModes.grok,
      }),
      handleGrokActionResult,
    );

  const normalizedGrokPromptForm = (): SavedPrompt => {
    const existing = grokSavedPrompts.filter((item) => item.id !== editingPromptId);
    const requestedFilename = promptForm.filename.trim() || `${providerId(promptForm.title || "prompt")}.md`;
    const filename = editingPromptId ? requestedFilename : uniquePromptFilename(requestedFilename, existing.map((item) => item.filename));
    return {
      ...promptForm,
      id: editingPromptId || uniqueId(promptForm.id || promptForm.title || filename, existing.map((item) => item.id)),
      title: promptForm.title.trim(),
      filename,
      content: promptForm.content,
    };
  };

  const saveGrokPromptOnly = () =>
    call(
      async () => {
        await invoke<SavedPrompt>("save_grok_prompt", { prompt: normalizedGrokPromptForm() });
        return invoke<SavedPrompt[]>("list_grok_prompts");
      },
      (promptList) => {
        setGrokSavedPrompts(promptList);
        setInstructionMode("list");
        setEditingPromptId(null);
        setToast(lang === "zh" ? "Grok 提示词已保存" : "Grok prompt saved");
      },
    );

  const removeGrokSavedPrompt = (id: string) =>
    call(
      async () => {
        await invoke<void>("delete_grok_prompt", { id });
        return invoke<SavedPrompt[]>("list_grok_prompts");
      },
      (promptList) => {
        setGrokSavedPrompts(promptList);
        setToast(lang === "zh" ? "提示词已删除" : "Prompt deleted");
      },
    );

  const importGrokPromptMd = async (file?: File | null) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".md")) {
      setError(lang === "zh" ? "请选择 .md 提示词文件" : "Please choose a .md prompt file");
      return;
    }
    setActionBusy("importPrompt");
    setLoading(true);
    setError("");
    try {
      const content = await file.text();
      const title = file.name.replace(/\.md$/i, "");
      const filename = uniquePromptFilename(file.name, grokSavedPrompts.map((item) => item.filename));
      await invoke<SavedPrompt>("save_grok_prompt", {
        prompt: {
          id: uniqueId(title, grokSavedPrompts.map((item) => item.id)),
          title: filename.replace(/\.md$/i, ""),
          filename,
          content,
        },
      });
      const promptList = await invoke<SavedPrompt[]>("list_grok_prompts");
      setGrokSavedPrompts(promptList);
      setToast(lang === "zh" ? `已导入提示词：${file.name}` : `Prompt imported: ${file.name}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setActionBusy("");
      if (promptImportRef.current) promptImportRef.current.value = "";
    }
  };

  // ─── Kilo 指令回调 ────────────────────────────────────────────────────
  const handleKiloActionResult = (result: KiloActionResult) => {
    setKiloState(result.state);
    setToast(result.message);
    void Promise.all([
      invoke<SavedPrompt[]>("list_kilo_prompts"),
      invoke<BuiltinPromptStatus[]>("get_kilo_builtin_prompt_status"),
    ])
      .then(([promptList, builtin]) => {
        setKiloSavedPrompts(promptList);
        setKiloBuiltinStatus(builtin);
      })
      .catch(() => undefined);
  };

  const installKiloInstruction = (templateId: string) =>
    call(
      () => invoke<KiloActionResult>("install_kilo_instruction", {
        templateId,
        injectionMode: promptInjectionModes.kilo,
      }),
      handleKiloActionResult,
    );

  const uninstallKiloInstruction = () =>
    call(
      () => invoke<KiloActionResult>("uninstall_kilo_instruction"),
      handleKiloActionResult,
    );

  const installKiloSavedPrompt = (id: string) =>
    call(
      () => invoke<KiloActionResult>("install_kilo_saved_prompt", {
        id,
        injectionMode: promptInjectionModes.kilo,
      }),
      handleKiloActionResult,
    );

  const normalizedKiloPromptForm = (): SavedPrompt => {
    const existing = kiloSavedPrompts.filter((item) => item.id !== editingPromptId);
    const requestedFilename = promptForm.filename.trim() || `${providerId(promptForm.title || "prompt")}.md`;
    const filename = editingPromptId ? requestedFilename : uniquePromptFilename(requestedFilename, existing.map((item) => item.filename));
    return {
      ...promptForm,
      id: editingPromptId || uniqueId(promptForm.id || promptForm.title || filename, existing.map((item) => item.id)),
      title: promptForm.title.trim(),
      filename,
      content: promptForm.content,
    };
  };

  const saveKiloPromptOnly = () =>
    call(
      async () => {
        await invoke<SavedPrompt>("save_kilo_prompt", { prompt: normalizedKiloPromptForm() });
        return invoke<SavedPrompt[]>("list_kilo_prompts");
      },
      (promptList) => {
        setKiloSavedPrompts(promptList);
        setInstructionMode("list");
        setEditingPromptId(null);
        setToast(lang === "zh" ? "Kilo 提示词已保存" : "Kilo prompt saved");
      },
    );

  const removeKiloSavedPrompt = (id: string) =>
    call(
      async () => {
        await invoke<void>("delete_kilo_prompt", { id });
        return invoke<SavedPrompt[]>("list_kilo_prompts");
      },
      (promptList) => {
        setKiloSavedPrompts(promptList);
        setToast(lang === "zh" ? "提示词已删除" : "Prompt deleted");
      },
    );

  const importKiloPromptMd = async (file?: File | null) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".md")) {
      setError(lang === "zh" ? "请选择 .md 提示词文件" : "Please choose a .md prompt file");
      return;
    }
    setActionBusy("importPrompt");
    setLoading(true);
    setError("");
    try {
      const content = await file.text();
      const title = file.name.replace(/\.md$/i, "");
      const filename = uniquePromptFilename(file.name, kiloSavedPrompts.map((item) => item.filename));
      await invoke<SavedPrompt>("save_kilo_prompt", {
        prompt: {
          id: uniqueId(title, kiloSavedPrompts.map((item) => item.id)),
          title: filename.replace(/\.md$/i, ""),
          filename,
          content,
        },
      });
      const promptList = await invoke<SavedPrompt[]>("list_kilo_prompts");
      setKiloSavedPrompts(promptList);
      setToast(lang === "zh" ? `已导入提示词：${file.name}` : `Prompt imported: ${file.name}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setActionBusy("");
      if (promptImportRef.current) promptImportRef.current.value = "";
    }
  };

  // ─── Pi 指令回调 ──────────────────────────────────────────────────────
  const handlePiActionResult = (result: PiActionResult) => {
    setPiState(result.state);
    setToast(result.message);
    void Promise.all([
      invoke<SavedPrompt[]>("list_pi_prompts"),
      invoke<BuiltinPromptStatus[]>("get_pi_builtin_prompt_status"),
    ])
      .then(([promptList, builtin]) => {
        setPiSavedPrompts(promptList);
        setPiBuiltinStatus(builtin);
      })
      .catch(() => undefined);
  };

  const installPiInstruction = (templateId: string) =>
    call(
      () => invoke<PiActionResult>("install_pi_instruction", {
        templateId,
        injectionMode: promptInjectionModes.pi,
      }),
      handlePiActionResult,
    );

  const uninstallPiInstruction = () =>
    call(
      () => invoke<PiActionResult>("uninstall_pi_instruction"),
      handlePiActionResult,
    );

  const installPiSavedPrompt = (id: string) =>
    call(
      () => invoke<PiActionResult>("install_pi_saved_prompt", {
        id,
        injectionMode: promptInjectionModes.pi,
      }),
      handlePiActionResult,
    );

  const normalizedPiPromptForm = (): SavedPrompt => {
    const existing = piSavedPrompts.filter((item) => item.id !== editingPromptId);
    const requestedFilename = promptForm.filename.trim() || `${providerId(promptForm.title || "prompt")}.md`;
    const filename = editingPromptId ? requestedFilename : uniquePromptFilename(requestedFilename, existing.map((item) => item.filename));
    return {
      ...promptForm,
      id: editingPromptId || uniqueId(promptForm.id || promptForm.title || filename, existing.map((item) => item.id)),
      title: promptForm.title.trim(),
      filename,
      content: promptForm.content,
    };
  };

  const savePiPromptOnly = () =>
    call(
      async () => {
        await invoke<SavedPrompt>("save_pi_prompt", { prompt: normalizedPiPromptForm() });
        return invoke<SavedPrompt[]>("list_pi_prompts");
      },
      (promptList) => {
        setPiSavedPrompts(promptList);
        setInstructionMode("list");
        setEditingPromptId(null);
        setToast(lang === "zh" ? "Pi 提示词已保存" : "Pi prompt saved");
      },
    );

  const removePiSavedPrompt = (id: string) =>
    call(
      async () => {
        await invoke<void>("delete_pi_prompt", { id });
        return invoke<SavedPrompt[]>("list_pi_prompts");
      },
      (promptList) => {
        setPiSavedPrompts(promptList);
        setToast(lang === "zh" ? "提示词已删除" : "Prompt deleted");
      },
    );

  const importPiPromptMd = async (file?: File | null) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".md")) {
      setError(lang === "zh" ? "请选择 .md 提示词文件" : "Please choose a .md prompt file");
      return;
    }
    setActionBusy("importPrompt");
    setLoading(true);
    setError("");
    try {
      const content = await file.text();
      const title = file.name.replace(/\.md$/i, "");
      const filename = uniquePromptFilename(file.name, piSavedPrompts.map((item) => item.filename));
      await invoke<SavedPrompt>("save_pi_prompt", {
        prompt: {
          id: uniqueId(title, piSavedPrompts.map((item) => item.id)),
          title: filename.replace(/\.md$/i, ""),
          filename,
          content,
        },
      });
      const promptList = await invoke<SavedPrompt[]>("list_pi_prompts");
      setPiSavedPrompts(promptList);
      setToast(lang === "zh" ? `已导入提示词：${file.name}` : `Prompt imported: ${file.name}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setActionBusy("");
      if (promptImportRef.current) promptImportRef.current.value = "";
    }
  };

  const normalizedProviderForm = (): SavedProvider => ({
    ...providerForm,
    appType: activeTool,
    id: editingProviderId || uniqueId(providerForm.id || customProviderId(providerForm.providerName || providerForm.baseUrl), savedProviders.map((item) => item.id)),
    providerName: providerForm.providerName.trim(),
    baseUrl: providerForm.baseUrl.trim().replace(/\/+$/, ""),
    model: providerForm.model.trim(),
    apiKey: (providerForm.apiKey || "").trim(),
    tomlConfig: (providerTomlDraft || providerForm.tomlConfig || buildProviderTomlPreview(providerForm, state)).trimEnd(),
    wireApi: providerForm.wireApi || "responses",
    requiresOpenaiAuth: providerForm.requiresOpenaiAuth,
  });

  const saveProviderOnly = () =>
    call(
      async () => {
        const saved = await invoke<SavedProvider>("save_provider", { provider: normalizedProviderForm() });
        const providerList = await invoke<SavedProvider[]>("list_saved_providers", { appType: activeTool });
        return { saved, providerList };
      },
      ({ providerList }) => {
        setSavedProviders(providerList);
        setProviderMode("list");
        setEditingProviderId(null);
        setProviderTomlDirty(false);
        setToast(lang === "zh" ? "供应商配置已保存" : "Provider saved");
      },
    );

  const switchProvider = (provider: SavedProvider, selectedModel?: string) =>
    call(
      async () => {
        let target = provider;
        const local = savedProviders.find((item) => item.id === provider.id);
        if (!local && !provider.native) {
          target = await invoke<SavedProvider>("save_provider", {
            provider: { ...provider, appType: activeTool },
          });
        }
        const result = await invoke<ToolProviderActionResult>("activate_saved_provider", {
          tool: activeTool,
          id: target.id,
          model: selectedModel || target.model || null,
          configDir: activeTool === "codex" ? configDir || null : null,
        });
        const [providerList, nextStatuses] = await Promise.all([
          invoke<SavedProvider[]>("list_saved_providers", { appType: activeTool }),
          invoke<ToolStatus[]>("get_tool_statuses", { configDir: configDir || null }),
        ]);
        const codexState = activeTool === "codex"
          ? await invoke<CodexState>("get_codex_state", { configDir: configDir || null })
          : null;
        return { result, providerList, nextStatuses, codexState };
      },
      ({ result, providerList, nextStatuses, codexState }) => {
        localStorage.setItem(activeProviderKey(activeTool), result.providerId);
        if (activeTool === "codex") localStorage.setItem(ACTIVE_PROVIDER_KEY, result.providerId);
        setActiveProviderId(result.providerId);
        setSavedProviders(providerList);
        setToolStatuses(nextStatuses);
        if (codexState) setState(codexState);
        setToast(result.message);
      },
    );

  const resetAvailableProviderModels = () => {
    providerModelsRequestRef.current += 1;
    setAvailableProviderModels([]);
    setProviderModelsLoading(false);
  };

  const fetchProviderModels = async () => {
    const baseUrl = providerForm.baseUrl.trim();
    const apiKey = (providerForm.apiKey || "").trim();
    if (!baseUrl || (!apiKey && !providerForm.hasApiKey)) {
      setError("");
      setToast(lang === "zh" ? "请先填写 API 请求地址和 API Key" : "Enter the API URL and API key first");
      return;
    }

    const requestId = providerModelsRequestRef.current + 1;
    providerModelsRequestRef.current = requestId;
    setProviderModelsLoading(true);
    setError("");
    setToast(lang === "zh" ? "正在获取模型列表..." : "Fetching model list...");
    try {
      const result = await invoke<ProviderModelsResult>("fetch_provider_models", {
        baseUrl,
        apiKey: apiKey || null,
        tool: activeTool,
        providerId: editingProviderId,
      });
      if (providerModelsRequestRef.current !== requestId) return;
      setAvailableProviderModels(result.models);
      setToast(result.models.length > 0
        ? (lang === "zh" ? `已获取 ${result.models.length} 个模型` : `${result.models.length} models fetched`)
        : (lang === "zh" ? "连接成功，但供应商没有返回模型" : "Connected, but the provider returned no models"));
    } catch (e) {
      if (providerModelsRequestRef.current !== requestId) return;
      setToast("");
      setError(String(e));
    } finally {
      if (providerModelsRequestRef.current === requestId) setProviderModelsLoading(false);
    }
  };

  const testProvider = async (
    id: string,
    baseUrl: string,
    apiKey?: string | null,
    savedProviderId?: string | null,
  ) => {
    setProviderTestingId(id);
    setError("");
    setToast(lang === "zh" ? "正在检测连接..." : "Testing connection...");
    try {
      const result = await invoke<ProviderConnectionResult>("test_provider_connection", {
        baseUrl,
        apiKey: apiKey || null,
        tool: activeTool,
        providerId: savedProviderId || null,
      });
      if (result.ok) {
        setToast(lang === "zh" ? `连接成功，响应延迟 ${result.durationMs}ms` : `Connected, ${result.durationMs}ms latency`);
      } else {
        setToast("");
        setError(lang === "zh" ? `连接失败：${result.message}` : `Connection failed: ${result.message}`);
      }
    } catch (e) {
      setToast("");
      setError(String(e));
    } finally {
      setProviderTestingId("");
    }
  };

  const saveProviderConfig = saveProviderOnly;

  const switchOfficialProvider = () =>
    call(
      () => invoke<ActionResult>("switch_official_provider", { configDir: configDir || null }),
      (result) => {
        localStorage.removeItem(ACTIVE_PROVIDER_KEY);
        localStorage.removeItem(activeProviderKey("codex"));
        setActiveProviderId("");
        handleActionResult(result);
      },
    );

  const importFromCcSwitch = async () => {
    setActionBusy("importCcSwitch");
    try {
      if (activeTool === "zcode") {
        await call(
          async () => {
            const [providers, statuses] = await Promise.all([
              invoke<SavedProvider[]>("list_saved_providers", { appType: "zcode" }),
              invoke<ToolStatus[]>("get_tool_statuses", { configDir: configDir || null }),
            ]);
            return { providers, statuses };
          },
          ({ providers, statuses }) => {
            setSavedProviders(providers);
            setToolStatuses(statuses);
            setToast(lang === "zh" ? `已刷新 ${providers.length} 个 ZCode 原生供应商` : `Refreshed ${providers.length} native ZCode provider(s)`);
          },
        );
        return;
      }
      await call(
        () => invoke<ImportResult>("import_ccswitch_providers", { tool: activeTool, dbPath: null }),
        (result) => {
          setSavedProviders(result.providers);
          const warningText = result.skipped > 0 ? `，跳过 ${result.skipped}` : "";
          setToast(
            lang === "zh"
              ? `cc-switch 导入完成：新增 ${result.added}，更新 ${result.updated}，合并 ${result.merged}${warningText}`
              : `cc-switch import complete: ${result.added} added, ${result.updated} updated, ${result.merged} merged${warningText}`,
          );
        },
      );
    } finally {
      setActionBusy("");
    }
  };

  const openExternalUrl = React.useCallback((url?: string | null) => {
    if (!url) return;
    window.setTimeout(() => {
      void invoke("open_url", { url }).catch(() => {
        setToast(lang === "zh" ? "打开浏览器失败" : "Failed to open browser");
      });
    }, 0);
  }, [lang]);

  const checkForUpdates = React.useCallback(async ({ quiet = false }: { quiet?: boolean } = {}) => {
    setReleaseInfo({ status: "checking" });
    try {
      if (aboutInfo?.nativeUpdaterSupported !== false) {
        const updaterResult = await appUpdater.check({ force: !quiet, timeout: 15_000 });
        if (updaterResult === "available") {
          const snapshot = appUpdater.getSnapshot();
          const latestVersion = snapshot.latestVersion || "";
          const releaseTag = latestVersion.startsWith("v") ? latestVersion : `v${latestVersion}`;
          setReleaseInfo({
            status: "ok",
            latestVersion: releaseTag,
            htmlUrl: `https://github.com/${FALLBACK_GITHUB_REPO}/releases/tag/${releaseTag}`,
            hasUpdate: true,
            updateMethod: "native",
          });
          if (quiet) {
            setToast(lang === "zh" ? `发现新版本 ${releaseTag}，可在概览页查看` : `New version ${releaseTag} is available`);
          } else {
            setUpdatePromptOpen(true);
          }
          return;
        }

        if (updaterResult === "up-to-date") {
          setReleaseInfo({
            status: "ok",
            latestVersion: aboutInfo?.appVersion,
            htmlUrl: `https://github.com/${FALLBACK_GITHUB_REPO}/releases/latest`,
            hasUpdate: false,
          });
          if (!quiet) setToast(lang === "zh" ? "当前已是最新版本" : "You are up to date");
          return;
        }
      }

      // Keep the existing lightweight release check as a manual-download fallback for
      // bootstrap and portable builds that cannot use the native updater yet.
      const update = await invoke<AppUpdateInfo>("check_app_update");
      const message = update.hasUpdate
        ? (lang === "zh" ? "发现新版本" : "Update available")
        : (lang === "zh" ? "当前已是最新版本" : "You are up to date");
      setReleaseInfo({
        status: "ok",
        latestVersion: update.latestVersion,
        htmlUrl: update.htmlUrl,
        hasUpdate: update.hasUpdate,
        updateMethod: update.hasUpdate ? "download" : undefined,
      });
      if (update.hasUpdate) {
        if (quiet) {
          setToast(lang === "zh" ? `发现新版本 ${update.latestVersion}，可在概览页查看` : `New version ${update.latestVersion} is available`);
        } else {
          setUpdatePromptOpen(true);
        }
      } else if (!quiet) {
        setToast(message);
      }
    } catch {
      const message = quiet ? (lang === "zh" ? "自动检查失败" : "Auto check failed") : (lang === "zh" ? "检查失败" : "Check failed");
      setReleaseInfo({
        status: "error",
      });
      if (!quiet) setToast(message);
    }
  }, [aboutInfo?.appVersion, aboutInfo?.nativeUpdaterSupported, lang]);

  React.useEffect(() => {
    if (!state || autoUpdateCheckedRef.current) return;
    autoUpdateCheckedRef.current = true;
    void checkForUpdates({ quiet: true });
  }, [state, checkForUpdates]);

  React.useEffect(() => {
    if (!state || promptAutoRefreshAttemptedRef.current) return;
    promptAutoRefreshAttemptedRef.current = true;
    void refreshBuiltinPrompts({ quiet: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state]);

  const loadSkillsMcp = React.useCallback(async ({ quiet = false }: { quiet?: boolean } = {}) => {
    const requestId = skillsMcpRequestRef.current + 1;
    skillsMcpRequestRef.current = requestId;
    const tool = activeTool;
    if (!quiet) {
      setActionBusy("loadSkillsMcp");
      setError("");
    }
    try {
      const result = await invoke<SkillsMcpState>("get_tool_skills_mcp_state", {
        tool,
        configDir: tool === "codex" ? configDir || null : null,
      });
      if (requestId === skillsMcpRequestRef.current && tool === activeToolRef.current) {
        setSkillsMcpState(result);
      }
    } catch (e) {
      if (
        !quiet
        && requestId === skillsMcpRequestRef.current
        && tool === activeToolRef.current
      ) {
        setError(String(e));
      }
    } finally {
      if (!quiet && requestId === skillsMcpRequestRef.current) setActionBusy("");
    }
  }, [activeTool, configDir]);

  React.useEffect(() => {
    if (tab !== "skillsMcp" || skillsMcpLoadedRef.current) return;
    skillsMcpLoadedRef.current = true;
    void loadSkillsMcp();
  }, [tab, loadSkillsMcp]);

  const openImportExistingSkillsMcpPreview = async () => {
    setActionBusy("previewExistingSkillsMcp");
    setError("");
    try {
      const preview = await invoke<SkillsMcpImportPreview>("preview_tool_skills_mcp_import", {
        tool: activeTool,
        configDir: activeTool === "codex" ? configDir || null : null,
      });
      if (preview.skills.length + preview.mcpServers.length === 0) {
        setSkillsMcpImportPreview(null);
        setSkillsMcpImportOpen(false);
        setToast(lang === "zh" ? "没有需要新导入的 Skills 或 MCP" : "No new Skills or MCP items to import");
        return;
      }
      setSkillsMcpImportPreview(preview);
      setSkillsMcpImportOpen(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setActionBusy("");
    }
  };

  const importExistingSkillsMcp = async () => {
    setActionBusy("importExistingSkillsMcp");
    setError("");
    try {
      const result = await invoke<SkillsMcpActionResult>("import_tool_skills_mcp", {
        tool: activeTool,
        configDir: activeTool === "codex" ? configDir || null : null,
      });
      setSkillsMcpState(result.state);
      setSkillsMcpImportOpen(false);
      setSkillsMcpImportPreview(null);
      setToast(result.message);
    } catch (e) {
      setError(String(e));
    } finally {
      setActionBusy("");
    }
  };

  const checkSkillUpdatesAction = async () => {
    setActionBusy("checkSkillUpdates");
    setError("");
    try {
      const result = await invoke<SkillsMcpState>("check_tool_skill_updates", {
        tool: activeTool,
        configDir: activeTool === "codex" ? configDir || null : null,
      });
      setSkillsMcpState(result);
      setToast(lang === "zh" ? "Skills 更新状态已刷新" : "Skill update status refreshed");
    } catch (e) {
      setError(String(e));
    } finally {
      setActionBusy("");
    }
  };

  const toggleSkillEnabled = async (id: string, enabled: boolean) => {
    setActionBusy(`skill:${id}`);
    setError("");
    try {
      const result = await invoke<SkillsMcpState>("toggle_tool_skill", {
        tool: activeTool,
        configDir: activeTool === "codex" ? configDir || null : null,
        id,
        enabled,
      });
      setSkillsMcpState(result);
      setToast(enabled ? (lang === "zh" ? "Skill 已启用" : "Skill enabled") : (lang === "zh" ? "Skill 已禁用" : "Skill disabled"));
    } catch (e) {
      setError(String(e));
    } finally {
      setActionBusy("");
    }
  };

  const toggleMcpEnabled = async (id: string, enabled: boolean) => {
    setActionBusy(`mcp:${id}`);
    setError("");
    try {
      const result = await invoke<SkillsMcpState>("toggle_tool_mcp", {
        tool: activeTool,
        configDir: activeTool === "codex" ? configDir || null : null,
        id,
        enabled,
      });
      setSkillsMcpState(result);
      setToast(enabled ? (lang === "zh" ? "MCP 已启用" : "MCP enabled") : (lang === "zh" ? "MCP 已禁用" : "MCP disabled"));
    } catch (e) {
      setError(String(e));
    } finally {
      setActionBusy("");
    }
  };

  const installMcpIntegration = async (
    input: McpIntegrationInstallInput,
  ): Promise<McpIntegrationInstallResult> => {
    const tool = activeTool;
    setActionBusy(`installMcpIntegration:${input.integrationId}`);
    setError("");
    try {
      const result = await invoke<SkillsMcpActionResult>("install_mcp_integration", {
        tool,
        configDir: tool === "codex" ? configDir || null : null,
        input,
      });
      if (tool === activeToolRef.current) setSkillsMcpState(result.state);
      setToast(result.message);
      return { ok: true };
    } catch (e) {
      const message = String(e);
      setError(message);
      return { ok: false, error: message };
    } finally {
      setActionBusy("");
    }
  };

  const detectMcpHost = React.useCallback(async (
    integrationId: McpIntegrationInstallInput["integrationId"],
    mode: McpIntegrationInstallInput["mode"],
    hostPath?: string | null,
  ): Promise<McpHostInstallPlan> => invoke<McpHostInstallPlan>("detect_mcp_host", {
    integrationId,
    mode: mode || null,
    hostPath: hostPath || null,
  }), []);

  const restoreMcpHost = React.useCallback(async (
    integrationId: McpIntegrationInstallInput["integrationId"],
  ): Promise<string> => invoke<string>("restore_mcp_host_install", { integrationId }), []);

  const installSkillZipFile = async (file?: File | null) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".zip")) {
      setError(lang === "zh" ? "请选择 .zip 技能包" : "Please choose a .zip skill package");
      return;
    }
    if (file.size > 20 * 1024 * 1024) {
      setError(lang === "zh" ? "ZIP 技能包不能超过 20MB" : "Skill ZIP must be smaller than 20MB");
      return;
    }
    setActionBusy("installSkillZip");
    setError("");
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const result = await invoke<SkillsMcpActionResult>("install_tool_skill_zip", {
        tool: activeTool,
        configDir: activeTool === "codex" ? configDir || null : null,
        fileName: file.name,
        bytes,
      });
      setSkillsMcpState(result.state);
      setToast(result.message);
    } catch (e) {
      setError(String(e));
    } finally {
      setActionBusy("");
      if (skillZipImportRef.current) skillZipImportRef.current.value = "";
    }
  };

  const loadToolConfig = React.useCallback(async ({ quiet = false }: { quiet?: boolean } = {}) => {
    const requestId = toolConfigRequestRef.current + 1;
    toolConfigRequestRef.current = requestId;
    const tool = activeTool;
    if (!quiet) {
      setActionBusy("loadToolConfig");
      setError("");
    }
    try {
      const result = await invoke<ToolConfigBundle>("get_tool_config", {
        tool,
        configDir: tool === "codex" ? configDir || null : null,
      });
      if (requestId === toolConfigRequestRef.current && tool === activeToolRef.current) {
        setToolConfig(result);
        setConfigFileId((current) => result.files.some((file) => file.id === current)
          ? current
          : result.primaryFileId);
      }
    } catch (e) {
      if (
        !quiet
        && requestId === toolConfigRequestRef.current
        && tool === activeToolRef.current
      ) {
        setError(String(e));
      }
    } finally {
      if (!quiet && requestId === toolConfigRequestRef.current) setActionBusy("");
    }
  }, [activeTool, configDir]);

  React.useEffect(() => {
    if (tab !== "toml") return;
    void loadToolConfig();
  }, [loadToolConfig, tab]);

  const officialAuthPlaceholder = '{\n  "OPENAI_API_KEY": null,\n  "auth_mode": "chatgpt",\n  "tokens": {\n    "access_token": "",\n    "refresh_token": "",\n    "id_token": ""\n  }\n}';

  const openOfficialEdit = () => {
    setOfficialForm({
      model: state?.model || "gpt-5.5",
      authJson: state?.authText || officialAuthPlaceholder,
    });
    setProviderMode("official");
  };

  const saveOfficialConfig = () =>
    call(
      () =>
        invoke<ActionResult>("save_official_config", {
          input: {
            configDir: configDir || null,
            model: officialForm.model,
            authJson: officialForm.authJson,
          },
        }),
      (result) => {
        handleActionResult(result);
        setProviderMode("list");
      },
    );

  const openAddProvider = () => {
    const next = blankProviderForTool(activeTool);
    resetAvailableProviderModels();
    setEditingProviderId(null);
    setProviderForm(next);
    setProviderTomlDraft(buildProviderTomlPreview(next, state));
    setProviderTomlDirty(false);
    setProviderMode("form");
  };

  const openEditProvider = (provider: SavedProvider) => {
    resetAvailableProviderModels();
    setEditingProviderId(provider.id);
    const next = { ...provider, appType: activeTool };
    setProviderForm(next);
    setProviderTomlDraft(next.tomlConfig?.trim() || buildProviderTomlPreview(next, state));
    setProviderTomlDirty(false);
    setProviderMode("form");
  };

  const openEditDetectedProvider = (provider: { id: string; providerName: string; baseUrl: string; model: string; apiKey?: string; wireApi: string; requiresOpenaiAuth: boolean }) => {
    resetAvailableProviderModels();
    setEditingProviderId(null);
    const next = {
      appType: activeTool,
      id: customProviderId(provider.providerName || provider.baseUrl),
      providerName: provider.providerName,
      baseUrl: provider.baseUrl,
      model: provider.model,
      apiKey: provider.apiKey || extractOpenAiApiKey(state?.authText),
      tomlConfig: "",
      wireApi: provider.wireApi || "responses",
      requiresOpenaiAuth: provider.requiresOpenaiAuth,
    };
    setProviderForm(next);
    setProviderTomlDraft(buildProviderTomlPreview(next, state));
    setProviderTomlDirty(false);
    setProviderMode("form");
  };

  const removeProvider = async (id: string) => {
    setLoading(true);
    setError("");
    try {
      await invoke<void>("delete_saved_provider", { id, appType: activeTool });
      const providerList = await invoke<SavedProvider[]>("list_saved_providers", { appType: activeTool });
      setSavedProviders(providerList);
      setToast(lang === "zh" ? "供应商已删除" : "Provider deleted");
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setLoading(false);
    }
  };

  const checkSessions = async () => {
    const requestId = sessionsRequestRef.current + 1;
    sessionsRequestRef.current = requestId;
    const tool = activeTool;
    const resolvedConfigDir = configDir || null;
    setActionBusy("checkSessions");
    setLoading(true);
    setError("");
    try {
      if (tool !== "codex") {
        const result = await invoke<ToolSessionList>("get_tool_sessions", {
          tool,
          configDir: null,
        });
        if (requestId !== sessionsRequestRef.current || tool !== activeToolRef.current) return;
        setToolSessionList(result);
        setSelectedSessionIds([]);
        setToast(lang === "zh"
          ? `已读取 ${result.sessions.length} 条 ${toolLabel(tool)} 会话`
          : `Loaded ${result.sessions.length} ${toolLabel(tool)} session(s)`);
        return;
      }
      const status = await invoke<SessionSyncStatus>("get_session_sync_status", {
        configDir: resolvedConfigDir,
        targetProvider: null,
      });
      if (requestId === sessionsRequestRef.current && tool === activeToolRef.current) {
        setSessionStatus(status);
        const hasMismatches = Boolean(status.needsSync);
        const previewCount = new Set(status.sessions.filter((item) => item.needsSync).map((item) => item.id)).size;
        const syncCount = hasMismatches
          ? Math.max(1, previewCount, status.mismatchedRollouts, status.mismatchedThreads)
          : 0;
        setToast(hasMismatches
          ? (lang === "zh" ? `有 ${syncCount} 条会话需要同步` : `${syncCount} session(s) need syncing`)
          : (lang === "zh" ? "全部会话已同步" : "All sessions are synced"));
      }
    } catch (e) {
      if (requestId === sessionsRequestRef.current && tool === activeToolRef.current) {
        setError(String(e));
      }
    } finally {
      if (requestId === sessionsRequestRef.current) {
        setActionBusy("");
        setLoading(false);
      }
    }
  };

  const syncSessions = async () => {
    if (activeTool !== "codex") return;
    const pendingCount = sessionSyncCount;
    setActionBusy("syncSessions");
    await call(
      () => invoke<SessionSyncResult>("sync_sessions_provider", { configDir: configDir || null, targetProvider: null }),
      (result) => {
        setSessionStatus(result.status);
        setSelectedSessionIds([]);
        const syncedCount = pendingCount || Math.max(result.updatedRollouts, result.updatedThreads);
        setToast(lang === "zh"
          ? `已同步 ${syncedCount} 条会话，聊天内容未改动`
          : `Synced ${syncedCount} session(s). Chat content was not changed.`);
      },
    );
    setActionBusy("");
  };

  const toggleSessionSelected = (id: string) => {
    setSelectedSessionIds((ids) => ids.includes(id) ? ids.filter((item) => item !== id) : [...ids, id]);
  };

  const setSessionGroupSelected = (sessions: SessionPreview[], checked: boolean) => {
    const groupIds = new Set(sessions.map((item) => item.id));
    setSelectedSessionIds((ids) => {
      const next = new Set(ids);
      if (checked) groupIds.forEach((id) => next.add(id));
      else groupIds.forEach((id) => next.delete(id));
      return Array.from(next);
    });
  };

  const closeSessionDeleteConfirm = () => {
    if (!sessionDeleteBusy) {
      setSessionDeleteConfirmOpen(false);
      setSessionDeleteSafetyConfirmed(false);
    }
  };

  const deleteSelectedSessions = async () => {
    if (activeTool !== "codex" || !selectedSessionIds.length || sessionDeleteBusy || !sessionDeleteSafetyConfirmed) return;
    setSessionDeleteBusy(true);
    setToast("");
    setError("");
    try {
      const result = await invoke<SessionDeleteResult>("delete_codex_sessions", {
        input: {
          configDir: configDir || null,
          sessionIds: selectedSessionIds,
        },
      });
      setSessionStatus(result.status);
      const remainingIds = new Set(result.status.sessions.map((item) => item.id));
      setSelectedSessionIds((ids) => ids.filter((id) => remainingIds.has(id)));
      setSessionDeleteConfirmOpen(false);
      setSessionDeleteSafetyConfirmed(false);
      const hasPartialFailure = result.failedSessions > 0 || Boolean(result.failureMessage);
      if (hasPartialFailure) {
        setError(result.failureMessage || (lang === "zh"
          ? `${result.failedSessions} 个会话删除失败，请关闭其他 Codex 窗口或 CLI 后重试。`
          : `${result.failedSessions} session deletion(s) failed. Close other Codex windows or CLIs and retry.`));
      } else {
        setToast(lang === "zh"
          ? `已永久删除 ${result.deletedSessions} 条会话，并清理数据库、Rollout 与关联历史`
          : `Permanently deleted ${result.deletedSessions} session(s) and cleaned database, rollout, and related history data`);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setSessionDeleteBusy(false);
    }
  };

  const closeStartupWizard = () => {
    localStorage.setItem(STARTUP_WIZARD_SEEN_KEY, "1");
    setStartupClosing(true);
    window.setTimeout(() => {
      setStartupWizardOpen(false);
      setStartupClosing(false);
    }, 260);
  };

  return (
    <AppShell
      activeTab={tab}
      onTabChange={(nextTab) => {
        if (nextTab === "skins" && !skinCenterEnabled) return;
        setTab(nextTab);
      }}
      lang={lang}
      theme={theme}
      onToggleTheme={toggleTheme}
      codexVersion={aboutInfo?.codexVersion}
      appVersion={aboutInfo?.appVersion}
      toolStatuses={toolStatuses}
      hasUpdate={Boolean(releaseInfo.hasUpdate)}
      updatePhase={updater.state.phase}
      onOpenUpdate={() => setUpdatePromptOpen(true)}
      isMacRuntime={isMacRuntime}
      skinCenterEnabled={skinCenterEnabled}
      contentClassName={cx(
        tab === "sessions" && "cx-app-content--sessions",
        (
          (tab === "provider" && providerMode === "list")
          || tab === "skillsMcp"
        ) && "cx-app-content--fixed",
        skillsMcpImportOpen && Boolean(skillsMcpImportPreview) && "cx-app-content--modal-locked",
      )}
    >
      <AppToast
        lang={lang}
        message={toast}
        error={error}
        loading={Boolean(providerTestingId || providerModelsLoading) && Boolean(toast)}
        onDismissMessage={() => setToast("")}
        onDismissError={() => setError("")}
      />
      <UpdateDialog
        open={updatePromptOpen && Boolean(releaseInfo.hasUpdate)}
        lang={lang}
        state={releaseInfo.updateMethod === "native" ? updater.state : undefined}
        currentVersion={aboutInfo?.appVersion}
        latestVersion={releaseInfo.latestVersion}
        onClose={() => setUpdatePromptOpen(false)}
        onUpdate={releaseInfo.updateMethod === "native" ? updater.downloadAndInstall : undefined}
        onRetry={releaseInfo.updateMethod === "native" ? updater.retry : undefined}
        onRestart={releaseInfo.updateMethod === "native" ? updater.restart : undefined}
        onDownload={() => {
          setUpdatePromptOpen(false);
          openExternalUrl(releaseInfo.htmlUrl);
        }}
      />
      <SkinRestartDialog
        open={skinCenterEnabled && Boolean(skinRestartRequest)}
        lang={lang}
        themeName={skinRestartRequest?.themeName}
        busy={actionBusy.startsWith("skin:")}
        onClose={closeSkinRestart}
        onConfirm={confirmSkinRestart}
      />
      <StartupWizardDialog
        open={startupWizardOpen}
        closing={startupClosing}
        lang={lang}
        diagnostics={startupDiagnostics}
        configDir={configDir}
        loading={loading}
        onConfigDirChange={setConfigDir}
        onRecheck={refresh}
        onSkip={closeStartupWizard}
        onOpenSettings={() => {
          setTab("settings");
          closeStartupWizard();
        }}
        onEnter={closeStartupWizard}
      />

      <PageTransition pageKey={tab}>
            {tab === "dashboard" && (
              <OverviewPage
                lang={lang}
                tool={activeTool}
                toolStatuses={toolStatuses}
                onToolChange={setActiveTool}
                model={codexState?.model}
                configDir={activeTool === "codex" ? configDir : ""}
                resolvedCodexDir={codexState?.codexDir || ""}
                configExists={Boolean(activeToolStatus?.configExists ?? codexState?.configExists)}
                providerLabel={activeTool === "codex"
                  ? (currentProvider?.name || codexState?.modelProvider)
                  : null}
                instructionEnabled={Boolean(activeToolStatus?.instructionEnabled ?? codexState?.instructionEnabled)}
                authExists={Boolean(activeToolStatus?.authExists ?? codexState?.authExists)}
                configPath={activeToolStatus?.configPath || codexState?.configPath}
                modelProvider={codexState?.modelProvider}
                instructionPath={activeTool === "codex"
                  ? (state
                    ? (state.instructionInjectionMode === "append"
                      ? `${state.agentsPath} (${lang === "zh" ? "追加模式" : "append"})`
                      : state.instructionFile)
                    : null)
                  : (activeToolStatus?.instructionPath || null)}
                loading={loading}
                hasUpdate={Boolean(releaseInfo.status === "ok" && releaseInfo.hasUpdate)}
                latestVersion={releaseInfo.latestVersion}
                onConfigDirChange={setConfigDir}
                onRefresh={refresh}
                onOpenUpdate={() => setUpdatePromptOpen(true)}
              />
            )}

            {tab === "provider" && (
              <ProvidersPage
                lang={lang}
                tool={activeTool}
                toolStatuses={toolStatuses}
                onToolChange={setActiveTool}
                supportsProviders={activeToolStatus?.capabilities.providers ?? true}
                copy={getProviderPageCopy(lang, activeTool)}
                mode={providerMode}
                providerRows={providerPageRows}
                loading={loading}
                testingId={providerTestingId}
                actionBusy={actionBusy}
                editingProviderId={editingProviderId}
                providerForm={{
                  apiKey: providerForm.apiKey || "",
                  hasApiKey: providerForm.hasApiKey,
                  baseUrl: providerForm.baseUrl,
                  providerName: providerForm.providerName,
                  model: providerForm.model,
                  wireApi: providerForm.wireApi,
                  requiresOpenaiAuth: providerForm.requiresOpenaiAuth,
                }}
                officialForm={officialForm}
                officialInfo={{
                  officialUrl: "https://chatgpt.com/codex",
                  authPath: state?.authPath || activeToolStatus?.authPath || "—",
                  current: (!state?.modelProvider || state.modelProvider === "openai") ? "OpenAI Official" : state.modelProvider,
                }}
                providerAuthPreview={<JsonPreview text={providerAuthPreview} />}
                providerTomlDraft={providerTomlDraft}
                providerTomlRef={providerTomlEditorRef}
                apiKeyVisible={providerApiKeyVisible}
                availableModels={availableProviderModels.map((model) => model.id)}
                fetchingModels={providerModelsLoading}
                onImportCcSwitch={importFromCcSwitch}
                onAddProvider={openAddProvider}
                onEnableProvider={(row, selectedModel) => {
                  if (row.source === "official") {
                    switchOfficialProvider();
                    return;
                  }
                  const local = findLocalProviderForRow(row);
                   switchProvider(local || {
                    appType: activeTool,
                    id: customProviderId(row.providerName),
                    providerName: row.providerName,
                    baseUrl: row.baseUrl,
                    model: row.model,
                    apiKey: row.apiKey || "",
                    tomlConfig: "",
                    wireApi: row.wireApi,
                    requiresOpenaiAuth: row.requiresOpenaiAuth,
                  }, selectedModel);
                }}
                onTestProvider={(row) => {
                  const local = findLocalProviderForRow(row);
                  void testProvider(
                    row.testingKey || `${row.source}-${row.id}`,
                    row.baseUrl,
                    local?.apiKey || row.apiKey || null,
                    local?.id || null,
                  );
                }}
                onEditProvider={(row) => {
                  if (row.source === "official") {
                    openOfficialEdit();
                    return;
                  }
                  const local = findLocalProviderForRow(row);
                  if (local) openEditProvider(local);
                  else if (row.source === "detected") openEditDetectedProvider(row);
                }}
                onDeleteProvider={(row) => {
                  const local = findLocalProviderForRow(row);
                  return local ? removeProvider(local.id) : Promise.resolve(false);
                }}
                onCancelMode={() => setProviderMode("list")}
                onOfficialModelChange={(value) => setOfficialForm((current) => ({ ...current, model: value }))}
                onOfficialAuthChange={(value) => setOfficialForm((current) => ({ ...current, authJson: value }))}
                onSaveOfficial={saveOfficialConfig}
                onApiKeyChange={(value) => {
                  resetAvailableProviderModels();
                  setProviderForm((current) => ({ ...current, apiKey: value }));
                }}
                onBaseUrlChange={(value) => {
                  resetAvailableProviderModels();
                  setProviderForm((current) => ({ ...current, baseUrl: value }));
                }}
                onProviderNameChange={(value) => setProviderForm((current) => ({
                  ...current,
                  providerName: value,
                  id: editingProviderId || customProviderId(value),
                }))}
                onProviderModelChange={(value) => setProviderForm((current) => ({ ...current, model: value }))}
                onFetchModels={() => void fetchProviderModels()}
                onWireApiChange={(value) => setProviderForm((current) => ({ ...current, wireApi: value }))}
                onRequiresAuthChange={(value) => setProviderForm((current) => ({ ...current, requiresOpenaiAuth: value }))}
                onToggleApiKeyVisibility={() => setProviderApiKeyVisible((value) => !value)}
                onProviderTomlDraftChange={(value) => {
                  setProviderTomlDraft(value);
                  setProviderTomlDirty(true);
                }}
                onResetProviderToml={() => {
                  setProviderTomlDraft(providerTomlPreview);
                  setProviderTomlDirty(false);
                }}
                onSaveProvider={saveProviderConfig}
              />
            )}

            {(tab === "sessions" || visitedTabs.has("sessions")) && (
              <SessionManagementPage
                active={tab === "sessions"}
                lang={lang}
                activeTool={activeTool}
                toolStatuses={toolStatuses}
                onToolChange={setActiveTool}
                sessionStatus={displayedSessionStatus}
                sessionHasMismatches={sessionHasMismatches}
                sessionSyncCount={sessionSyncCount}
                sessionTargetLabel={sessionTargetLabel}
                sessionVisibleTotal={sessionVisibleTotal}
                sessionPreviewTruncated={sessionPreviewTruncated}
                visibleSessions={visibleSessions}
                filteredSessions={filteredSessions}
                allSessionsByCwd={allSessionsByCwd}
                groupedSessions={groupedSessions}
                selectedSessionIds={selectedSessionIds}
                selectedSessionSet={selectedSessionSet}
                selectedSessions={selectedSessions}
                sessionQuery={sessionQuery}
                sessionGroupByCwd={sessionGroupByCwd}
                showInternalSessions={showInternalSessions}
                loading={loading}
                actionBusy={actionBusy}
                sessionDeleteConfirmOpen={sessionDeleteConfirmOpen}
                sessionDeleteBusy={sessionDeleteBusy}
                sessionDeleteSafetyConfirmed={sessionDeleteSafetyConfirmed}
                onCheckSessions={checkSessions}
                onSyncSessions={syncSessions}
                onSessionQueryChange={(value) => {
                  setSessionQuery(value);
                  setSelectedSessionIds([]);
                  setSessionDeleteConfirmOpen(false);
                }}
                onSessionGroupByCwdChange={setSessionGroupByCwd}
                onShowInternalSessionsChange={(checked) => {
                  setShowInternalSessions(checked);
                  setSelectedSessionIds([]);
                  setSessionDeleteConfirmOpen(false);
                }}
                onOpenDeleteConfirm={() => {
                  setSessionDeleteSafetyConfirmed(false);
                  setSessionDeleteConfirmOpen(true);
                }}
                onToggleSessionSelected={toggleSessionSelected}
                onSetSessionGroupSelected={setSessionGroupSelected}
                onCloseDeleteConfirm={closeSessionDeleteConfirm}
                onDeleteSelectedSessions={deleteSelectedSessions}
                onDeleteSafetyConfirmedChange={setSessionDeleteSafetyConfirmed}
              />
            )}

            {(tab === "skillsMcp" || visitedTabs.has("skillsMcp")) && (
              <SkillsMcpPage
                lang={lang}
                activeTool={activeTool}
                toolStatuses={toolStatuses}
                onToolChange={setActiveTool}
                state={skillsMcpState}
                activeTab={skillsMcpTab}
                actionBusy={actionBusy}
                importOpen={skillsMcpImportOpen}
                importPreview={skillsMcpImportPreview}
                zipInputRef={skillZipImportRef}
                className={tab !== "skillsMcp" ? "page-pane-hidden" : undefined}
                onTabChange={setSkillsMcpTab}
                onLoad={loadSkillsMcp}
                onOpenImportPreview={openImportExistingSkillsMcpPreview}
                onCloseImportPreview={() => setSkillsMcpImportOpen(false)}
                onConfirmImport={importExistingSkillsMcp}
                onInstallZip={installSkillZipFile}
                onCheckUpdates={checkSkillUpdatesAction}
                onToggleSkill={toggleSkillEnabled}
                onToggleMcp={toggleMcpEnabled}
                onInstallMcpIntegration={installMcpIntegration}
                onDetectMcpHost={detectMcpHost}
                onRestoreMcpHost={restoreMcpHost}
                onOpenExternalUrl={openExternalUrl}
              />
            )}

            {skinCenterEnabled && state && (tab === "skins" || visitedTabs.has("skins")) && (
              <div className={cx("cx-skins-pane", tab !== "skins" && "page-pane-hidden")}>
                <SkinsPage
                  lang={lang}
                  state={skinCenterState}
                  actionBusy={actionBusy}
                  pauseBusy={skinPauseBusy}
                  zipInputRef={skinZipImportRef}
                  imageInputRef={skinImageInputRef}
                  onLoad={refreshSkinCenter}
                  onImportZip={importSkinThemeZip}
                  onCreateFromImage={createSkinThemeFromImage}
                  onUpdateThemeSettings={updateSkinThemeSettings}
                  onEnableTheme={enableSkinTheme}
                  onExportTheme={exportSkinTheme}
                  onPauseTheme={pauseSkinTheme}
                />
              </div>
            )}

            {tab === "instruction" && (
              <PromptsPage
                lang={lang}
                instructionMode={instructionMode}
                promptForm={promptForm}
                editingPromptId={editingPromptId}
                loading={loading}
                actionBusy={actionBusy}
                promptSyncing={promptSyncing}
                promptCatalogReady={promptCatalogReady}
                promptImportRef={promptImportRef}
                promptInjectionMode={promptInjectionMode}
                promptModeHelpOpen={promptModeHelpOpen}
                promptModeHelpRef={promptModeHelpRef}
                promptEngine={promptEngine}
                onPromptEngineChange={setPromptEngine}
                instructionEnabled={Boolean(state?.instructionEnabled)}
                codexInstructionStatus={state?.instructionStatus}
                codexInactiveInstructionFile={state?.inactiveInstructionFile}
                activeInstructionTitle={activeInstructionTitle}
                activeInjectionMode={activePromptInjectionMode}
                codexRuntimeScope={state?.codexDir}
                codexRuntimeEntryPath={state?.agentsPath}
                claudeRuntimeEntryPath={claudeState?.memoryPath}
                claudeRuntime={claudeState?.runtime}
                zcodeRuntime={zcodeState}
                grokRuntime={grokState}
                kiloRuntime={kiloState}
                piRuntime={piState}
                promptBackups={promptBackups}
                promptBackupsOpen={promptBackupsOpen}
                promptBackupsLoading={promptBackupsLoading}
                promptRestoreBusyId={promptRestoreBusyId}
                instructionTemplates={instructionTemplates}
                builtinPromptStatuses={builtinPromptStatus}
                activeBuiltinTemplateId={activeBuiltinTemplateId}
                orphanedBuiltinPrompt={missingActiveBuiltinTemplateId ? {
                  id: missingActiveBuiltinTemplateId,
                  title: activeInstructionTitle,
                  description: lang === "zh"
                    ? "该模板已从在线目录移除，当前配置仍在使用。"
                    : "This template was removed online but is still active.",
                } : null}
                savedPrompts={savedPrompts}
                managedSavedPromptId={state?.instructionTemplateKey?.startsWith("saved:")
                  ? state.instructionTemplateKey.slice("saved:".length)
                  : null}
                preservedSavedPromptFilename={state?.instructionInjectionMode === "append" ? currentInstructionFilename : null}
                externalPrompt={state?.instructionFile
                  && currentInstructionId === "custom"
                  && !savedPrompts.some((prompt) => currentInstructionFilename === prompt.filename)
                  && !(missingActiveBuiltinTemplateId && state?.instructionInjectionMode !== "append")
                  ? {
                    title: lang === "zh" ? "用户原有指令提示词" : "Existing user prompt",
                    description: state?.instructionInjectionMode === "append"
                      ? (lang === "zh"
                        ? "追加模式已保留这份外部提示词，并同时加载 DevConduit 的 AGENTS.md 区块。"
                        : "Append mode preserves this external prompt alongside the DevConduit AGENTS.md block.")
                      : (lang === "zh"
                        ? "当前使用的是非 DevConduit 管理的外部提示词。"
                        : "This external prompt is not managed by DevConduit."),
                    filename: currentInstructionFilename,
                  }
                  : null}
                onSyncBuiltinPrompts={() => refreshBuiltinPrompts()}
                onImportPrompt={importPromptMd}
                onAddPrompt={openAddPrompt}
                onInstructionModeChange={setInstructionMode}
                onPromptInjectionModeChange={setPromptInjectionMode}
                onTogglePromptModeHelp={() => setPromptModeHelpOpen((open) => !open)}
                onOpenPromptBackups={openPromptBackups}
                onClosePromptBackups={closePromptBackups}
                onRestorePromptBackup={restorePromptBackup}
                onEnableBuiltinPrompt={switchInstructionTemplate}
                onDisableInstruction={disableInstruction}
                onEnableSavedPrompt={enableSavedPrompt}
                onDisableExternalPrompt={disableExternalInstruction}
                onEditPrompt={openEditPrompt}
                onDeletePrompt={removeSavedPrompt}
                onPromptFormFieldChange={(field, value) => setPromptForm((current) => ({
                  ...current,
                  [field]: value,
                  ...(field === "title" ? { id: editingPromptId || providerId(value) } : {}),
                }))}
                onSavePrompt={savePromptOnly}
                claudeInstructionEnabled={Boolean(claudeState?.instructionEnabled)}
                claudeActiveInstructionTitle={claudeActiveInstructionTitle}
                claudeInstructionTemplates={claudeInstructionTemplates}
                claudeBuiltinPromptStatuses={claudeBuiltinStatus}
                claudeActiveBuiltinTemplateId={claudeActiveBuiltinTemplateId}
                claudeSavedPrompts={claudeSavedPrompts}
                claudeManagedSavedPromptId={claudeManagedSavedPromptId}
                onEnableClaudeBuiltinPrompt={switchClaudeTemplate}
                onDisableClaudeInstruction={disableClaudeInstruction}
                onEnableClaudeSavedPrompt={enableClaudeSavedPrompt}
                onEditClaudePrompt={openEditPrompt}
                onDeleteClaudePrompt={removeClaudeSavedPrompt}
                onImportClaudePrompt={importClaudePromptMd}
                onSaveClaudePrompt={saveClaudePromptOnly}
                onInstallClaudeRuntime={installClaudeRuntime}
                onUninstallClaudeRuntime={uninstallClaudeRuntime}
                zcodeInstructionEnabled={Boolean(zcodeState?.instructionEnabled)}
                zcodeActiveInstructionTitle={zcodeActiveInstructionTitle}
                zcodeInstructionTemplates={zcodeInstructionTemplates}
                zcodeBuiltinPromptStatuses={zcodeBuiltinStatus}
                zcodeActiveBuiltinTemplateId={zcodeActiveBuiltinTemplateId}
                zcodeSavedPrompts={zcodeSavedPrompts}
                zcodeManagedSavedPromptId={zcodeManagedSavedPromptId}
                onEnableZcodeBuiltinPrompt={installZcodeInstruction}
                onDisableZcodeInstruction={uninstallZcodeInstruction}
                onEnableZcodeSavedPrompt={installZcodeSavedPrompt}
                onEditZcodePrompt={openEditPrompt}
                onDeleteZcodePrompt={removeZcodeSavedPrompt}
                onImportZcodePrompt={importZcodePromptMd}
                onSaveZcodePrompt={saveZcodePromptOnly}
                grokInstructionEnabled={Boolean(grokState?.instructionEnabled)}
                grokActiveInstructionTitle={grokActiveInstructionTitle}
                grokInstructionTemplates={grokInstructionTemplates}
                grokBuiltinPromptStatuses={grokBuiltinStatus}
                grokActiveBuiltinTemplateId={grokActiveBuiltinTemplateId}
                grokSavedPrompts={grokSavedPrompts}
                grokManagedSavedPromptId={grokManagedSavedPromptId}
                onEnableGrokBuiltinPrompt={installGrokInstruction}
                onDisableGrokInstruction={uninstallGrokInstruction}
                onEnableGrokSavedPrompt={installGrokSavedPrompt}
                onEditGrokPrompt={openEditPrompt}
                onDeleteGrokPrompt={removeGrokSavedPrompt}
                onImportGrokPrompt={importGrokPromptMd}
                onSaveGrokPrompt={saveGrokPromptOnly}
                kiloInstructionEnabled={Boolean(kiloState?.instructionEnabled)}
                kiloActiveInstructionTitle={kiloActiveInstructionTitle}
                kiloInstructionTemplates={kiloInstructionTemplates}
                kiloBuiltinPromptStatuses={kiloBuiltinStatus}
                kiloActiveBuiltinTemplateId={kiloActiveBuiltinTemplateId}
                kiloSavedPrompts={kiloSavedPrompts}
                kiloManagedSavedPromptId={kiloManagedSavedPromptId}
                onEnableKiloBuiltinPrompt={installKiloInstruction}
                onDisableKiloInstruction={uninstallKiloInstruction}
                onEnableKiloSavedPrompt={installKiloSavedPrompt}
                onEditKiloPrompt={openEditPrompt}
                onDeleteKiloPrompt={removeKiloSavedPrompt}
                onImportKiloPrompt={importKiloPromptMd}
                onSaveKiloPrompt={saveKiloPromptOnly}
                piInstructionEnabled={Boolean(piState?.instructionEnabled)}
                piActiveInstructionTitle={piActiveInstructionTitle}
                piInstructionTemplates={piInstructionTemplates}
                piBuiltinPromptStatuses={piBuiltinStatus}
                piActiveBuiltinTemplateId={piActiveBuiltinTemplateId}
                piSavedPrompts={piSavedPrompts}
                piManagedSavedPromptId={piManagedSavedPromptId}
                onEnablePiBuiltinPrompt={installPiInstruction}
                onDisablePiInstruction={uninstallPiInstruction}
                onEnablePiSavedPrompt={installPiSavedPrompt}
                onEditPiPrompt={openEditPrompt}
                onDeletePiPrompt={removePiSavedPrompt}
                onImportPiPrompt={importPiPromptMd}
                onSavePiPrompt={savePiPromptOnly}
              />
            )}

            {tab === "toml" && (
              <ToolConfigPage
                lang={lang}
                tool={activeTool}
                toolStatuses={toolStatuses}
                config={toolConfig}
                selectedFileId={configFileId}
                loading={actionBusy === "loadToolConfig"}
                preview={selectedToolConfigFile?.format === "json" || selectedToolConfigFile?.format === "jsonc"
                  ? <JsonPreview text={selectedToolConfigFile.text || "{\n}"} />
                  : selectedToolConfigFile && selectedToolConfigFile.format !== "toml"
                    ? <PlainPreview text={selectedToolConfigFile.text || (lang === "zh" ? "# 未找到配置文件。" : "# Configuration file not found.")} />
                    : <TomlPreview text={selectedToolConfigFile?.text || "# Configuration file not found."} />}
                onToolChange={setActiveTool}
                onFileChange={setConfigFileId}
                onRefresh={() => void loadToolConfig()}
              />
            )}

            {tab === "about" && (
              <AboutPage
                copy={{
                  eyebrow: "About",
                   title: lang === "zh" ? "关于 DevConduit" : "About DevConduit",
                   appVersionLabel: `DevConduit ${lang === "zh" ? "版本" : "Version"}`,
                   projectLabel: lang === "zh" ? "项目地址" : "Project",
                  environmentsTitle: lang === "zh" ? "工具环境" : "Tool environments",
                  installedLabel: lang === "zh" ? "已检测到" : "Detected",
                  missingLabel: lang === "zh" ? "未检测到" : "Not detected",
                  versionLabel: lang === "zh" ? "版本" : "Version",
                  homeLabel: lang === "zh" ? "目录" : "Home",
                  configLabel: lang === "zh" ? "配置" : "Config",
                  openProjectLabel: lang === "zh" ? "打开项目主页" : "Open project",
                  openIssuesLabel: lang === "zh" ? "反馈问题" : "Issues",
                  releasesEyebrow: "GitHub Releases",
                  releasesTitle: lang === "zh" ? "更新检查" : "Update check",
                  releaseStatusLabel: lang === "zh" ? "状态" : "Status",
                  latestVersionLabel: lang === "zh" ? "最新版本" : "Latest version",
                  checkUpdateLabel: lang === "zh" ? "检查更新" : "Check updates",
                  openReleasesLabel: lang === "zh" ? "打开下载页" : "Open releases",
                 }}
                 appVersion={aboutInfo?.appVersion || "-"}
                toolStatuses={toolStatuses}
                projectUrl={aboutInfo?.projectUrl || `https://github.com/${FALLBACK_GITHUB_REPO}`}
                release={{
                  status: releaseStatusLabel,
                  latestVersion: releaseInfo.latestVersion || "-",
                  tone: releaseInfo.status === "error"
                    ? "error"
                    : releaseInfo.hasUpdate
                      ? "warning"
                      : releaseInfo.status === "ok"
                        ? "success"
                        : "neutral",
                  checking: releaseInfo.status === "checking"
                    || updater.state.phase === "downloading"
                    || updater.state.phase === "installing",
                  canOpenReleases: Boolean(releaseInfo.htmlUrl),
                }}
                onOpenProject={() => openExternalUrl(aboutInfo?.projectUrl || `https://github.com/${FALLBACK_GITHUB_REPO}`)}
                onOpenIssues={() => openExternalUrl(`${aboutInfo?.projectUrl || `https://github.com/${FALLBACK_GITHUB_REPO}`}/issues`)}
                onCheckUpdate={() => void checkForUpdates()}
                onOpenReleases={() => openExternalUrl(releaseInfo.htmlUrl)}
              />
            )}

            {tab === "settings" && (
              <SettingsPage
                lang={lang}
                copy={{
                  eyebrow: "Settings",
                  title: t.settings.title,
                  languageTitle: t.settings.language,
                  languageDescription: t.settings.languageDesc,
                  chineseLabel: t.settings.zh,
                  englishLabel: t.settings.en,
                  productTitle: t.settings.productName,
                  productDescription: t.settings.productDesc,
                  productValue: "DevConduit",
                  recheckTitle: lang === "zh" ? "首次启动向导" : "First-run wizard",
                  recheckDescription: lang === "zh"
                    ? "重新检测 CODEX_HOME、config.toml、auth.json 和 SQLite 会话库。"
                    : "Recheck CODEX_HOME, config.toml, auth.json and SQLite session stores.",
                  recheckLabel: lang === "zh" ? "重新检测" : "Recheck",
                }}
                onLanguageChange={setLang}
                recheckBusy={loading}
                onRecheck={() => {
                  localStorage.removeItem(STARTUP_WIZARD_SEEN_KEY);
                  setStartupWizardOpen(true);
                  refresh();
                }}
              />
            )}
      </PageTransition>
    </AppShell>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><App /></React.StrictMode>,
);
