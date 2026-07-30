import {
  ArrowUpRight,
  CheckCircle2,
  Code2,
  FileText,
  KeyRound,
  RefreshCw,
  Wrench,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { ToolTabs } from "../components/ToolTabs";
import type { ToolId, ToolStatus } from "../types";
import "../styles/overview-page.css";

export type OverviewLanguage = "zh" | "en";

export type OverviewPageProps = {
  lang: OverviewLanguage;
  tool: ToolId;
  toolStatuses: readonly ToolStatus[];
  onToolChange: (tool: ToolId) => void;
  model?: string | null;
  configDir: string;
  resolvedCodexDir: string;
  configExists: boolean;
  providerLabel?: string | null;
  instructionEnabled: boolean;
  authExists: boolean;
  configPath?: string | null;
  modelProvider?: string | null;
  instructionPath?: string | null;
  loading: boolean;
  hasUpdate: boolean;
  latestVersion?: string | null;
  onConfigDirChange: (value: string) => void;
  onRefresh: () => void;
  onOpenUpdate: () => void;
};

type StatusCardProps = {
  icon: LucideIcon;
  label: string;
  value: string;
  tone: "success" | "active" | "muted";
};

function StatusCard({ icon: Icon, label, value, tone }: StatusCardProps) {
  return (
    <article className={`cx-overview-status-card cx-overview-status-card--${tone}`}>
      <div className="cx-overview-status-icon" aria-hidden="true">
        <Icon size={19} strokeWidth={1.9} />
      </div>
      <div className="cx-overview-status-copy">
        <span>{label}</span>
        <strong title={value}>{value}</strong>
      </div>
    </article>
  );
}

function ConfigRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="cx-overview-config-row">
      <span>{label}</span>
      <code title={value}>{value}</code>
    </div>
  );
}

export function OverviewPage({
  lang,
  tool,
  toolStatuses,
  onToolChange,
  model,
  configDir,
  resolvedCodexDir,
  configExists,
  providerLabel,
  instructionEnabled,
  authExists,
  configPath,
  modelProvider,
  instructionPath,
  loading,
  hasUpdate,
  latestVersion,
  onConfigDirChange,
  onRefresh,
  onOpenUpdate,
}: OverviewPageProps) {
  const isChinese = lang === "zh";
  const status = toolStatuses.find((item) => item.id === tool);
  const codexFallback = tool === "codex";
  const activeModel = status?.model
    || (codexFallback ? model : null)
    || (isChinese ? "未配置模型" : "Model not configured");
  const activeConfigExists = status?.configExists ?? configExists;
  const activeAuthExists = status?.authExists ?? authExists;
  const activeInstructionEnabled = status?.instructionEnabled ?? instructionEnabled;
  const activeProvider = status?.provider
    || (codexFallback ? (providerLabel || modelProvider) : null)
    || (codexFallback
      ? (isChinese ? "官方配置" : "Official")
      : (isChinese ? "未配置供应商" : "Provider not configured"));
  const activeHome = status?.homeDir
    || (codexFallback ? (resolvedCodexDir || configDir) : null)
    || (isChinese ? "未设置" : "Not set");
  const activeConfigPath = status?.configPath
    || (codexFallback ? configPath : null)
    || (isChinese ? "未设置" : "Not set");
  const activeInstructionPath = status?.instructionPath
    || (codexFallback ? instructionPath : null)
    || (isChinese ? "未设置" : "Not set");
  const updateVersion = latestVersion?.trim() || "";
  const editableHome = tool === "codex";
  const text = isChinese
    ? {
        eyebrow: `${status?.label || "工具"} 环境`,
        directory: "配置目录",
        directoryPlaceholder: "留空使用默认目录",
        load: "加载",
        config: "配置文件",
        found: "已找到",
        missing: "未找到",
        provider: "供应商 / 后端",
        instruction: "指令状态",
        enabled: "已启用",
        disabled: "未启用",
        auth: "认证文件",
        authFound: "已找到",
        authMissing: "未找到",
        updateFound: "发现新版本",
        updateAvailable: (version: string) => `DevConduit ${version} 已发布`,
        viewUpdate: "查看更新",
        liveStatus: "当前状态",
        currentConfig: `${status?.label || "工具"} 配置`,
        model: "模型",
        providerName: "供应商",
        configPath: "主配置",
        instructionFile: "指令文件",
        nativeInstruction: "原生指令路径",
        noValue: "未设置",
        unavailable: "未检测到",
      }
    : {
        eyebrow: `${status?.label || "Tool"} environment`,
        directory: "Config directory",
        directoryPlaceholder: "Leave empty for the default directory",
        load: "Load",
        config: "Config file",
        found: "Found",
        missing: "Missing",
        provider: "Provider / backend",
        instruction: "Instructions",
        enabled: "Enabled",
        disabled: "Disabled",
        auth: "Auth file",
        authFound: "Found",
        authMissing: "Missing",
        updateFound: "New version available",
        updateAvailable: (version: string) => `DevConduit ${version} is available`,
        viewUpdate: "View update",
        liveStatus: "CURRENT STATUS",
        currentConfig: `${status?.label || "Tool"} configuration`,
        model: "Model",
        providerName: "Provider",
        configPath: "Primary config",
        instructionFile: "Instruction file",
        nativeInstruction: "Native instruction path",
        noValue: "Not set",
        unavailable: "Not detected",
      };

  return (
    <section className="cx-overview-page" aria-label={isChinese ? "概览" : "Overview"}>
      <header className="cx-overview-header">
        <div className="cx-overview-heading">
          <p className="cx-overview-eyebrow">
            <span className="cx-overview-live-dot" aria-hidden="true" />
            {text.eyebrow}
          </p>
          <h2 title={activeModel}>{activeModel}</h2>
        </div>
        <div className="cx-overview-home-control">
          <label htmlFor="cx-overview-tool-home">{text.directory}</label>
          <input
            id="cx-overview-tool-home"
            type="text"
            value={editableHome ? (configDir || resolvedCodexDir) : activeHome}
            onChange={(event) => onConfigDirChange(event.target.value)}
            placeholder={text.directoryPlaceholder}
            spellCheck={false}
            readOnly={!editableHome}
            aria-label={text.directory}
          />
          <button type="button" onClick={onRefresh} disabled={loading}>
            <RefreshCw size={15} strokeWidth={2} className={loading ? "cx-overview-spin" : undefined} aria-hidden="true" />
            {text.load}
          </button>
        </div>
      </header>

      <ToolTabs
        active={tool}
        onChange={onToolChange}
        statuses={toolStatuses}
        ariaLabel={isChinese ? "工具" : "Tools"}
        className="cx-overview-tool-tabs"
      />

      {hasUpdate && (
        <aside className="cx-overview-update-strip" role="status">
          <div className="cx-overview-update-copy">
            <span className="cx-overview-update-dot" aria-hidden="true" />
            <div>
              <strong>{text.updateFound}</strong>
              {updateVersion && <p>{text.updateAvailable(updateVersion)}</p>}
            </div>
          </div>
          <button type="button" onClick={onOpenUpdate}>
            {text.viewUpdate}
            <ArrowUpRight size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </aside>
      )}

      {status?.notice && (
        <div className="cx-overview-tool-notice">
          <Wrench size={16} aria-hidden="true" />
          <span>{status.notice}</span>
        </div>
      )}

      <div className="cx-overview-status-grid">
        <StatusCard
          icon={FileText}
          label={text.config}
          value={activeConfigExists ? text.found : text.missing}
          tone={activeConfigExists ? "success" : "muted"}
        />
        <StatusCard
          icon={Code2}
          label={text.provider}
          value={activeProvider}
          tone={activeProvider !== text.noValue ? "active" : "muted"}
        />
        <StatusCard
          icon={Wrench}
          label={text.instruction}
          value={activeInstructionEnabled ? text.enabled : text.disabled}
          tone={activeInstructionEnabled ? "success" : "muted"}
        />
        <StatusCard
          icon={KeyRound}
          label={text.auth}
          value={activeAuthExists ? text.authFound : text.authMissing}
          tone={activeAuthExists ? "success" : "muted"}
        />
      </div>

      <section className="cx-overview-config-panel">
        <div className="cx-overview-panel-heading">
          <div>
            <p className="cx-overview-section-label">{text.liveStatus}</p>
            <h3>{text.currentConfig}</h3>
          </div>
          <span className={`cx-overview-instruction-pill${activeInstructionEnabled ? " cx-overview-instruction-pill--active" : ""}`}>
            <CheckCircle2 size={14} strokeWidth={2} aria-hidden="true" />
            {activeInstructionEnabled ? text.enabled : text.disabled}
          </span>
        </div>
        <div className="cx-overview-config-list">
          <ConfigRow label={text.directory} value={activeHome} />
          <ConfigRow label={text.configPath} value={activeConfigPath} />
          <ConfigRow label={text.model} value={activeModel || text.noValue} />
          <ConfigRow label={text.providerName} value={activeProvider || text.noValue} />
          <ConfigRow label={text.instructionFile} value={activeInstructionPath} />
          {status && status.nativeInstructionPath !== status.instructionPath && (
            <ConfigRow label={text.nativeInstruction} value={status.nativeInstructionPath} />
          )}
          {status?.version && <ConfigRow label="Version" value={status.version} />}
          {!status && <ConfigRow label={text.providerName} value={text.unavailable} />}
        </div>
      </section>
    </section>
  );
}
