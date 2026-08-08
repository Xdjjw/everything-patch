import type { ToolId, ToolStatus } from "../types";
import "../styles/tool-tabs.css";

export const TOOL_ORDER: ToolId[] = ["codex", "claude", "grok", "zcode", "kilo", "pi"];

const TOOL_LABELS: Record<ToolId, string> = {
  codex: "Codex",
  claude: "Claude Code",
  grok: "Grok Build",
  zcode: "ZCode",
  kilo: "Kilo Code",
  pi: "Pi",
};

export type ToolTabsProps = {
  active: ToolId;
  onChange: (tool: ToolId) => void;
  statuses?: readonly ToolStatus[];
  className?: string;
  ariaLabel?: string;
};

export function ToolTabs({
  active,
  onChange,
  statuses = [],
  className,
  ariaLabel = "Tools",
}: ToolTabsProps) {
  return (
    <div className={`cx-tool-tabs${className ? ` ${className}` : ""}`} role="tablist" aria-label={ariaLabel}>
      {TOOL_ORDER.map((tool) => {
        const status = statuses.find((item) => item.id === tool);
        const installed = status?.installed !== false;
        return (
          <button
            key={tool}
            type="button"
            role="tab"
            aria-selected={active === tool}
            className={`cx-tool-tab${active === tool ? " cx-tool-tab--active" : ""}`}
            onClick={() => onChange(tool)}
          >
            <span className={`cx-tool-tab-dot${installed ? " cx-tool-tab-dot--ready" : ""}`} aria-hidden="true" />
            <span>{TOOL_LABELS[tool]}</span>
          </button>
        );
      })}
    </div>
  );
}

export function toolLabel(tool: ToolId) {
  return TOOL_LABELS[tool];
}
