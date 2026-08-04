import { useEffect, useMemo, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  ArrowLeft,
  Binary,
  Bug,
  Download,
  ExternalLink,
  FolderOpen,
  Network,
  Radar,
  RotateCcw,
  Search,
  ShieldCheck,
  Wrench,
} from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import type {
  Lang,
  McpHostInstallPlan,
  ManagedMcpServer,
  McpIntegrationId,
  McpIntegrationInstallInput,
  McpIntegrationInstallResult,
  ToolId,
} from "../types";
import { toolLabel } from "./ToolTabs";
import { Button, Checkbox, IconButton, ModalShell, StatusBadge, cx } from "./ui";

type RuntimePlatform = "windows" | "macos" | "linux";
type InstallMode = "local" | "remote" | "direct" | "proxy";
type SourceKind = "none" | "directory" | "python" | "jar";
type SourceMode = "managed" | "manual";

type LocalizedText = {
  zh: string;
  en: string;
};

type McpIntegrationDefinition = {
  id: McpIntegrationId;
  name: string;
  vendor: string;
  projectUrl: string;
  icon: LucideIcon;
  description: LocalizedText;
  requirements: readonly LocalizedText[];
  defaultCommand: {
    windows: string;
    other: string;
  };
  defaultEndpoint?: string;
  sourceKind: SourceKind;
  managedVersion: string;
  managedLicense: string;
  managedSummary: LocalizedText;
};

type InstallForm = {
  mode: InstallMode;
  sourceMode: SourceMode;
  sourcePath: string;
  hostPath: string;
  command: string;
  endpoint: string;
  confirmed: boolean;
};

export type McpInstallCatalogProps = {
  lang: Lang;
  tool: ToolId;
  servers: readonly ManagedMcpServer[];
  open: boolean;
  busy: boolean;
  onClose: () => void;
  onInstall: (input: McpIntegrationInstallInput) => Promise<McpIntegrationInstallResult>;
  onDetectHost: (
    integrationId: McpIntegrationInstallInput["integrationId"],
    mode: McpIntegrationInstallInput["mode"],
    hostPath?: string | null,
  ) => Promise<McpHostInstallPlan>;
  onRestoreHost: (integrationId: McpIntegrationInstallInput["integrationId"]) => Promise<string>;
  onOpenExternalUrl: (url: string) => void;
};

const integrations: readonly McpIntegrationDefinition[] = [
  {
    id: "ida-pro-mcp",
    name: "IDA Pro MCP",
    vendor: "mrexodia",
    projectUrl: "https://github.com/mrexodia/ida-pro-mcp",
    icon: Binary,
    description: {
      zh: "IDA Pro 的 idalib MCP 服务，适用于 Windows 和 macOS。",
      en: "The idalib MCP server for IDA Pro on Windows and macOS.",
    },
    requirements: [
      { zh: "IDA Pro 8.3+", en: "IDA Pro 8.3+" },
      { zh: "Python 3.11+", en: "Python 3.11+" },
      { zh: "uv", en: "uv" },
      { zh: "需先完成 idalib 激活", en: "idalib must already be activated" },
      { zh: "自动模式执行 uv sync --locked", en: "Automatic mode runs uv sync --locked" },
      { zh: "IDA Free 不支持", en: "IDA Free unsupported" },
    ],
    defaultCommand: { windows: "uv", other: "uv" },
    sourceKind: "directory",
    managedVersion: "f82e6e2 · 2026-07-30",
    managedLicense: "MIT",
    managedSummary: {
      zh: "下载固定提交源码，并使用 uv.lock 创建项目环境。",
      en: "Downloads the pinned source revision and creates its environment from uv.lock.",
    },
  },
  {
    id: "cheatengine-mcp",
    name: "Cheat Engine MCP",
    vendor: "miscusi-peek",
    projectUrl: "https://github.com/miscusi-peek/cheatengine-mcp-bridge",
    icon: Radar,
    description: {
      zh: "Windows 使用 Named Pipe，macOS 可通过 TCP 连接远程 Windows 主机。",
      en: "Uses Named Pipes on Windows or a TCP relay to a remote Windows host from macOS.",
    },
    requirements: [
      { zh: "Cheat Engine 运行于 Windows", en: "Cheat Engine on Windows" },
      { zh: "Python 3.10+", en: "Python 3.10+" },
      { zh: "Windows 自动安装 Lua 桥并保留备份", en: "Automatically installs and backs up the Lua bridge on Windows" },
      { zh: "自动模式创建独立 Python 环境", en: "Automatic mode creates an isolated Python environment" },
    ],
    defaultCommand: { windows: "python", other: "python3" },
    defaultEndpoint: "127.0.0.1:9876",
    sourceKind: "directory",
    managedVersion: "588813f · 2026-08-03",
    managedLicense: "MIT",
    managedSummary: {
      zh: "下载桥接源码与 Lua 脚本，并创建独立 Python 环境。",
      en: "Downloads the bridge and Lua script and creates an isolated Python environment.",
    },
  },
  {
    id: "x64dbg-mcp",
    name: "x64dbg MCP",
    vendor: "Wasdubya",
    projectUrl: "https://github.com/Wasdubya/x64dbgMCP",
    icon: Bug,
    description: {
      zh: "x64dbg/x32dbg 的 Python MCP 桥接，macOS 可连接远程 Windows 调试器。",
      en: "Python MCP bridge for x64dbg/x32dbg, with remote Windows debugger support from macOS.",
    },
    requirements: [
      { zh: "Python 3.10+", en: "Python 3.10+" },
      { zh: "Windows 自动安装 32/64 位插件并保留备份", en: "Automatically installs and backs up the 32/64-bit plugins on Windows" },
      { zh: "自动模式创建独立 Python 环境", en: "Automatic mode creates an isolated Python environment" },
      { zh: "HTTP 仅绑定可信网络", en: "Bind HTTP only to trusted networks" },
    ],
    defaultCommand: { windows: "python", other: "python3" },
    defaultEndpoint: "http://127.0.0.1:8888/",
    sourceKind: "python",
    managedVersion: "build1.1",
    managedLicense: "GPL-3.0",
    managedSummary: {
      zh: "下载 Python 桥及 32/64 位插件，并创建独立 Python 环境。",
      en: "Downloads the Python bridge and 32/64-bit plugins and creates an isolated Python environment.",
    },
  },
  {
    id: "burp-suite-mcp",
    name: "Burp Suite MCP",
    vendor: "PortSwigger",
    projectUrl: "https://github.com/PortSwigger/mcp-server",
    icon: ShieldCheck,
    description: {
      zh: "PortSwigger 官方 MCP Server，支持 Burp Suite Professional 和 Community。",
      en: "PortSwigger's official MCP Server for Burp Suite Professional and Community.",
    },
    requirements: [
      { zh: "Burp Pro / Community", en: "Burp Pro / Community" },
      { zh: "需在 Burp 中加载下载的官方扩展", en: "Load the downloaded official extension in Burp" },
      { zh: "默认 127.0.0.1:9876", en: "Default 127.0.0.1:9876" },
      { zh: "stdio 代理需 Java", en: "Java for the stdio proxy" },
    ],
    defaultCommand: { windows: "java", other: "java" },
    defaultEndpoint: "http://127.0.0.1:9876/sse",
    sourceKind: "none",
    managedVersion: "v1.3.0",
    managedLicense: "GPL-3.0",
    managedSummary: {
      zh: "下载 PortSwigger 官方扩展；需要代理时同时获取官方 stdio proxy。",
      en: "Downloads PortSwigger's official extension and its stdio proxy when required.",
    },
  },
];

function currentPlatform(): RuntimePlatform {
  const value = `${navigator.userAgent} ${navigator.platform}`.toLowerCase();
  if (value.includes("win")) return "windows";
  if (value.includes("mac")) return "macos";
  return "linux";
}

function localized(text: LocalizedText, lang: Lang) {
  return text[lang];
}

function supportsBurpDirectSse(tool: ToolId) {
  return tool === "claude" || tool === "zcode" || tool === "kilo";
}

function defaultMode(
  integration: McpIntegrationDefinition,
  platform: RuntimePlatform,
  tool: ToolId,
): InstallMode {
  if (integration.id === "burp-suite-mcp") return supportsBurpDirectSse(tool) ? "direct" : "proxy";
  if (integration.id === "cheatengine-mcp" || integration.id === "x64dbg-mcp") {
    return platform === "windows" ? "local" : "remote";
  }
  return "local";
}

function createForm(
  integration: McpIntegrationDefinition,
  platform: RuntimePlatform,
  tool: ToolId,
): InstallForm {
  return {
    mode: defaultMode(integration, platform, tool),
    sourceMode: "managed",
    sourcePath: "",
    hostPath: "",
    command: platform === "windows" ? integration.defaultCommand.windows : integration.defaultCommand.other,
    endpoint: integration.defaultEndpoint || "",
    confirmed: false,
  };
}

function sourceKindFor(integration: McpIntegrationDefinition, mode: InstallMode): SourceKind {
  if (integration.id === "burp-suite-mcp") return mode === "proxy" ? "jar" : "none";
  return integration.sourceKind;
}

function getCopy(lang: Lang, toolName: string) {
  return lang === "zh"
    ? {
        title: "接入工具 MCP",
        description: `默认从经过固定版本与 SHA-256 校验的上游来源获取文件，并写入 ${toolName} 配置。`,
        close: "关闭",
        configure: "安装",
        reconfigure: "重新配置",
        configured: "已配置",
        added: "已添加",
        notConfigured: "未配置",
        native: "本地",
        remote: "远程桥接",
        project: "项目主页",
        back: "返回集成目录",
        install: `写入 ${toolName}`,
        fetchInstall: "自动安装并配置",
        installing: "正在自动安装",
        requirements: "前置条件",
        sourceMode: "文件来源",
        managedSource: "自动获取（推荐）",
        manualSource: "手动选择",
        mode: "连接模式",
        localMode: "Windows 本地",
        remoteMode: "远程桥接",
        directMode: "SSE 直连",
        proxyMode: "stdio 代理",
        sourceDirectory: "已解压的项目目录",
        pythonFile: "Python MCP 桥接脚本",
        proxyJar: "mcp-proxy-all.jar",
        chooseDirectory: "选择项目目录",
        choosePython: "选择 x64dbg.py",
        chooseJar: "选择 mcp-proxy-all.jar",
        runtimeCommand: "运行时命令",
        endpoint: "服务地址",
        tcpEndpoint: "TCP 桥接地址",
        hostSoftware: "宿主软件",
        hostPath: "安装目录",
        hostPathPlaceholder: "留空自动检测",
        chooseHost: "选择安装目录",
        detectingHost: "正在检测宿主软件",
        hostReady: "可自动安装",
        hostDetected: "已检测",
        hostMissing: "未找到",
        hostRemote: "远程模式",
        hostManual: "需要确认目录",
        restoreHost: "恢复上次安装",
        restoringHost: "正在恢复",
        hostDetectFailed: "宿主软件检测失败",
        hostRestoreFailed: "恢复宿主文件失败",
        managedNotice: "DevConduit 将下载固定版本、核对 SHA-256，并在需要时创建独立 Python 环境。Windows 上会自动检测 CE/x64dbg、备份原文件并安装桥接插件；IDA 激活和 Burp 首次启用仍由对应软件控制。",
        manualNotice: "DevConduit 只校验你选择的本地文件并修改配置，不会下载或运行第三方文件。",
        confirmManaged: "我已确认来源、固定版本、许可证和安装预览，同意下载依赖、备份宿主文件并写入当前工具配置。",
        confirmManual: "我已检查所选本地文件，同意将该 MCP 写入当前工具配置。",
        pickerFailed: "无法打开文件选择器",
        installFailed: "写入 MCP 配置失败",
        windowsMac: "Windows / macOS 本地",
        windowsRemoteMac: "Windows 本地 / macOS 远程",
        burpProxyRequired: `${toolName} 使用官方 stdio 代理`,
      }
    : {
        title: "Add tool MCP",
        description: `By default, fetches upstream files pinned by version and SHA-256, then writes the ${toolName} configuration.`,
        close: "Close",
        configure: "Install",
        reconfigure: "Reconfigure",
        configured: "Configured",
        added: "Added",
        notConfigured: "Not configured",
        native: "Local",
        remote: "Remote bridge",
        project: "Project page",
        back: "Back to integrations",
        install: `Write to ${toolName}`,
        fetchInstall: "Install and configure",
        installing: "Installing automatically",
        requirements: "Requirements",
        sourceMode: "File source",
        managedSource: "Automatic (recommended)",
        manualSource: "Choose manually",
        mode: "Connection mode",
        localMode: "Windows local",
        remoteMode: "Remote bridge",
        directMode: "Direct SSE",
        proxyMode: "stdio proxy",
        sourceDirectory: "Extracted project directory",
        pythonFile: "Python MCP bridge script",
        proxyJar: "mcp-proxy-all.jar",
        chooseDirectory: "Choose project directory",
        choosePython: "Choose x64dbg.py",
        chooseJar: "Choose mcp-proxy-all.jar",
        runtimeCommand: "Runtime command",
        endpoint: "Service endpoint",
        tcpEndpoint: "TCP bridge endpoint",
        hostSoftware: "Host application",
        hostPath: "Installation directory",
        hostPathPlaceholder: "Leave empty to detect automatically",
        chooseHost: "Choose installation directory",
        detectingHost: "Detecting host application",
        hostReady: "Ready for automatic install",
        hostDetected: "Detected",
        hostMissing: "Not found",
        hostRemote: "Remote mode",
        hostManual: "Directory needs attention",
        restoreHost: "Restore previous install",
        restoringHost: "Restoring",
        hostDetectFailed: "Host application detection failed",
        hostRestoreFailed: "Could not restore host files",
        managedNotice: "DevConduit downloads the pinned version, verifies SHA-256, and creates an isolated Python environment when needed. On Windows it detects CE/x64dbg, backs up existing files, and installs the bridge plugins. IDA activation and Burp's first enable remain controlled by those applications.",
        manualNotice: "DevConduit only validates the local files you choose and updates configuration. It does not download or run third-party files.",
        confirmManaged: "I reviewed the source, pinned version, license, and installation preview and agree to download dependencies, back up host files, and update the active tool configuration.",
        confirmManual: "I reviewed the selected local files and agree to write this MCP into the active tool configuration.",
        pickerFailed: "Could not open the file picker",
        installFailed: "Could not write the MCP configuration",
        windowsMac: "Windows / macOS local",
        windowsRemoteMac: "Windows local / macOS remote",
        burpProxyRequired: `${toolName} uses the official stdio proxy`,
      };
}

export function McpInstallCatalog({
  lang,
  tool,
  servers,
  open,
  busy,
  onClose,
  onInstall,
  onDetectHost,
  onRestoreHost,
  onOpenExternalUrl,
}: McpInstallCatalogProps) {
  const platform = useMemo(currentPlatform, []);
  const copy = useMemo(() => getCopy(lang, toolLabel(tool)), [lang, tool]);
  const [selectedId, setSelectedId] = useState<McpIntegrationId | null>(null);
  const [form, setForm] = useState<InstallForm | null>(null);
  const [pickerError, setPickerError] = useState("");
  const [installError, setInstallError] = useState("");
  const [hostPlan, setHostPlan] = useState<McpHostInstallPlan | null>(null);
  const [hostPlanBusy, setHostPlanBusy] = useState(false);
  const [hostPlanError, setHostPlanError] = useState("");
  const [restoreBusy, setRestoreBusy] = useState(false);
  const selected = integrations.find((integration) => integration.id === selectedId) || null;
  const sourceKind = selected && form ? sourceKindFor(selected, form.mode) : "none";

  useEffect(() => {
    if (open) return;
    setSelectedId(null);
    setForm(null);
    setPickerError("");
    setInstallError("");
    setHostPlan(null);
    setHostPlanError("");
    setRestoreBusy(false);
  }, [open]);

  useEffect(() => {
    if (!selected) return;
    setForm(createForm(selected, platform, tool));
    setPickerError("");
    setInstallError("");
    setHostPlan(null);
    setHostPlanError("");
  }, [platform, selectedId, tool]);

  useEffect(() => {
    if (!open || !selected || !form || form.sourceMode !== "managed") {
      setHostPlan(null);
      setHostPlanBusy(false);
      setHostPlanError("");
      return;
    }
    let cancelled = false;
    setHostPlanBusy(true);
    setHostPlanError("");
    const timer = window.setTimeout(() => {
      void onDetectHost(selected.id, form.mode, form.hostPath.trim() || null)
        .then((plan) => {
          if (!cancelled) setHostPlan(plan);
        })
        .catch((error) => {
          if (!cancelled) {
            setHostPlan(null);
            setHostPlanError(`${copy.hostDetectFailed}: ${String(error)}`);
          }
        })
        .finally(() => {
          if (!cancelled) setHostPlanBusy(false);
        });
    }, form.hostPath.trim() ? 300 : 0);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [copy.hostDetectFailed, form?.hostPath, form?.mode, form?.sourceMode, onDetectHost, open, selected?.id]);

  const selectIntegration = (integration: McpIntegrationDefinition) => {
    setSelectedId(integration.id);
  };

  const close = () => {
    if (!busy) onClose();
  };

  const back = () => {
    if (busy) return;
    setSelectedId(null);
    setForm(null);
    setPickerError("");
    setInstallError("");
    setHostPlan(null);
    setHostPlanError("");
  };

  const updateForm = (patch: Partial<InstallForm>) => {
    setInstallError("");
    setForm((current) => current ? { ...current, ...patch } : current);
  };

  const setMode = (mode: InstallMode) => {
    if (!selected || !form) return;
    const next = createForm(selected, platform, tool);
    updateForm({
      mode,
      sourcePath: sourceKindFor(selected, mode) === sourceKind ? form.sourcePath : "",
      command: next.command,
      endpoint: form.endpoint || next.endpoint,
      confirmed: false,
    });
    setPickerError("");
    setInstallError("");
  };

  const setSourceMode = (sourceMode: SourceMode) => {
    updateForm({ sourceMode, sourcePath: "", confirmed: false });
    setPickerError("");
    setInstallError("");
  };

  const chooseSource = async () => {
    if (!selected || !form || sourceKind === "none") return;
    setPickerError("");
    try {
      const result = await openDialog({
        multiple: false,
        directory: sourceKind === "directory",
        title: sourceKind === "directory"
          ? copy.chooseDirectory
          : sourceKind === "python"
            ? copy.choosePython
            : copy.chooseJar,
        filters: sourceKind === "python"
          ? [{ name: "Python", extensions: ["py"] }]
          : sourceKind === "jar"
            ? [{ name: "Java archive", extensions: ["jar"] }]
            : undefined,
      });
      const path = Array.isArray(result) ? result[0] : result;
      if (path) updateForm({ sourcePath: path, confirmed: false });
    } catch (error) {
      setPickerError(`${copy.pickerFailed}: ${String(error)}`);
    }
  };

  const chooseHost = async () => {
    if (!selected || !form) return;
    setHostPlanError("");
    try {
      const result = await openDialog({
        multiple: false,
        directory: true,
        title: copy.chooseHost,
      });
      const path = Array.isArray(result) ? result[0] : result;
      if (path) updateForm({ hostPath: path, confirmed: false });
    } catch (error) {
      setHostPlanError(`${copy.pickerFailed}: ${String(error)}`);
    }
  };

  const restoreHost = async () => {
    if (!selected || restoreBusy || busy) return;
    setRestoreBusy(true);
    setHostPlanError("");
    try {
      const message = await onRestoreHost(selected.id);
      setHostPlan((current) => current ? { ...current, message, canRestore: false } : current);
    } catch (error) {
      setHostPlanError(`${copy.hostRestoreFailed}: ${String(error)}`);
    } finally {
      setRestoreBusy(false);
    }
  };

  const localBridgeNeedsHost = Boolean(
    selected
      && form
      && form.sourceMode === "managed"
      && form.mode === "local"
      && (selected.id === "cheatengine-mcp" || selected.id === "x64dbg-mcp"),
  );
  const hostReady = !localBridgeNeedsHost || hostPlan?.status === "ready";

  const canInstall = Boolean(
    selected
      && form
      && form.confirmed
      && hostReady
      && (form.sourceMode === "managed" || sourceKind === "none" || form.sourcePath.trim())
      && (selected.id === "burp-suite-mcp" && form.mode === "direct" || form.command.trim())
      && (!selected.defaultEndpoint || form.endpoint.trim()),
  );

  const install = async () => {
    if (!selected || !form || !canInstall) return;
    setInstallError("");
    try {
      const result = await onInstall({
        integrationId: selected.id,
        sourcePath: form.sourceMode === "managed" || sourceKind === "none" ? null : form.sourcePath.trim(),
        hostPath: form.hostPath.trim() || null,
        command: selected.id === "burp-suite-mcp" && form.mode === "direct" ? null : form.command.trim(),
        endpoint: selected.defaultEndpoint ? form.endpoint.trim() : null,
        mode: form.mode,
        sourceMode: form.sourceMode,
      });
      if (result.ok) {
        onClose();
      } else {
        setInstallError(result.error || copy.installFailed);
      }
    } catch (error) {
      setInstallError(`${copy.installFailed}: ${String(error)}`);
    }
  };

  const modeOptions = selected?.id === "burp-suite-mcp" && supportsBurpDirectSse(tool)
    ? [
        { id: "direct" as const, label: copy.directMode },
        { id: "proxy" as const, label: copy.proxyMode },
      ]
    : selected?.id === "cheatengine-mcp" || selected?.id === "x64dbg-mcp"
      ? [
          ...(platform === "windows" ? [{ id: "local" as const, label: copy.localMode }] : []),
          { id: "remote" as const, label: copy.remoteMode },
        ]
      : [];

  const footer = selected && form ? (
    <>
      <Button variant="secondary" icon={<ArrowLeft />} onClick={back} disabled={busy}>
        {copy.back}
      </Button>
      <Button
        icon={form.sourceMode === "managed" ? <Download /> : <Wrench />}
        onClick={() => void install()}
        disabled={busy || !canInstall}
      >
        {busy ? copy.installing : form.sourceMode === "managed" ? copy.fetchInstall : copy.install}
      </Button>
    </>
  ) : undefined;

  return (
    <ModalShell
      open={open}
      onClose={close}
      title={copy.title}
      description={copy.description}
      closeLabel={copy.close}
      size="xl"
      closeOnBackdrop={!busy}
      closeOnEscape={!busy}
      showCloseButton={!busy}
      className="cx-mcp-catalog-modal"
      bodyClassName="cx-mcp-catalog-body"
      footer={footer}
    >
      {!selected || !form ? (
        <div className="cx-mcp-catalog-grid">
          {integrations.map((integration) => {
            const server = servers.find((candidate) => candidate.id === integration.id);
            const status = server?.enabled
              ? { label: copy.configured, tone: "success" as const }
              : server
                ? { label: copy.added, tone: "info" as const }
                : { label: copy.notConfigured, tone: "neutral" as const };
            const Icon = integration.icon;
            const remoteCapable = integration.id === "cheatengine-mcp" || integration.id === "x64dbg-mcp";
            return (
              <article className="cx-mcp-catalog-card" key={integration.id}>
                <div className="cx-mcp-catalog-card-head">
                  <span className="cx-mcp-catalog-icon"><Icon size={21} aria-hidden="true" /></span>
                  <StatusBadge tone={status.tone}>{status.label}</StatusBadge>
                </div>
                <div className="cx-mcp-catalog-card-copy">
                  <strong>{integration.name}</strong>
                  <span>{integration.vendor}</span>
                  <p>{localized(integration.description, lang)}</p>
                </div>
                <div className="cx-mcp-catalog-card-meta">
                  <Network size={14} aria-hidden="true" />
                  <span>{remoteCapable ? copy.windowsRemoteMac : copy.windowsMac}</span>
                </div>
                <div className="cx-mcp-catalog-card-actions">
                  <IconButton
                    icon={<ExternalLink size={16} />}
                    label={`${integration.name} ${copy.project}`}
                    variant="ghost"
                    onClick={() => onOpenExternalUrl(integration.projectUrl)}
                  />
                  <Button
                    size="sm"
                    variant={server?.enabled ? "secondary" : "primary"}
                    onClick={() => selectIntegration(integration)}
                  >
                    {server ? copy.reconfigure : copy.configure}
                  </Button>
                </div>
              </article>
            );
          })}
        </div>
      ) : (
        <div className="cx-mcp-installer">
          <div className="cx-mcp-installer-head">
            <IconButton icon={<ArrowLeft size={17} />} label={copy.back} variant="ghost" onClick={back} disabled={busy} />
            <span className="cx-mcp-catalog-icon"><selected.icon size={22} aria-hidden="true" /></span>
            <div>
              <strong>{selected.name}</strong>
              <span>{selected.vendor}</span>
            </div>
            <IconButton
              icon={<ExternalLink size={16} />}
              label={`${selected.name} ${copy.project}`}
              variant="ghost"
              onClick={() => onOpenExternalUrl(selected.projectUrl)}
            />
          </div>

          <div className="cx-mcp-installer-layout">
            <section className="cx-mcp-installer-form">
              <div className="cx-mcp-installer-field">
                <span>{copy.sourceMode}</span>
                <div className="cx-mcp-mode-switch" role="radiogroup" aria-label={copy.sourceMode}>
                  {([
                    { id: "managed" as const, label: copy.managedSource },
                    { id: "manual" as const, label: copy.manualSource },
                  ]).map((option) => (
                    <button
                      type="button"
                      role="radio"
                      aria-checked={form.sourceMode === option.id}
                      className={cx("cx-mcp-mode-option", form.sourceMode === option.id && "cx-mcp-mode-option--active")}
                      key={option.id}
                      onClick={() => setSourceMode(option.id)}
                      disabled={busy}
                    >
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>

              {modeOptions.length > 0 && (
                <div className="cx-mcp-installer-field">
                  <span>{copy.mode}</span>
                  <div className="cx-mcp-mode-switch" role="radiogroup" aria-label={copy.mode}>
                    {modeOptions.map((option) => (
                      <button
                        type="button"
                        role="radio"
                        aria-checked={form.mode === option.id}
                        className={cx("cx-mcp-mode-option", form.mode === option.id && "cx-mcp-mode-option--active")}
                        key={option.id}
                        onClick={() => setMode(option.id)}
                        disabled={busy}
                      >
                        {option.label}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {form.sourceMode === "manual" && sourceKind !== "none" && (
                <label className="cx-mcp-installer-field">
                  <span>{sourceKind === "directory" ? copy.sourceDirectory : sourceKind === "python" ? copy.pythonFile : copy.proxyJar}</span>
                  <div className="cx-mcp-path-control">
                    <input
                      value={form.sourcePath}
                      onChange={(event) => updateForm({ sourcePath: event.target.value, confirmed: false })}
                      disabled={busy}
                      spellCheck={false}
                    />
                    <IconButton
                      icon={<FolderOpen size={17} />}
                      label={sourceKind === "directory" ? copy.chooseDirectory : sourceKind === "python" ? copy.choosePython : copy.chooseJar}
                      onClick={() => void chooseSource()}
                      disabled={busy}
                    />
                  </div>
                </label>
              )}

              {form.sourceMode === "managed" && form.mode !== "remote" && (
                <label className="cx-mcp-installer-field">
                  <span>{copy.hostPath}</span>
                  <div className="cx-mcp-path-control">
                    <input
                      value={form.hostPath}
                      placeholder={copy.hostPathPlaceholder}
                      onChange={(event) => updateForm({ hostPath: event.target.value, confirmed: false })}
                      disabled={busy}
                      spellCheck={false}
                    />
                    <IconButton
                      icon={<FolderOpen size={17} />}
                      label={copy.chooseHost}
                      onClick={() => void chooseHost()}
                      disabled={busy}
                    />
                  </div>
                </label>
              )}

              {!(selected.id === "burp-suite-mcp" && form.mode === "direct") && (
                <label className="cx-mcp-installer-field">
                  <span>{copy.runtimeCommand}</span>
                  <input
                    value={form.command}
                    onChange={(event) => updateForm({ command: event.target.value, confirmed: false })}
                    disabled={busy}
                    spellCheck={false}
                  />
                </label>
              )}

              {selected.defaultEndpoint && !(selected.id === "cheatengine-mcp" && form.mode === "local") && (
                <label className="cx-mcp-installer-field">
                  <span>{selected.id === "cheatengine-mcp" && form.mode === "remote" ? copy.tcpEndpoint : copy.endpoint}</span>
                  <input
                    value={form.endpoint}
                    onChange={(event) => updateForm({ endpoint: event.target.value, confirmed: false })}
                    disabled={busy}
                    spellCheck={false}
                  />
                </label>
              )}

              {(pickerError || installError || hostPlanError) && (
                <p className="cx-mcp-picker-error" role="alert">
                  {pickerError || installError || hostPlanError}
                </p>
              )}
            </section>

            <aside className="cx-mcp-installer-review">
              <div className="cx-mcp-requirements">
                <strong>{copy.requirements}</strong>
                <div>
                  {selected.requirements.map((requirement) => (
                    <span key={localized(requirement, lang)}>{localized(requirement, lang)}</span>
                  ))}
                  {selected.id === "burp-suite-mcp" && !supportsBurpDirectSse(tool) && (
                    <span>{copy.burpProxyRequired}</span>
                  )}
                </div>
              </div>
              {form.sourceMode === "managed" && (
                <div className="cx-mcp-host-plan" aria-live="polite">
                  <div className="cx-mcp-host-plan-head">
                    <strong>{copy.hostSoftware}</strong>
                    {hostPlanBusy ? (
                      <StatusBadge tone="info">{copy.detectingHost}</StatusBadge>
                    ) : hostPlan ? (
                      <StatusBadge tone={hostPlan.status === "ready" || hostPlan.status === "detected" ? "success" : hostPlan.status === "remote" ? "info" : "warning"}>
                        {hostPlan.status === "ready"
                          ? copy.hostReady
                          : hostPlan.status === "detected"
                            ? copy.hostDetected
                            : hostPlan.status === "remote"
                              ? copy.hostRemote
                              : hostPlan.status === "missing"
                                ? copy.hostMissing
                                : copy.hostManual}
                      </StatusBadge>
                    ) : null}
                  </div>
                  {hostPlan && (
                    <>
                      <p>{hostPlan.message}</p>
                      {hostPlan.hostPath && <code>{hostPlan.hostPath}</code>}
                      {hostPlan.targets.map((target) => (
                        <div className="cx-mcp-host-target" key={target.path}>
                          <Search size={14} aria-hidden="true" />
                          <span>{target.path}</span>
                        </div>
                      ))}
                      {hostPlan.nextStep && <small>{hostPlan.nextStep}</small>}
                      {hostPlan.canRestore && (
                        <Button
                          size="sm"
                          variant="secondary"
                          icon={<RotateCcw size={15} />}
                          onClick={() => void restoreHost()}
                          disabled={busy || restoreBusy}
                        >
                          {restoreBusy ? copy.restoringHost : copy.restoreHost}
                        </Button>
                      )}
                    </>
                  )}
                </div>
              )}
              <div className="cx-mcp-no-download">
                <ShieldCheck size={18} aria-hidden="true" />
                <p>
                  {form.sourceMode === "managed"
                    ? `${selected.managedVersion} · ${selected.managedLicense}. ${localized(selected.managedSummary, lang)} ${copy.managedNotice}`
                    : copy.manualNotice}
                </p>
              </div>
              <Checkbox
                checked={form.confirmed}
                onCheckedChange={(confirmed) => updateForm({ confirmed })}
                label={form.sourceMode === "managed" ? copy.confirmManaged : copy.confirmManual}
                disabled={busy}
                className="cx-mcp-install-confirm"
              />
            </aside>
          </div>
        </div>
      )}
    </ModalShell>
  );
}
