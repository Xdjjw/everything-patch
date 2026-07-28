import type { ReactNode } from "react";
import {
  CheckCircle2,
  Download,
  ExternalLink,
  FileCode2,
  Globe2,
  Loader2,
  RefreshCw,
  Sparkles,
  TerminalSquare,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { ToolTabs } from "../components/ToolTabs";
import type { ToolConfigBundle, ToolId, ToolStatus } from "../types";
import "../styles/utility-pages.css";

export type UtilityLanguage = "zh" | "en";
export type UtilityStatusTone = "neutral" | "success" | "warning" | "error";

type PageHeaderProps = {
  eyebrow: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  aside?: ReactNode;
};

function PageHeader({ eyebrow, title, description, aside }: PageHeaderProps) {
  return (
    <header className="cx-page-header">
      <div className="cx-page-header-copy">
        <div className="cx-page-eyebrow">{eyebrow}</div>
        <h2>{title}</h2>
        {description && <p>{description}</p>}
      </div>
      {aside}
    </header>
  );
}

export type ToolConfigPageProps = {
  lang: UtilityLanguage;
  tool: ToolId;
  toolStatuses: readonly ToolStatus[];
  config: ToolConfigBundle | null;
  selectedFileId: string;
  loading: boolean;
  preview: ReactNode;
  onToolChange: (tool: ToolId) => void;
  onFileChange: (fileId: string) => void;
  onRefresh: () => void;
};

export function ToolConfigPage({
  lang,
  tool,
  toolStatuses,
  config,
  selectedFileId,
  loading,
  preview,
  onToolChange,
  onFileChange,
  onRefresh,
}: ToolConfigPageProps) {
  const isChinese = lang === "zh";
  const activeFile = config?.files.find((file) => file.id === selectedFileId)
    || config?.files.find((file) => file.id === config.primaryFileId)
    || config?.files[0];
  const title = isChinese ? `${config?.label || "工具"} 配置` : `${config?.label || "Tool"} configuration`;
  const description = isChinese
    ? "按工具查看原生配置文件。所有密钥和令牌均由后端递归脱敏。"
    : "Inspect each tool's native configuration. Secrets and tokens are recursively redacted by the backend.";
  const loaded = loading
    ? (isChinese ? "读取中" : "Loading")
    : activeFile?.exists
      ? (isChinese ? "已读取" : "Loaded")
      : (isChinese ? "未找到" : "Missing");
  return (
    <section className="cx-utility cx-page cx-page--toml">
      <PageHeader
        eyebrow={activeFile?.path || config?.label || "Config"}
        title={title}
        description={description}
        aside={(
          <button
            type="button"
            className={`cx-page-header-status cx-page-header-status--button${activeFile?.exists ? "" : " cx-page-header-status--missing"}`}
            aria-live="polite"
            onClick={onRefresh}
            disabled={loading}
          >
            {loading ? <Loader2 size={13} className="cx-page-spin" aria-hidden="true" /> : <RefreshCw size={13} aria-hidden="true" />}
            <span className="cx-page-status-dot" aria-hidden="true" />
            <span>{loaded}</span>
          </button>
        )}
      />
      <ToolTabs
        active={tool}
        onChange={onToolChange}
        statuses={toolStatuses}
        ariaLabel={isChinese ? "配置工具" : "Configuration tools"}
        className="cx-config-tool-tabs"
      />
      <div className="cx-config-file-bar">
        <div className="cx-config-file-copy">
          <FileCode2 size={17} aria-hidden="true" />
          <span>
            <strong>{activeFile?.label || (isChinese ? "主配置" : "Primary config")}</strong>
            <code title={activeFile?.path}>{activeFile?.path || (isChinese ? "尚未读取" : "Not loaded")}</code>
          </span>
        </div>
        {(config?.files.length || 0) > 1 && (
          <label className="cx-config-file-select">
            <span>{isChinese ? "配置文件" : "Config file"}</span>
            <select value={activeFile?.id || ""} onChange={(event) => onFileChange(event.target.value)}>
              {config?.files.map((file) => (
                <option value={file.id} key={file.id}>{file.label}</option>
              ))}
            </select>
          </label>
        )}
      </div>
      {config?.notice && <p className="cx-config-notice">{config.notice}</p>}
      <section className="cx-page-panel cx-page-code-panel">
        <div className="cx-page-code-frame">{preview}</div>
      </section>
    </section>
  );
}

export type SettingsCopy = {
  eyebrow: ReactNode;
  title: ReactNode;
  languageTitle: ReactNode;
  languageDescription: ReactNode;
  chineseLabel: ReactNode;
  englishLabel: ReactNode;
  productTitle: ReactNode;
  productDescription: ReactNode;
  productValue: ReactNode;
  recheckTitle: ReactNode;
  recheckDescription: ReactNode;
  recheckLabel: ReactNode;
};

export type SettingsPageProps = {
  lang: UtilityLanguage;
  copy: SettingsCopy;
  onLanguageChange: (lang: UtilityLanguage) => void;
  onRecheck: () => void;
  recheckBusy?: boolean;
};

type SettingRowProps = {
  icon: LucideIcon;
  title: ReactNode;
  description: ReactNode;
  action: ReactNode;
};

function SettingRow({ icon: Icon, title, description, action }: SettingRowProps) {
  return (
    <div className="cx-page-setting-row">
      <div className="cx-page-setting-icon" aria-hidden="true">
        <Icon size={18} strokeWidth={1.9} />
      </div>
      <div className="cx-page-setting-copy">
        <strong>{title}</strong>
        <p>{description}</p>
      </div>
      <div className="cx-page-setting-action">{action}</div>
    </div>
  );
}

export function SettingsPage({
  lang,
  copy,
  onLanguageChange,
  onRecheck,
  recheckBusy = false,
}: SettingsPageProps) {
  return (
    <section className="cx-utility cx-page cx-page--settings">
      <PageHeader eyebrow={copy.eyebrow} title={copy.title} />
      <div className="cx-page-settings-list">
        <SettingRow
          icon={Globe2}
          title={copy.languageTitle}
          description={copy.languageDescription}
          action={(
            <div className="cx-page-segmented" role="group" aria-label={String(copy.languageTitle)}>
              <button
                type="button"
                className={lang === "zh" ? "cx-page-segmented-button cx-page-segmented-button--active" : "cx-page-segmented-button"}
                onClick={() => onLanguageChange("zh")}
                aria-pressed={lang === "zh"}
              >
                {copy.chineseLabel}
              </button>
              <button
                type="button"
                className={lang === "en" ? "cx-page-segmented-button cx-page-segmented-button--active" : "cx-page-segmented-button"}
                onClick={() => onLanguageChange("en")}
                aria-pressed={lang === "en"}
              >
                {copy.englishLabel}
              </button>
            </div>
          )}
        />

        <SettingRow
          icon={Sparkles}
          title={copy.productTitle}
          description={copy.productDescription}
          action={<span className="cx-page-value-pill">{copy.productValue}</span>}
        />

        <SettingRow
          icon={CheckCircle2}
          title={copy.recheckTitle}
          description={copy.recheckDescription}
          action={(
            <button
              type="button"
              className="cx-page-button cx-page-button--secondary"
              onClick={onRecheck}
              disabled={recheckBusy}
            >
              {recheckBusy && <Loader2 size={15} className="cx-page-spin" aria-hidden="true" />}
              {copy.recheckLabel}
            </button>
          )}
        />
      </div>
    </section>
  );
}

export type AboutCopy = {
  eyebrow: ReactNode;
  title: ReactNode;
  appVersionLabel: ReactNode;
  projectLabel: ReactNode;
  environmentsTitle: ReactNode;
  installedLabel: ReactNode;
  missingLabel: ReactNode;
  versionLabel: ReactNode;
  homeLabel: ReactNode;
  configLabel: ReactNode;
  openProjectLabel: ReactNode;
  openIssuesLabel: ReactNode;
  releasesEyebrow: ReactNode;
  releasesTitle: ReactNode;
  releaseStatusLabel: ReactNode;
  latestVersionLabel: ReactNode;
  checkUpdateLabel: ReactNode;
  openReleasesLabel: ReactNode;
};

export type AboutReleaseState = {
  status: ReactNode;
  latestVersion: ReactNode;
  tone?: UtilityStatusTone;
  checking?: boolean;
  canOpenReleases?: boolean;
};

export type AboutPageProps = {
  copy: AboutCopy;
  appVersion: ReactNode;
  toolStatuses: readonly ToolStatus[];
  projectUrl: ReactNode;
  release: AboutReleaseState;
  onOpenProject: () => void;
  onOpenIssues: () => void;
  onCheckUpdate: () => void;
  onOpenReleases: () => void;
};

type InfoRowProps = {
  label: ReactNode;
  value: ReactNode;
  mono?: boolean;
};

function InfoRow({ label, value, mono = false }: InfoRowProps) {
  return (
    <div className="cx-page-info-row">
      <span>{label}</span>
      <strong className={mono ? "cx-page-info-value cx-page-info-value--mono" : "cx-page-info-value"}>{value}</strong>
    </div>
  );
}

export function AboutPage({
  copy,
  appVersion,
  toolStatuses,
  projectUrl,
  release,
  onOpenProject,
  onOpenIssues,
  onCheckUpdate,
  onOpenReleases,
}: AboutPageProps) {
  const releaseTone = release.tone || "neutral";
  const releaseStatusClass = `cx-page-release-status cx-page-release-status--${releaseTone}`;

  return (
    <section className="cx-utility cx-page cx-page--about">
      <PageHeader eyebrow={copy.eyebrow} title={copy.title} />

      <section className="cx-page-panel cx-page-about-panel">
        <div className="cx-page-info-list">
          <InfoRow label={copy.appVersionLabel} value={appVersion} />
          <InfoRow label={copy.projectLabel} value={projectUrl} mono />
        </div>
        <div className="cx-page-panel-actions">
          <button type="button" className="cx-page-button cx-page-button--secondary" onClick={onOpenProject}>
            <ExternalLink size={15} aria-hidden="true" />
            {copy.openProjectLabel}
          </button>
          <button type="button" className="cx-page-button cx-page-button--secondary" onClick={onOpenIssues}>
            <ExternalLink size={15} aria-hidden="true" />
            {copy.openIssuesLabel}
          </button>
        </div>
      </section>

      <section className="cx-page-panel cx-page-environments-panel">
        <div className="cx-page-environments-heading">
          <TerminalSquare size={17} aria-hidden="true" />
          <h3>{copy.environmentsTitle}</h3>
        </div>
        <div className="cx-page-environment-list">
          {toolStatuses.map((status) => (
            <article className="cx-page-environment-row" key={status.id}>
              <span className={`cx-page-environment-dot${status.installed ? " cx-page-environment-dot--ready" : ""}`} aria-hidden="true" />
              <div className="cx-page-environment-name">
                <strong>{status.label}</strong>
                <small>{status.installed ? copy.installedLabel : copy.missingLabel}</small>
              </div>
              <dl>
                <div><dt>{copy.versionLabel}</dt><dd>{status.version || "—"}</dd></div>
                <div><dt>{copy.homeLabel}</dt><dd title={status.homeDir}>{status.homeDir}</dd></div>
                <div><dt>{copy.configLabel}</dt><dd title={status.configPath}>{status.configPath}</dd></div>
              </dl>
            </article>
          ))}
        </div>
      </section>

      <section className="cx-page-panel cx-page-release-panel">
        <div className="cx-page-release-header">
          <div>
            <div className="cx-page-eyebrow cx-page-eyebrow--muted">{copy.releasesEyebrow}</div>
            <h3>{copy.releasesTitle}</h3>
          </div>
          <span className={releaseStatusClass} aria-live="polite">{release.status}</span>
        </div>
        <div className="cx-page-info-list">
          <InfoRow label={copy.releaseStatusLabel} value={release.status} />
          <InfoRow label={copy.latestVersionLabel} value={release.latestVersion} />
        </div>
        <div className="cx-page-panel-actions">
          <button
            type="button"
            className="cx-page-button cx-page-button--primary"
            onClick={onCheckUpdate}
            disabled={release.checking}
          >
            {release.checking ? <Loader2 size={15} className="cx-page-spin" aria-hidden="true" /> : <RefreshCw size={15} aria-hidden="true" />}
            {copy.checkUpdateLabel}
          </button>
          <button
            type="button"
            className="cx-page-button cx-page-button--secondary"
            onClick={onOpenReleases}
            disabled={release.canOpenReleases === false}
          >
            <Download size={15} aria-hidden="true" />
            {copy.openReleasesLabel}
          </button>
        </div>
      </section>
    </section>
  );
}
