import { useEffect, useId, useMemo, useState } from "react";
import type { ChangeEvent, RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  ArrowLeftRight,
  CircleHelp,
  CirclePlus,
  Eye,
  FileText,
  History,
  Loader2,
  PencilLine,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  ShieldCheck,
  Sparkles,
  Terminal,
  Trash2,
  Upload,
} from "lucide-react";

import { PageTransition } from "../components/PageTransition";
import { PromptCategoryManager } from "../components/PromptCategoryManager";
import type { PromptCategoryItem } from "../components/PromptCategoryManager";
import { Button, Checkbox, IconButton, ModalShell, StatusBadge, Toggle, cx } from "../components/ui";
import { usePromptCategories } from "../promptCategories";
import type {
  BuiltinPromptStatus,
  ClaudeRuntimeState,
  CodexInstructionStatus,
  GrokState,
  KiloState,
  PiState,
  InstructionMode,
  InstructionTemplate,
  Lang,
  PromptBackupEntry,
  PromptEngine,
  PromptInjectionMode,
  PromptRuntimePreview,
  SavedPrompt,
  ZcodeState,
} from "../types";
import "../styles/skills-prompts-pages.css";

type MaybeAsyncAction = () => void | Promise<void>;

export type ExternalPromptItem = {
  title: string;
  description: string;
  filename?: string | null;
};

export type OrphanedBuiltinPrompt = {
  id: string;
  title: string;
  description?: string | null;
};

export type PromptFormField = "title" | "filename" | "content";

export type PromptsPageProps = {
  lang: Lang;
  instructionMode: InstructionMode;
  promptForm: SavedPrompt;
  editingPromptId: string | null;
  loading: boolean;
  actionBusy: string;
  promptSyncing: boolean;
  promptCatalogReady: boolean;
  promptImportRef: RefObject<HTMLInputElement>;
  promptInjectionMode: PromptInjectionMode;
  promptModeHelpOpen: boolean;
  promptModeHelpRef: RefObject<HTMLDivElement>;
  promptEngine: PromptEngine;
  onPromptEngineChange: (engine: PromptEngine) => void;
  instructionEnabled: boolean;
  codexInstructionStatus?: CodexInstructionStatus;
  codexInactiveInstructionFile?: string | null;
  activeInstructionTitle: string;
  activeInjectionMode?: PromptInjectionMode;
  codexRuntimeScope?: string;
  codexRuntimeEntryPath?: string;
  claudeRuntimeEntryPath?: string;
  claudeRuntime?: ClaudeRuntimeState;
  zcodeRuntime?: ZcodeState | null;
  grokRuntime?: GrokState | null;
  kiloRuntime?: KiloState | null;
  piRuntime?: PiState | null;
  promptBackups: PromptBackupEntry[];
  promptBackupsOpen: boolean;
  promptBackupsLoading: boolean;
  promptRestoreBusyId: string;
  instructionTemplates: InstructionTemplate[];
  builtinPromptStatuses: BuiltinPromptStatus[];
  activeBuiltinTemplateId?: string | null;
  orphanedBuiltinPrompt?: OrphanedBuiltinPrompt | null;
  savedPrompts: SavedPrompt[];
  managedSavedPromptId?: string | null;
  preservedSavedPromptFilename?: string | null;
  externalPrompt?: ExternalPromptItem | null;
  className?: string;
  onSyncBuiltinPrompts: MaybeAsyncAction;
  onImportPrompt: (file?: File | null) => void | Promise<void>;
  onAddPrompt: () => void;
  onInstructionModeChange: (mode: InstructionMode) => void;
  onPromptInjectionModeChange: (mode: PromptInjectionMode) => void;
  onTogglePromptModeHelp: () => void;
  onOpenPromptBackups: () => void;
  onClosePromptBackups: () => void;
  onRestorePromptBackup: (id: string) => void | Promise<void>;
  onEnableBuiltinPrompt: (id: string) => void | Promise<void>;
  onDisableInstruction: MaybeAsyncAction;
  onEnableSavedPrompt: (id: string) => void | Promise<void>;
  onDisableExternalPrompt: MaybeAsyncAction;
  onEditPrompt: (prompt: SavedPrompt) => void;
  onDeletePrompt: (id: string) => void | Promise<void>;
  onPromptFormFieldChange: (field: PromptFormField, value: string) => void;
  onSavePrompt: MaybeAsyncAction;
  // ─── Claude 平行 props ────────────────────────────────────────────────
  claudeInstructionEnabled: boolean;
  claudeActiveInstructionTitle: string;
  claudeInstructionTemplates: InstructionTemplate[];
  claudeBuiltinPromptStatuses: BuiltinPromptStatus[];
  claudeActiveBuiltinTemplateId?: string | null;
  claudeSavedPrompts: SavedPrompt[];
  claudeManagedSavedPromptId?: string | null;
  onEnableClaudeBuiltinPrompt: (id: string) => void | Promise<void>;
  onDisableClaudeInstruction: MaybeAsyncAction;
  onEnableClaudeSavedPrompt: (id: string) => void | Promise<void>;
  onEditClaudePrompt: (prompt: SavedPrompt) => void;
  onDeleteClaudePrompt: (id: string) => void | Promise<void>;
  onImportClaudePrompt: (file?: File | null) => void | Promise<void>;
  onSaveClaudePrompt: MaybeAsyncAction;
  onInstallClaudeRuntime: MaybeAsyncAction;
  onUninstallClaudeRuntime: MaybeAsyncAction;
  // ─── ZCode 平行 props ──────────────────────────────────────────────────
  zcodeInstructionEnabled: boolean;
  zcodeActiveInstructionTitle: string;
  zcodeInstructionTemplates: InstructionTemplate[];
  zcodeBuiltinPromptStatuses: BuiltinPromptStatus[];
  zcodeActiveBuiltinTemplateId?: string | null;
  zcodeSavedPrompts: SavedPrompt[];
  zcodeManagedSavedPromptId?: string | null;
  onEnableZcodeBuiltinPrompt: (id: string) => void | Promise<void>;
  onDisableZcodeInstruction: MaybeAsyncAction;
  onEnableZcodeSavedPrompt: (id: string) => void | Promise<void>;
  onEditZcodePrompt: (prompt: SavedPrompt) => void;
  onDeleteZcodePrompt: (id: string) => void | Promise<void>;
  onImportZcodePrompt: (file?: File | null) => void | Promise<void>;
  onSaveZcodePrompt: MaybeAsyncAction;
  // ─── Grok 平行 props ───────────────────────────────────────────────────
  grokInstructionEnabled: boolean;
  grokActiveInstructionTitle: string;
  grokInstructionTemplates: InstructionTemplate[];
  grokBuiltinPromptStatuses: BuiltinPromptStatus[];
  grokActiveBuiltinTemplateId?: string | null;
  grokSavedPrompts: SavedPrompt[];
  grokManagedSavedPromptId?: string | null;
  onEnableGrokBuiltinPrompt: (id: string) => void | Promise<void>;
  onDisableGrokInstruction: MaybeAsyncAction;
  onEnableGrokSavedPrompt: (id: string) => void | Promise<void>;
  onEditGrokPrompt: (prompt: SavedPrompt) => void;
  onDeleteGrokPrompt: (id: string) => void | Promise<void>;
  onImportGrokPrompt: (file?: File | null) => void | Promise<void>;
  onSaveGrokPrompt: MaybeAsyncAction;
  // ─── Kilo parallel props ───────────────────────────────────────────────
  kiloInstructionEnabled: boolean;
  kiloActiveInstructionTitle: string;
  kiloInstructionTemplates: InstructionTemplate[];
  kiloBuiltinPromptStatuses: BuiltinPromptStatus[];
  kiloActiveBuiltinTemplateId?: string | null;
  kiloSavedPrompts: SavedPrompt[];
  kiloManagedSavedPromptId?: string | null;
  onEnableKiloBuiltinPrompt: (id: string) => void | Promise<void>;
  onDisableKiloInstruction: MaybeAsyncAction;
  onEnableKiloSavedPrompt: (id: string) => void | Promise<void>;
  onEditKiloPrompt: (prompt: SavedPrompt) => void;
  onDeleteKiloPrompt: (id: string) => void | Promise<void>;
  onImportKiloPrompt: (file?: File | null) => void | Promise<void>;
  onSaveKiloPrompt: MaybeAsyncAction;
  // ─── Pi parallel props ─────────────────────────────────────────────────
  piInstructionEnabled: boolean;
  piActiveInstructionTitle: string;
  piInstructionTemplates: InstructionTemplate[];
  piBuiltinPromptStatuses: BuiltinPromptStatus[];
  piActiveBuiltinTemplateId?: string | null;
  piSavedPrompts: SavedPrompt[];
  piManagedSavedPromptId?: string | null;
  onEnablePiBuiltinPrompt: (id: string) => void | Promise<void>;
  onDisablePiInstruction: MaybeAsyncAction;
  onEnablePiSavedPrompt: (id: string) => void | Promise<void>;
  onEditPiPrompt: (prompt: SavedPrompt) => void;
  onDeletePiPrompt: (id: string) => void | Promise<void>;
  onImportPiPrompt: (file?: File | null) => void | Promise<void>;
  onSavePiPrompt: MaybeAsyncAction;
};

function getCopy(lang: Lang) {
  return lang === "zh"
    ? {
        eyebrow: "PROMPT INJECTION",
        title: "一键管理指令提示词",
        description: "选择启用方式，再管理内置、在线或自定义的 Markdown 提示词。",
        sync: "同步 GitHub 模板",
        syncing: "同步中...",
        importMd: "导入 md",
        importing: "导入中...",
        add: "添加提示词",
        manageCategories: "分类管理",
        emptyCategory: "该分类下暂无提示词",
        currentStatus: "当前状态",
        noActive: "未启用提示词",
        inactiveByConfig: "当前配置未加载",
        inactiveByConfigDetail: (filename: string) => `检测到 DevConduit 管理的 ${filename}，但当前 config.toml 没有引用它。CCSwitch 切换到不含 model_instructions_file 的配置时会出现这个状态。`,
        keepExisting: "保留原提示词",
        replaceExisting: "替换原提示词",
        appendMode: "追加到 AGENTS.md",
        replaceMode: "替换指令文件",
        appendDetail: "当前模板写入 AGENTS.md，同时保留已有指令文件。",
        replaceDetail: "当前模板通过 model_instructions_file 独立加载。",
        inactiveDetail: "先选择启用方式，再打开下方任一模板。",
        enableMethod: "启用方式",
        helpLabel: "查看启用方式说明",
        appendHelp: "只在 AGENTS.md 中增加 DevConduit 管理区块，不改动原有 model_instructions_file，适合叠加使用。",
        replaceHelp: "当前模板会成为唯一生效的指令入口，原有 model_instructions_file 将被替换。",
        pendingMode: (mode: string) => `当前模式不变，下次启用将使用“${mode}”。`,
        modeHint: "点击模板开关时，使用这里选择的方式。",
        keepTitle: "写入 AGENTS.md，并保留现有 model_instructions_file",
        replaceTitle: "使用 model_instructions_file 替换现有指令文件",
        enable: "启用",
        disable: "关闭",
        disableExternal: "禁用外部提示词",
        current: "当前",
        onlineRemoved: "在线已移除",
        removedDescription: "该模板已从在线目录移除，当前配置仍在使用。",
        customDescription: "自定义指令提示词",
        preservedDescription: "用户原有提示词，追加模式下继续生效。",
        existingPrompt: "用户原有指令提示词",
        engineCodex: "Codex",
        engineClaude: "Claude Code",
        engineZcode: "ZCode",
        engineGrok: "Grok Build",
        engineKilo: "Kilo Code",
        enginePi: "Pi",
        engineSwitchLabel: "指令引擎",
        claudeMode: "写入 ~/.claude/CLAUDE.md import 区块",
        claudeModeDetail: "当前模板写入 ~/.claude/keysmith/ 并在 CLAUDE.md 注入受管 import 区块。",
        claudeActiveMemory: "CLAUDE.md",
        zcodeMode: "写入 ~/.zcode-keysmith system-role + wrapper",
        zcodeModeDetail: "当前模板写入 system-role.md，并通过 wrapper 注入 ZCode runtime 的 system prompt。",
        zcodeActiveMemory: "system-role.md",
        grokMode: "写入 ~/.grok/AGENTS.md + compat 隔离",
        grokModeDetail: "当前模板写入 AGENTS.md，注入 compat 隔离块并隔离 hooks。",
        grokActiveMemory: "AGENTS.md",
        kiloMode: "写入 ~/.config/kilo/AGENTS.md",
        kiloModeDetail: "通过 Kilo 官方全局指令文件加载；首次写入前保留原文件快照。",
        kiloActiveMemory: "AGENTS.md",
        piMode: "写入 ~/.pi/agent/AGENTS.md",
        piModeDetail: "通过 Pi 官方全局指令文件加载；首次写入前保留原文件快照。",
        piActiveMemory: "AGENTS.md",
        backups: "备份与还原",
        backupsDescription: "每次启用、停用和还原前都会自动创建当前工具的完整提示词快照。",
        backupsEmpty: "当前工具还没有提示词备份。",
        backupFiles: (count: number) => `${count} 个文件`,
        backupModeAppend: "保留模式",
        backupModeReplace: "替换模式",
        restore: "还原",
        restoring: "还原中...",
        restoreConfirmTitle: "确认还原这个快照？",
        restoreConfirmDescription: "当前状态会先自动备份，因此本次还原也可以撤销。",
        cancel: "取消",
        close: "关闭",
        edit: "编辑",
        remove: "删除",
        formEyebrow: "CUSTOM PROMPT",
        addFormTitle: "添加提示词",
        editFormTitle: "编辑提示词",
        formDescription: "保存为 Markdown 文件，之后可在列表中单独启用。",
        back: "返回",
        promptDetails: "提示词详情",
        promptName: "提示词名称",
        promptNamePlaceholder: "例如：通用编程助手、代码审查专家",
        filename: "文件名",
        filenamePlaceholder: "my-prompt.md",
        content: "提示词内容",
        contentHint: "Markdown",
        contentPlaceholder: "在此输入提示词内容...",
        save: "保存",
      }
    : {
        eyebrow: "PROMPT INJECTION",
        title: "Manage instruction prompts",
        description: "Choose an activation method, then manage bundled, online, or custom Markdown prompts.",
        sync: "Sync GitHub templates",
        syncing: "Syncing...",
        importMd: "Import md",
        importing: "Importing...",
        add: "Add prompt",
        manageCategories: "Manage categories",
        emptyCategory: "No prompts in this category",
        currentStatus: "Current status",
        noActive: "No prompt enabled",
        inactiveByConfig: "Current config does not load it",
        inactiveByConfigDetail: (filename: string) => `DevConduit found ${filename}, but the current config.toml does not reference it. This can happen when CCSwitch switches to a profile without model_instructions_file.`,
        keepExisting: "Keep existing",
        replaceExisting: "Replace existing",
        appendMode: "Append to AGENTS.md",
        replaceMode: "Replace instruction file",
        appendDetail: "The current template is written to AGENTS.md while the existing instruction file is preserved.",
        replaceDetail: "The current template is loaded independently through model_instructions_file.",
        inactiveDetail: "Choose an activation method, then turn on a template below.",
        enableMethod: "Enable method",
        helpLabel: "Show activation method help",
        appendHelp: "Adds a DevConduit-managed block to AGENTS.md without changing the existing model_instructions_file.",
        replaceHelp: "Makes the selected template the only instruction entry and replaces the existing model_instructions_file.",
        pendingMode: (mode: string) => `The current mode is unchanged. The next enable uses “${mode}”.`,
        modeHint: "This method is used when a template is turned on.",
        keepTitle: "Write to AGENTS.md and preserve model_instructions_file",
        replaceTitle: "Replace the existing instruction file through model_instructions_file",
        enable: "Enable",
        disable: "Disable",
        disableExternal: "Disable external prompt",
        current: "Current",
        onlineRemoved: "Removed online",
        removedDescription: "This template was removed online but is still active.",
        customDescription: "Custom instruction prompt",
        preservedDescription: "Existing user prompt preserved by append mode.",
        existingPrompt: "Existing user prompt",
        engineCodex: "Codex",
        engineClaude: "Claude Code",
        engineZcode: "ZCode",
        engineGrok: "Grok Build",
        engineKilo: "Kilo Code",
        enginePi: "Pi",
        engineSwitchLabel: "Instruction engine",
        claudeMode: "Write import block to ~/.claude/CLAUDE.md",
        claudeModeDetail: "The current template is written to ~/.claude/keysmith/ and a managed import block is injected into CLAUDE.md.",
        claudeActiveMemory: "CLAUDE.md",
        zcodeMode: "Write ~/.zcode-keysmith system-role + wrapper",
        zcodeModeDetail: "The current template is written to system-role.md and injected into ZCode runtime system prompt via wrapper.",
        zcodeActiveMemory: "system-role.md",
        grokMode: "Write ~/.grok/AGENTS.md + compat isolation",
        grokModeDetail: "The current template is written to AGENTS.md, compat isolation block is injected and hooks are isolated.",
        grokActiveMemory: "AGENTS.md",
        kiloMode: "Write ~/.config/kilo/AGENTS.md",
        kiloModeDetail: "Uses Kilo's official global instruction file and preserves the original file before the first write.",
        kiloActiveMemory: "AGENTS.md",
        piMode: "Write ~/.pi/agent/AGENTS.md",
        piModeDetail: "Uses Pi's official global instruction file and preserves the original file before the first write.",
        piActiveMemory: "AGENTS.md",
        backups: "Backups & restore",
        backupsDescription: "A complete prompt snapshot for the current tool is created before every enable, disable, and restore.",
        backupsEmpty: "No prompt backups for this tool yet.",
        backupFiles: (count: number) => `${count} file${count === 1 ? "" : "s"}`,
        backupModeAppend: "Keep mode",
        backupModeReplace: "Replace mode",
        restore: "Restore",
        restoring: "Restoring...",
        restoreConfirmTitle: "Restore this snapshot?",
        restoreConfirmDescription: "The current state is backed up first, so this restore can also be undone.",
        cancel: "Cancel",
        close: "Close",
        edit: "Edit",
        remove: "Delete",
        formEyebrow: "CUSTOM PROMPT",
        addFormTitle: "Add prompt",
        editFormTitle: "Edit prompt",
        formDescription: "Save it as Markdown, then enable it separately from the list.",
        back: "Back",
        promptDetails: "Prompt details",
        promptName: "Prompt name",
        promptNamePlaceholder: "For example: General coding assistant",
        filename: "Filename",
        filenamePlaceholder: "my-prompt.md",
        content: "Prompt content",
        contentHint: "Markdown",
        contentPlaceholder: "Enter prompt content here...",
        save: "Save",
      };
}

type EngineModeCopy = {
  label: string;
  detail: string;
  help: string;
  title: string;
};

function getEngineModeCopy(
  lang: Lang,
  engine: PromptEngine,
  mode: PromptInjectionMode,
): EngineModeCopy {
  const isAppend = mode === "append";
  if (lang === "zh") {
    if (engine === "claude") {
      return isAppend
        ? {
          label: "保留 CLAUDE.md",
          detail: "在现有 CLAUDE.md 后加入受管 import 区块，原有内容继续生效。",
          help: "保留 CLAUDE.md 原内容，只增加 DevConduit 管理的 import 区块。",
          title: "保留现有 CLAUDE.md，并追加当前提示词 import",
        }
        : {
          label: "替换 CLAUDE.md",
          detail: "CLAUDE.md 仅保留当前模板的受管 import，替换前内容已完整备份。",
          help: "当前模板会成为 CLAUDE.md 的唯一指令入口；替换前会自动创建可还原快照。",
          title: "用当前提示词 import 替换 CLAUDE.md",
        };
    }
    if (engine === "zcode") {
      return isAppend
        ? {
          label: "合并 system prompt",
          detail: "运行时依次加载 ZCode 原 system prompt 与当前模板。",
          help: "保留 ZCode 自带或用户设置的 system prompt，并在其后合并当前模板。",
          title: "保留 ZCode 原 system prompt，并合并当前模板",
        }
        : {
          label: "替换 system prompt",
          detail: "运行时优先使用当前模板，ZCode 原 system prompt 不会从其配置中删除。",
          help: "当前模板会成为运行时 system prompt；原配置保持不动，注入状态可从备份还原。",
          title: "在 ZCode 运行时使用当前模板替换 system prompt",
        };
    }
    if (engine === "grok") {
      return isAppend
        ? {
          label: "追加到 AGENTS.md",
          detail: "只在现有 AGENTS.md 中增加受管区块，原有规则继续生效。",
          help: "保留 Grok 的 AGENTS.md，只追加 DevConduit 管理的提示词区块。",
          title: "保留 Grok AGENTS.md，并追加当前提示词",
        }
        : {
          label: "替换 AGENTS.md",
          detail: "当前模板成为完整 AGENTS.md，替换前文件已完整备份。",
          help: "用当前模板替换 Grok AGENTS.md；替换前会自动创建可还原快照。",
          title: "用当前模板替换 Grok AGENTS.md",
        };
    }
    if (engine === "kilo") {
      return isAppend
        ? {
          label: "追加到 AGENTS.md",
          detail: "只在 Kilo 全局 AGENTS.md 中增加受管区块，原有规则继续生效。",
          help: "保留 Kilo 的全局 AGENTS.md，只追加 DevConduit 管理的提示词区块。",
          title: "保留 Kilo AGENTS.md，并追加当前提示词",
        }
        : {
          label: "替换 AGENTS.md",
          detail: "当前模板成为完整的 Kilo 全局 AGENTS.md，原文件已保存为可恢复快照。",
          help: "用当前模板替换 Kilo 全局 AGENTS.md；首次替换前会完整保留原文件。",
          title: "用当前模板替换 Kilo AGENTS.md",
        };
    }
    if (engine === "pi") {
      return isAppend
        ? {
          label: "追加到 AGENTS.md",
          detail: "只在 Pi 全局 AGENTS.md 中增加受管区块，原有规则继续生效。",
          help: "保留 Pi 的全局 AGENTS.md，只追加 DevConduit 管理的提示词区块。",
          title: "保留 Pi AGENTS.md，并追加当前提示词",
        }
        : {
          label: "替换 AGENTS.md",
          detail: "当前模板成为完整的 Pi 全局 AGENTS.md，原文件已保存为可恢复快照。",
          help: "用当前模板替换 Pi 全局 AGENTS.md；首次替换前会完整保留原文件。",
          title: "用当前模板替换 Pi AGENTS.md",
        };
    }
    return isAppend
      ? {
        label: "追加到 AGENTS.md",
        detail: "当前模板写入 AGENTS.md，同时保留已有指令文件。",
        help: "只在 AGENTS.md 中增加 DevConduit 管理区块，不改动原有 model_instructions_file。",
        title: "写入 AGENTS.md，并保留现有 model_instructions_file",
      }
      : {
        label: "替换指令文件",
        detail: "当前模板通过 model_instructions_file 独立加载。",
        help: "当前模板会成为唯一生效的指令入口，原有 model_instructions_file 将被替换。",
        title: "使用 model_instructions_file 替换现有指令文件",
      };
  }

  if (engine === "claude") {
    return isAppend
      ? {
        label: "Keep CLAUDE.md",
        detail: "A managed import block is appended while the existing CLAUDE.md remains active.",
        help: "Keeps the existing CLAUDE.md and adds only the DevConduit-managed import block.",
        title: "Keep CLAUDE.md and append the current prompt import",
      }
      : {
        label: "Replace CLAUDE.md",
        detail: "CLAUDE.md contains only the managed import; the previous content is fully backed up.",
        help: "Makes the current template the only CLAUDE.md instruction entry after creating a restorable snapshot.",
        title: "Replace CLAUDE.md with the current prompt import",
      };
  }
  if (engine === "zcode") {
    return isAppend
      ? {
        label: "Merge system prompt",
        detail: "The ZCode system prompt and the current template are loaded in sequence at runtime.",
        help: "Keeps ZCode's native or user system prompt and merges the current template after it.",
        title: "Keep the ZCode system prompt and merge the current template",
      }
      : {
        label: "Replace system prompt",
        detail: "The current template takes priority at runtime without deleting ZCode's stored prompt.",
        help: "Uses the current template as the runtime system prompt; the injection state can be restored from a backup.",
        title: "Replace the ZCode runtime system prompt with this template",
      };
  }
  if (engine === "grok") {
    return isAppend
      ? {
        label: "Append to AGENTS.md",
        detail: "Only a managed block is added to AGENTS.md, so existing rules remain active.",
        help: "Keeps Grok's AGENTS.md and appends a DevConduit-managed prompt block.",
        title: "Keep Grok AGENTS.md and append the current prompt",
      }
      : {
        label: "Replace AGENTS.md",
        detail: "The current template becomes the complete AGENTS.md after the original file is backed up.",
        help: "Replaces Grok AGENTS.md with this template after creating a restorable snapshot.",
        title: "Replace Grok AGENTS.md with the current template",
      };
  }
  if (engine === "kilo") {
    return isAppend
      ? {
        label: "Append to AGENTS.md",
        detail: "Only a managed block is added to Kilo's global AGENTS.md, preserving existing rules.",
        help: "Keeps Kilo's global AGENTS.md and appends a DevConduit-managed prompt block.",
        title: "Keep Kilo AGENTS.md and append the current prompt",
      }
      : {
        label: "Replace AGENTS.md",
        detail: "The current template becomes Kilo's complete global AGENTS.md after preserving the original file.",
        help: "Replaces Kilo's global AGENTS.md after keeping an exact restorable snapshot.",
        title: "Replace Kilo AGENTS.md with the current template",
      };
  }
  if (engine === "pi") {
    return isAppend
      ? {
        label: "Append to AGENTS.md",
        detail: "Only a managed block is added to Pi's global AGENTS.md, preserving existing rules.",
        help: "Keeps Pi's global AGENTS.md and appends a DevConduit-managed prompt block.",
        title: "Keep Pi AGENTS.md and append the current prompt",
      }
      : {
        label: "Replace AGENTS.md",
        detail: "The current template becomes Pi's complete global AGENTS.md after preserving the original file.",
        help: "Replaces Pi's global AGENTS.md after keeping an exact restorable snapshot.",
        title: "Replace Pi AGENTS.md with the current template",
      };
  }
  return isAppend
    ? {
      label: "Append to AGENTS.md",
      detail: "The current template is written to AGENTS.md while the existing instruction file is preserved.",
      help: "Adds a DevConduit-managed block to AGENTS.md without changing model_instructions_file.",
      title: "Write AGENTS.md and preserve model_instructions_file",
    }
    : {
      label: "Replace instruction file",
      detail: "The current template is loaded independently through model_instructions_file.",
      help: "Makes the current template the only instruction entry by replacing model_instructions_file.",
      title: "Replace model_instructions_file with the current template",
    };
}

function backupActionLabel(action: string, lang: Lang) {
  if (action === "before-restore") return lang === "zh" ? "还原前自动快照" : "Automatic pre-restore snapshot";
  if (action.includes("disable") || action.includes("uninstall")) {
    return lang === "zh" ? "停用前自动快照" : "Automatic pre-disable snapshot";
  }
  if (action.includes("restore-grok-hooks")) {
    return lang === "zh" ? "恢复 Hooks 前快照" : "Pre-hook-restore snapshot";
  }
  return lang === "zh" ? "启用前自动快照" : "Automatic pre-enable snapshot";
}

function run(action: MaybeAsyncAction) {
  void action();
}

function getRuntimeCopy(lang: Lang) {
  return lang === "zh"
    ? {
      deployment: "部署状态",
      preview: "预览变更",
      active: "已部署",
      inactive: "未部署",
      attention: "需要处理",
      unsupported: "当前平台不支持",
      nativeCodex: "原生配置入口",
      nativeClaude: "原生 CLAUDE.md 入口",
      nativeZcode: "运行时 launcher 入口",
      nativeGrok: "全局规则入口",
      nativeKilo: "Kilo 全局指令入口",
      nativePi: "Pi 全局指令入口",
      cliRuntime: "Claude CLI runtime",
      cliRuntimeDetail: "在新终端中追加当前 DevConduit 指令",
      cliRuntimeUnavailable: "当前平台只支持 macOS zsh 或 Windows PowerShell。",
      cliRuntimeNeedsPrompt: "先启用一个 Claude 指令后才能安装。",
      previewTitle: "部署变更预览",
      previewDescription: "确认前不会写入任何文件。确认后会先创建可恢复快照。",
      files: "将处理的文件",
      backup: "恢复快照位置",
      restart: "完成后需要重新打开对应终端或应用。",
      confirm: "我确认先创建恢复快照，再应用这些文件变更。",
      install: "确认安装",
      uninstall: "确认卸载",
      close: "关闭",
      cancel: "取消",
      operationInstall: "写入或更新",
      operationUninstall: "移除或恢复",
      nativeDetail: "使用该工具自身支持的原生配置方式，不接管全局命令。",
      claudeDetail: "使用受管 profile 区块调用原始 claude 命令，不修改 Claude 二进制或设置。",
    }
    : {
      deployment: "Deployment status",
      preview: "Preview changes",
      active: "Deployed",
      inactive: "Not deployed",
      attention: "Needs attention",
      unsupported: "Unsupported platform",
      nativeCodex: "Native config entry",
      nativeClaude: "Native CLAUDE.md entry",
      nativeZcode: "Runtime launcher entry",
      nativeGrok: "Global rules entry",
      nativeKilo: "Kilo global instruction entry",
      nativePi: "Pi global instruction entry",
      cliRuntime: "Claude CLI runtime",
      cliRuntimeDetail: "Append the current DevConduit instruction in new terminals",
      cliRuntimeUnavailable: "Only macOS zsh and Windows PowerShell are supported on this platform.",
      cliRuntimeNeedsPrompt: "Enable a Claude instruction before installing it.",
      previewTitle: "Deployment change preview",
      previewDescription: "No files are written until confirmation. A restorable snapshot is created first.",
      files: "Files to change",
      backup: "Recovery snapshot location",
      restart: "Reopen the related terminal or application after completion.",
      confirm: "I understand that a recovery snapshot is created before these changes are applied.",
      install: "Install",
      uninstall: "Uninstall",
      close: "Close",
      cancel: "Cancel",
      operationInstall: "Write or update",
      operationUninstall: "Remove or restore",
      nativeDetail: "Uses the tool's own supported configuration path without replacing a global command.",
      claudeDetail: "Uses a managed profile block to invoke the original claude command without modifying Claude binaries or settings.",
    };
}

type PendingRuntimePreview = {
  preview: PromptRuntimePreview;
  apply?: MaybeAsyncAction;
  claudeCliRuntime: boolean;
};

type RuntimePreviewDialogProps = {
  lang: Lang;
  pending: PendingRuntimePreview | null;
  busy: boolean;
  onClose: () => void;
  onApply: () => void | Promise<void>;
};

function RuntimePreviewDialog({ lang, pending, busy, onClose, onApply }: RuntimePreviewDialogProps) {
  const copy = getRuntimeCopy(lang);
  const [confirmed, setConfirmed] = useState(false);

  useEffect(() => {
    setConfirmed(false);
  }, [pending?.preview.engine, pending?.preview.operation, pending?.claudeCliRuntime]);

  if (!pending) return null;
  const { preview } = pending;
  const isInstall = preview.operation === "install";

  return (
    <ModalShell
      open
      onClose={onClose}
      title={<span className="cx-prompt-runtime-dialog-title"><Eye size={19} aria-hidden="true" />{copy.previewTitle}</span>}
      description={copy.previewDescription}
      closeLabel={copy.close}
      closeOnBackdrop={!busy}
      closeOnEscape={!busy}
      showCloseButton={!busy}
      size="lg"
      className="cx-prompt-runtime-dialog"
      bodyClassName="cx-prompt-runtime-dialog-body"
      footer={pending.apply ? (
        <>
          <Button variant="secondary" onClick={onClose} disabled={busy}>{copy.cancel}</Button>
          <Button
            icon={busy ? <Loader2 className="cx-prompts-spin" /> : <ShieldCheck />}
            onClick={() => void onApply()}
            disabled={busy || !confirmed}
            data-initial-focus
          >
            {isInstall ? copy.install : copy.uninstall}
          </Button>
        </>
      ) : (
        <Button variant="secondary" onClick={onClose}>{copy.close}</Button>
      )}
    >
      <div className="cx-prompt-runtime-preview-copy">
        <strong>{pending.claudeCliRuntime ? copy.claudeDetail : copy.nativeDetail}</strong>
        {preview.restartHint && <span>{copy.restart}</span>}
      </div>
      <section className="cx-prompt-runtime-preview-files" aria-label={copy.files}>
        <strong>{copy.files}</strong>
        <div>
          {preview.targets.map((target) => (
            <div className="cx-prompt-runtime-preview-file" key={`${target.path}:${target.operation}`}>
              <span>{target.label}</span>
              <code title={target.path}>{target.path}</code>
              <StatusBadge tone={isInstall ? "info" : "warning"} dot={false}>
                {isInstall ? copy.operationInstall : copy.operationUninstall}
              </StatusBadge>
            </div>
          ))}
        </div>
      </section>
      <section className="cx-prompt-runtime-preview-backup" aria-label={copy.backup}>
        <ShieldCheck size={18} aria-hidden="true" />
        <div>
          <strong>{copy.backup}</strong>
          <code title={preview.backupLocation}>{preview.backupLocation}</code>
        </div>
      </section>
      {pending.apply && (
        <Checkbox
          checked={confirmed}
          onCheckedChange={setConfirmed}
          label={copy.confirm}
          disabled={busy}
          className="cx-prompt-runtime-confirm"
        />
      )}
    </ModalShell>
  );
}

type RuntimeDeploymentPanelProps = {
  lang: Lang;
  engine: PromptEngine;
  instructionEnabled: boolean;
  codexRuntimeEntryPath?: string;
  claudeRuntimeEntryPath?: string;
  claudeRuntime?: ClaudeRuntimeState;
  zcodeRuntime?: ZcodeState | null;
  grokRuntime?: GrokState | null;
  kiloRuntime?: KiloState | null;
  piRuntime?: PiState | null;
  loading: boolean;
  previewLoading: boolean;
  onPreview: () => void;
  onToggleClaudeRuntime: () => void;
};

function RuntimeDeploymentPanel({
  lang,
  engine,
  instructionEnabled,
  codexRuntimeEntryPath,
  claudeRuntimeEntryPath,
  claudeRuntime,
  zcodeRuntime,
  grokRuntime,
  kiloRuntime,
  piRuntime,
  loading,
  previewLoading,
  onPreview,
  onToggleClaudeRuntime,
}: RuntimeDeploymentPanelProps) {
  const copy = getRuntimeCopy(lang);
  const native = engine === "codex"
    ? { title: copy.nativeCodex, path: codexRuntimeEntryPath, active: instructionEnabled, attention: false }
    : engine === "claude"
      ? { title: copy.nativeClaude, path: claudeRuntimeEntryPath, active: instructionEnabled, attention: false }
      : engine === "zcode"
        ? {
          title: copy.nativeZcode,
          path: zcodeRuntime?.systemFile,
          active: instructionEnabled,
          attention: Boolean(zcodeRuntime?.zcodeApp && !zcodeRuntime.runtimePatchable),
        }
        : engine === "grok"
          ? {
          title: copy.nativeGrok,
          path: grokRuntime?.grokDir,
          active: instructionEnabled,
          attention: Boolean(grokRuntime?.grokDirExists && !grokRuntime.configTomlExists),
          }
          : engine === "kilo"
            ? {
              title: copy.nativeKilo,
              path: kiloRuntime?.agentsPath,
              active: instructionEnabled,
              attention: Boolean(kiloRuntime?.manifestExists && !kiloRuntime.agentsMdExists),
            }
            : {
              title: copy.nativePi,
              path: piRuntime?.agentsPath,
              active: instructionEnabled,
              attention: Boolean(piRuntime?.manifestExists && !piRuntime.agentsMdExists),
            };
  const nativeTone = native.attention ? "warning" : native.active ? "success" : "neutral";
  const nativeLabel = native.attention ? copy.attention : native.active ? copy.active : copy.inactive;
  const runtimeStatus = claudeRuntime?.status;
  const claudeRuntimeTone = runtimeStatus === "active"
    ? "success"
    : runtimeStatus === "partial" || runtimeStatus === "needs-repair"
      ? "warning"
      : "neutral";
  const claudeRuntimeLabel = runtimeStatus === "active"
    ? copy.active
    : runtimeStatus === "partial" || runtimeStatus === "needs-repair"
      ? copy.attention
      : runtimeStatus === "unsupported"
        ? copy.unsupported
        : copy.inactive;

  return (
    <section className="cx-prompt-runtime-panel" aria-label={copy.deployment}>
      <div className="cx-prompt-runtime-native">
        <div className="cx-prompt-runtime-heading">
          <Terminal size={17} aria-hidden="true" />
          <div>
            <strong>{copy.deployment}</strong>
            <span>{native.title}</span>
          </div>
        </div>
        {native.path && <code title={native.path}>{native.path}</code>}
        <div className="cx-prompt-runtime-native-actions">
          <StatusBadge tone={nativeTone} dot={false}>{nativeLabel}</StatusBadge>
          <IconButton
            icon={previewLoading ? <Loader2 className="cx-prompts-spin" /> : <Eye size={16} />}
            label={copy.preview}
            variant="ghost"
            size="sm"
            onClick={onPreview}
            disabled={loading || previewLoading}
          />
        </div>
      </div>
      {engine === "claude" && claudeRuntime && (
        <div className="cx-prompt-runtime-cli">
          <div className="cx-prompt-runtime-cli-copy">
            <div>
              <strong>{copy.cliRuntime}</strong>
              <StatusBadge tone={claudeRuntimeTone} dot={false}>{claudeRuntimeLabel}</StatusBadge>
            </div>
            <span>{claudeRuntime.supported ? copy.cliRuntimeDetail : copy.cliRuntimeUnavailable}</span>
            {!instructionEnabled && claudeRuntime.supported && <span>{copy.cliRuntimeNeedsPrompt}</span>}
          </div>
          <Toggle
            checked={claudeRuntime.active}
            onCheckedChange={() => onToggleClaudeRuntime()}
            disabled={loading || previewLoading || (!instructionEnabled && !claudeRuntime.active) || !claudeRuntime.supported}
            aria-label={copy.cliRuntime}
          />
        </div>
      )}
    </section>
  );
}

function promptCategoryKey(engine: PromptEngine, kind: "builtin" | "saved" | "external", id: string) {
  return `${engine}:${kind}:${id.trim().toLowerCase()}`;
}

function savedPromptCategoryKey(engine: PromptEngine, prompt: SavedPrompt) {
  return prompt.id.startsWith("external-")
    ? promptCategoryKey(engine, "external", prompt.filename)
    : promptCategoryKey(engine, "saved", prompt.id);
}

type PromptRowProps = {
  title: string;
  description: string;
  enabled: boolean;
  loading: boolean;
  toggleLabel: string;
  onToggle: () => void | Promise<void>;
  children?: React.ReactNode;
  actions?: React.ReactNode;
};

function PromptRow({
  title,
  description,
  enabled,
  loading,
  toggleLabel,
  onToggle,
  children,
  actions,
}: PromptRowProps) {
  return (
    <article className="cx-prompts-row">
      <div className="cx-prompts-row-head">
        <div className="cx-prompts-row-heading">
          <div className="cx-prompts-row-icon" aria-hidden="true"><FileText size={16} strokeWidth={1.9} /></div>
          <div className="cx-prompts-row-title" title={title}><strong>{title}</strong></div>
        </div>
        <div className="cx-prompts-row-actions">
          <Toggle
            checked={enabled}
            onCheckedChange={() => void onToggle()}
            disabled={loading}
            aria-label={toggleLabel}
          />
        </div>
      </div>
      <div className="cx-prompts-row-copy">
        <p title={description}>{description}</p>
      </div>
      {(children || actions) && (
        <div className="cx-prompts-row-footer">
          <div className="cx-prompts-row-details">{children}</div>
          {actions}
        </div>
      )}
    </article>
  );
}

function PromptFormView({
  lang,
  promptForm,
  editingPromptId,
  loading,
  onInstructionModeChange,
  onPromptFormFieldChange,
  onSavePrompt,
}: Pick<
  PromptsPageProps,
  | "lang"
  | "promptForm"
  | "editingPromptId"
  | "loading"
  | "onInstructionModeChange"
  | "onPromptFormFieldChange"
  | "onSavePrompt"
>) {
  const copy = getCopy(lang);
  const titleId = useId();
  const filenameId = useId();
  const contentId = useId();

  return (
    <div className="cx-prompts-form-page">
      <header className="cx-prompts-header cx-prompts-form-header">
        <div className="cx-prompts-heading">
          <p><PencilLine size={14} aria-hidden="true" />{copy.formEyebrow}</p>
          <h2>{editingPromptId ? copy.editFormTitle : copy.addFormTitle}</h2>
          <span>{copy.formDescription}</span>
        </div>
        <Button
          variant="secondary"
          icon={<ArrowLeft />}
          onClick={() => onInstructionModeChange("list")}
          disabled={loading}
        >
          {copy.back}
        </Button>
      </header>

      <section className="cx-prompts-form-panel" aria-labelledby={`${titleId}-panel`}>
        <div className="cx-prompts-form-panel-head">
          <FileText size={18} aria-hidden="true" />
          <h3 id={`${titleId}-panel`}>{copy.promptDetails}</h3>
        </div>
        <div className="cx-prompts-form-grid">
          <label className="cx-prompts-field" htmlFor={titleId}>
            <span>{copy.promptName}</span>
            <input
              id={titleId}
              type="text"
              value={promptForm.title}
              onChange={(event) => onPromptFormFieldChange("title", event.currentTarget.value)}
              placeholder={copy.promptNamePlaceholder}
              disabled={loading}
              autoComplete="off"
            />
          </label>
          <label className="cx-prompts-field" htmlFor={filenameId}>
            <span>{copy.filename}</span>
            <input
              id={filenameId}
              type="text"
              value={promptForm.filename}
              onChange={(event) => onPromptFormFieldChange("filename", event.currentTarget.value)}
              placeholder={copy.filenamePlaceholder}
              disabled={loading}
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <label className="cx-prompts-field cx-prompts-field--content" htmlFor={contentId}>
            <span>{copy.content}<small>{copy.contentHint}</small></span>
            <textarea
              id={contentId}
              value={promptForm.content}
              onChange={(event) => onPromptFormFieldChange("content", event.currentTarget.value)}
              placeholder={copy.contentPlaceholder}
              disabled={loading}
              spellCheck={false}
            />
          </label>
        </div>
      </section>

      <div className="cx-prompts-form-actions">
        <Button
          size="lg"
          icon={loading ? <Loader2 className="cx-prompts-spin" /> : <Save />}
          onClick={() => run(onSavePrompt)}
          disabled={loading}
        >
          {copy.save}
        </Button>
      </div>
    </div>
  );
}

type PromptBackupsDialogProps = {
  lang: Lang;
  engine: PromptEngine;
  open: boolean;
  loading: boolean;
  restoringId: string;
  backups: PromptBackupEntry[];
  onClose: () => void;
  onRestore: (id: string) => void | Promise<void>;
};

function PromptBackupsDialog({
  lang,
  engine,
  open,
  loading,
  restoringId,
  backups,
  onClose,
  onRestore,
}: PromptBackupsDialogProps) {
  const copy = getCopy(lang);
  const [candidateId, setCandidateId] = useState("");
  const candidate = backups.find((entry) => entry.id === candidateId);
  const busy = Boolean(restoringId);

  useEffect(() => {
    if (!open) setCandidateId("");
  }, [engine, open]);

  const formatDate = (value: string) => {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en", {
      dateStyle: "medium",
      timeStyle: "medium",
    }).format(date);
  };

  const restoreCandidate = async () => {
    if (!candidate) return;
    await onRestore(candidate.id);
    setCandidateId("");
  };

  return (
    <ModalShell
      open={open}
      onClose={onClose}
      title={<span className="cx-prompt-backups-title"><History size={19} aria-hidden="true" />{copy.backups}</span>}
      description={copy.backupsDescription}
      closeLabel={copy.close}
      closeOnBackdrop={!busy}
      closeOnEscape={!busy}
      showCloseButton={!busy}
      size="lg"
      className="cx-prompt-backups-dialog"
      bodyClassName="cx-prompt-backups-body"
      footer={candidate ? (
        <>
          <Button variant="secondary" onClick={() => setCandidateId("")} disabled={busy}>
            {copy.cancel}
          </Button>
          <Button
            icon={busy ? <Loader2 className="cx-prompts-spin" /> : <RotateCcw />}
            onClick={() => void restoreCandidate()}
            disabled={busy}
            data-initial-focus
          >
            {busy ? copy.restoring : copy.restore}
          </Button>
        </>
      ) : (
        <Button variant="secondary" onClick={onClose} disabled={busy}>{copy.close}</Button>
      )}
    >
      {candidate && (
        <div className="cx-prompt-backups-confirm" role="status">
          <strong>{copy.restoreConfirmTitle}</strong>
          <span>{copy.restoreConfirmDescription}</span>
        </div>
      )}
      {loading ? (
        <div className="cx-prompt-backups-empty">
          <Loader2 className="cx-prompts-spin" aria-hidden="true" />
        </div>
      ) : backups.length === 0 ? (
        <div className="cx-prompt-backups-empty">{copy.backupsEmpty}</div>
      ) : (
        <div className="cx-prompt-backups-list">
          {backups.map((entry) => (
            <div
              className={cx("cx-prompt-backup-row", candidateId === entry.id && "cx-prompt-backup-row--selected")}
              key={entry.id}
            >
              <div className="cx-prompt-backup-copy">
                <div>
                  <strong>{backupActionLabel(entry.action, lang)}</strong>
                  {entry.injectionMode && (
                    <StatusBadge tone="neutral" dot={false}>
                      {entry.injectionMode === "append" ? copy.backupModeAppend : copy.backupModeReplace}
                    </StatusBadge>
                  )}
                </div>
                <span>{formatDate(entry.createdAt)} · {copy.backupFiles(entry.fileCount)}</span>
              </div>
              <Button
                variant="secondary"
                size="sm"
                icon={restoringId === entry.id ? <Loader2 className="cx-prompts-spin" /> : <RotateCcw />}
                onClick={() => setCandidateId(entry.id)}
                disabled={busy}
              >
                {restoringId === entry.id ? copy.restoring : copy.restore}
              </Button>
            </div>
          ))}
        </div>
      )}
    </ModalShell>
  );
}

export function PromptsPage({
  lang,
  instructionMode,
  promptForm,
  editingPromptId,
  loading,
  actionBusy,
  promptSyncing,
  promptCatalogReady,
  promptImportRef,
  promptInjectionMode,
  promptModeHelpOpen,
  promptModeHelpRef,
  promptEngine,
  onPromptEngineChange,
  instructionEnabled,
  codexInstructionStatus,
  codexInactiveInstructionFile,
  activeInstructionTitle,
  activeInjectionMode,
  codexRuntimeScope,
  codexRuntimeEntryPath,
  claudeRuntimeEntryPath,
  claudeRuntime,
  zcodeRuntime,
  grokRuntime,
  kiloRuntime,
  piRuntime,
  promptBackups,
  promptBackupsOpen,
  promptBackupsLoading,
  promptRestoreBusyId,
  instructionTemplates,
  builtinPromptStatuses,
  activeBuiltinTemplateId,
  orphanedBuiltinPrompt,
  savedPrompts,
  managedSavedPromptId,
  preservedSavedPromptFilename,
  externalPrompt,
  className,
  onSyncBuiltinPrompts,
  onImportPrompt,
  onAddPrompt,
  onInstructionModeChange,
  onPromptInjectionModeChange,
  onTogglePromptModeHelp,
  onOpenPromptBackups,
  onClosePromptBackups,
  onRestorePromptBackup,
  onEnableBuiltinPrompt,
  onDisableInstruction,
  onEnableSavedPrompt,
  onDisableExternalPrompt,
  onEditPrompt,
  onDeletePrompt,
  onPromptFormFieldChange,
  onSavePrompt,
  claudeInstructionEnabled,
  claudeActiveInstructionTitle,
  claudeInstructionTemplates,
  claudeBuiltinPromptStatuses,
  claudeActiveBuiltinTemplateId,
  claudeSavedPrompts,
  claudeManagedSavedPromptId,
  onEnableClaudeBuiltinPrompt,
  onDisableClaudeInstruction,
  onEnableClaudeSavedPrompt,
  onEditClaudePrompt,
  onDeleteClaudePrompt,
  onImportClaudePrompt,
  onSaveClaudePrompt,
  onInstallClaudeRuntime,
  onUninstallClaudeRuntime,
  zcodeInstructionEnabled,
  zcodeActiveInstructionTitle,
  zcodeInstructionTemplates,
  zcodeBuiltinPromptStatuses,
  zcodeActiveBuiltinTemplateId,
  zcodeSavedPrompts,
  zcodeManagedSavedPromptId,
  onEnableZcodeBuiltinPrompt,
  onDisableZcodeInstruction,
  onEnableZcodeSavedPrompt,
  onEditZcodePrompt,
  onDeleteZcodePrompt,
  onImportZcodePrompt,
  onSaveZcodePrompt,
  grokInstructionEnabled,
  grokActiveInstructionTitle,
  grokInstructionTemplates,
  grokBuiltinPromptStatuses,
  grokActiveBuiltinTemplateId,
  grokSavedPrompts,
  grokManagedSavedPromptId,
  onEnableGrokBuiltinPrompt,
  onDisableGrokInstruction,
  onEnableGrokSavedPrompt,
  onEditGrokPrompt,
  onDeleteGrokPrompt,
  onImportGrokPrompt,
  onSaveGrokPrompt,
  kiloInstructionEnabled,
  kiloActiveInstructionTitle,
  kiloInstructionTemplates,
  kiloBuiltinPromptStatuses,
  kiloActiveBuiltinTemplateId,
  kiloSavedPrompts,
  kiloManagedSavedPromptId,
  onEnableKiloBuiltinPrompt,
  onDisableKiloInstruction,
  onEnableKiloSavedPrompt,
  onEditKiloPrompt,
  onDeleteKiloPrompt,
  onImportKiloPrompt,
  onSaveKiloPrompt,
  piInstructionEnabled,
  piActiveInstructionTitle,
  piInstructionTemplates,
  piBuiltinPromptStatuses,
  piActiveBuiltinTemplateId,
  piSavedPrompts,
  piManagedSavedPromptId,
  onEnablePiBuiltinPrompt,
  onDisablePiInstruction,
  onEnablePiSavedPrompt,
  onEditPiPrompt,
  onDeletePiPrompt,
  onImportPiPrompt,
  onSavePiPrompt,
}: PromptsPageProps) {
  const copy = getCopy(lang);
  const helpId = useId();
  const isClaude = promptEngine === "claude";
  const isZcode = promptEngine === "zcode";
  const isGrok = promptEngine === "grok";
  const isKilo = promptEngine === "kilo";
  const isPi = promptEngine === "pi";
  const isCodex = promptEngine === "codex";
  const appendModeCopy = getEngineModeCopy(lang, promptEngine, "append");
  const replaceModeCopy = getEngineModeCopy(lang, promptEngine, "replace");
  const activeModeCopy = getEngineModeCopy(
    lang,
    promptEngine,
    activeInjectionMode || promptInjectionMode,
  );
  const selectedModeLabel = promptInjectionMode === "append" ? copy.keepExisting : copy.replaceExisting;
  const activeModeLabel = activeModeCopy.label;
  const importBusy = actionBusy === "importPrompt";
  const [categoryManagerOpen, setCategoryManagerOpen] = useState(false);
  const [pendingRuntimePreview, setPendingRuntimePreview] = useState<PendingRuntimePreview | null>(null);
  const [runtimePreviewLoading, setRuntimePreviewLoading] = useState(false);
  const [runtimePreviewBusy, setRuntimePreviewBusy] = useState(false);
  const [runtimePreviewError, setRuntimePreviewError] = useState("");
  const promptCategories = usePromptCategories(lang);
  // All engines share the same list interactions while keeping native backend semantics.
  const activeTemplates = isPi ? piInstructionTemplates : isKilo ? kiloInstructionTemplates : isGrok ? grokInstructionTemplates : isZcode ? zcodeInstructionTemplates : isClaude ? claudeInstructionTemplates : instructionTemplates;
  const activeBuiltinStatuses = isPi ? piBuiltinPromptStatuses : isKilo ? kiloBuiltinPromptStatuses : isGrok ? grokBuiltinPromptStatuses : isZcode ? zcodeBuiltinPromptStatuses : isClaude ? claudeBuiltinPromptStatuses : builtinPromptStatuses;
  const activeBuiltinId = isPi
    ? piActiveBuiltinTemplateId
    : isKilo
      ? kiloActiveBuiltinTemplateId
    : isGrok
      ? grokActiveBuiltinTemplateId
    : isZcode
      ? zcodeActiveBuiltinTemplateId
      : isClaude
        ? claudeActiveBuiltinTemplateId
        : activeBuiltinTemplateId;
  const activeSavedPrompts = isPi ? piSavedPrompts : isKilo ? kiloSavedPrompts : isGrok ? grokSavedPrompts : isZcode ? zcodeSavedPrompts : isClaude ? claudeSavedPrompts : savedPrompts;
  const activeManagedSavedPromptId = isPi ? piManagedSavedPromptId : isKilo ? kiloManagedSavedPromptId : isGrok ? grokManagedSavedPromptId : isZcode ? zcodeManagedSavedPromptId : isClaude ? claudeManagedSavedPromptId : managedSavedPromptId;
  const activeInstructionEnabled = isPi ? piInstructionEnabled : isKilo ? kiloInstructionEnabled : isGrok ? grokInstructionEnabled : isZcode ? zcodeInstructionEnabled : isClaude ? claudeInstructionEnabled : instructionEnabled;
  const activeInstructionTitleResolved = isPi ? piActiveInstructionTitle : isKilo ? kiloActiveInstructionTitle : isGrok ? grokActiveInstructionTitle : isZcode ? zcodeActiveInstructionTitle : isClaude ? claudeActiveInstructionTitle : activeInstructionTitle;
  const codexInstructionInactive = isCodex && codexInstructionStatus === "inactive";
  const modePending = Boolean(activeInstructionEnabled && activeInjectionMode && activeInjectionMode !== promptInjectionMode);
  const categoryItems = useMemo<PromptCategoryItem[]>(() => [
    ...activeTemplates.map((template) => ({
      key: promptCategoryKey(promptEngine, "builtin", template.id),
      title: template.title,
    })),
    ...activeSavedPrompts.map((prompt) => ({
      key: savedPromptCategoryKey(promptEngine, prompt),
      title: prompt.title,
    })),
  ], [activeSavedPrompts, activeTemplates, promptEngine]);
  const promptIsVisible = (key: string) =>
    promptCategories.categoryForPrompt(key) === promptCategories.activeCategoryId;
  const visiblePromptCount = categoryItems.filter((item) => promptIsVisible(item.key)).length;
  const deleteSavedPrompt = async (prompt: SavedPrompt) => {
    const handler = isPi ? onDeletePiPrompt : isKilo ? onDeleteKiloPrompt : isGrok ? onDeleteGrokPrompt : isZcode ? onDeleteZcodePrompt : isClaude ? onDeleteClaudePrompt : onDeletePrompt;
    await handler(prompt.id);
    promptCategories.forgetPrompt(savedPromptCategoryKey(promptEngine, prompt));
  };

  if (instructionMode === "form") {
    return (
      <PageTransition pageKey={`prompts:${instructionMode}`}>
        <section className={cx("cx-prompts-page", "cx-prompts-page--form", className)} aria-label={editingPromptId ? copy.editFormTitle : copy.addFormTitle}>
          <PromptFormView
            lang={lang}
            promptForm={promptForm}
            editingPromptId={editingPromptId}
            loading={loading}
            onInstructionModeChange={onInstructionModeChange}
            onPromptFormFieldChange={onPromptFormFieldChange}
            onSavePrompt={isPi ? onSavePiPrompt : isKilo ? onSaveKiloPrompt : isGrok ? onSaveGrokPrompt : isZcode ? onSaveZcodePrompt : isClaude ? onSaveClaudePrompt : onSavePrompt}
          />
        </section>
      </PageTransition>
    );
  }

  const handlePromptFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const handler = isPi ? onImportPiPrompt : isKilo ? onImportKiloPrompt : isGrok ? onImportGrokPrompt : isZcode ? onImportZcodePrompt : isClaude ? onImportClaudePrompt : onImportPrompt;
    void handler(event.currentTarget.files?.[0]);
  };

  // 当前引擎下启用/禁用内置模板与自定义提示词的回调。
  const enableBuiltinHandler = isPi ? onEnablePiBuiltinPrompt : isKilo ? onEnableKiloBuiltinPrompt : isGrok ? onEnableGrokBuiltinPrompt : isZcode ? onEnableZcodeBuiltinPrompt : isClaude ? onEnableClaudeBuiltinPrompt : onEnableBuiltinPrompt;
  const disableHandler = isPi ? onDisablePiInstruction : isKilo ? onDisableKiloInstruction : isGrok ? onDisableGrokInstruction : isZcode ? onDisableZcodeInstruction : isClaude ? onDisableClaudeInstruction : onDisableInstruction;
  const enableSavedHandler = isPi ? onEnablePiSavedPrompt : isKilo ? onEnableKiloSavedPrompt : isGrok ? onEnableGrokSavedPrompt : isZcode ? onEnableZcodeSavedPrompt : isClaude ? onEnableClaudeSavedPrompt : onEnableSavedPrompt;
  const editHandler = isPi ? onEditPiPrompt : isKilo ? onEditKiloPrompt : isGrok ? onEditGrokPrompt : isZcode ? onEditZcodePrompt : isClaude ? onEditClaudePrompt : onEditPrompt;
  const requestRuntimePreview = async (operation: "install" | "uninstall", apply?: MaybeAsyncAction) => {
    setRuntimePreviewLoading(true);
    setRuntimePreviewError("");
    try {
      const preview = await invoke<PromptRuntimePreview>("preview_prompt_runtime", {
        engine: promptEngine,
        operation,
        configDir: isCodex ? codexRuntimeScope || null : null,
      });
      setPendingRuntimePreview({ preview, apply, claudeCliRuntime: false });
    } catch (error) {
      setRuntimePreviewError(String(error));
    } finally {
      setRuntimePreviewLoading(false);
    }
  };
  const requestClaudeRuntimePreview = async () => {
    setRuntimePreviewLoading(true);
    setRuntimePreviewError("");
    try {
      const preview = await invoke<PromptRuntimePreview>("preview_claude_runtime");
      setPendingRuntimePreview({
        preview,
        apply: preview.operation === "install" ? onInstallClaudeRuntime : onUninstallClaudeRuntime,
        claudeCliRuntime: true,
      });
    } catch (error) {
      setRuntimePreviewError(String(error));
    } finally {
      setRuntimePreviewLoading(false);
    }
  };
  const applyRuntimePreview = async () => {
    const action = pendingRuntimePreview?.apply;
    if (!action) return;
    setRuntimePreviewBusy(true);
    try {
      await action();
      setPendingRuntimePreview(null);
    } finally {
      setRuntimePreviewBusy(false);
    }
  };
  const confirmPromptChange = (operation: "install" | "uninstall", action: MaybeAsyncAction) => {
    void requestRuntimePreview(operation, action);
  };

  return (
    <PageTransition pageKey={`prompts:${instructionMode}`}>
      <section className={cx("cx-prompts-page", "cx-prompts-page--list", className)} aria-label={copy.title}>
      <RuntimePreviewDialog
        lang={lang}
        pending={pendingRuntimePreview}
        busy={runtimePreviewBusy}
        onClose={() => {
          if (!runtimePreviewBusy) setPendingRuntimePreview(null);
        }}
        onApply={applyRuntimePreview}
      />
      <PromptBackupsDialog
        lang={lang}
        engine={promptEngine}
        open={promptBackupsOpen}
        loading={promptBackupsLoading}
        restoringId={promptRestoreBusyId}
        backups={promptBackups}
        onClose={onClosePromptBackups}
        onRestore={onRestorePromptBackup}
      />
      <PromptCategoryManager
        open={categoryManagerOpen}
        lang={lang}
        categories={promptCategories.categories}
        prompts={categoryItems}
        categoryForPrompt={promptCategories.categoryForPrompt}
        onClose={() => setCategoryManagerOpen(false)}
        onAddCategory={promptCategories.addCategory}
        onRenameCategory={promptCategories.renameCategory}
        onDeleteCategory={promptCategories.deleteCategory}
        onMovePrompt={promptCategories.movePrompt}
      />
      <header className="cx-prompts-header">
        <div className="cx-prompts-heading">
          <p><Sparkles size={14} aria-hidden="true" />{copy.eyebrow}</p>
          <h2>{copy.title}</h2>
        </div>
        <div className="cx-prompts-header-actions">
          <div className="cx-prompts-engine-switch" role="radiogroup" aria-label={copy.engineSwitchLabel}>
            <button
              type="button"
              role="radio"
              aria-checked={promptEngine === "codex"}
              className={cx("cx-prompts-engine-button", promptEngine === "codex" && "cx-prompts-engine-button--active")}
              onClick={() => onPromptEngineChange("codex")}
            >
              {copy.engineCodex}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={promptEngine === "claude"}
              className={cx("cx-prompts-engine-button", promptEngine === "claude" && "cx-prompts-engine-button--active")}
              onClick={() => onPromptEngineChange("claude")}
            >
              {copy.engineClaude}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={promptEngine === "zcode"}
              className={cx("cx-prompts-engine-button", promptEngine === "zcode" && "cx-prompts-engine-button--active")}
              onClick={() => onPromptEngineChange("zcode")}
            >
              {copy.engineZcode}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={promptEngine === "grok"}
              className={cx("cx-prompts-engine-button", promptEngine === "grok" && "cx-prompts-engine-button--active")}
              onClick={() => onPromptEngineChange("grok")}
            >
              {copy.engineGrok}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={promptEngine === "kilo"}
              className={cx("cx-prompts-engine-button", promptEngine === "kilo" && "cx-prompts-engine-button--active")}
              onClick={() => onPromptEngineChange("kilo")}
            >
              {copy.engineKilo}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={promptEngine === "pi"}
              className={cx("cx-prompts-engine-button", promptEngine === "pi" && "cx-prompts-engine-button--active")}
              onClick={() => onPromptEngineChange("pi")}
            >
              {copy.enginePi}
            </button>
          </div>
          <input
            ref={promptImportRef}
            className="cx-prompts-file-input"
            type="file"
            accept=".md,text/markdown,text/plain"
            onChange={handlePromptFileChange}
            disabled={loading}
            tabIndex={-1}
            aria-hidden="true"
          />
          {isCodex && (
            <Button
              variant="secondary"
              icon={promptSyncing ? <Loader2 className="cx-prompts-spin" /> : <RefreshCw />}
              onClick={() => run(onSyncBuiltinPrompts)}
              disabled={loading || promptSyncing}
            >
              {promptSyncing ? copy.syncing : copy.sync}
            </Button>
          )}
          <Button
            variant="secondary"
            icon={importBusy ? <Loader2 className="cx-prompts-spin" /> : <Upload />}
            onClick={() => promptImportRef.current?.click()}
            disabled={loading}
          >
            {importBusy ? copy.importing : copy.importMd}
          </Button>
          <Button className="cx-prompts-add-button" icon={<Plus />} onClick={onAddPrompt} disabled={loading}>
            {copy.add}
          </Button>
        </div>
      </header>

      <section className="cx-prompts-mode-panel">
        <div className="cx-prompts-active-summary">
          <div className="cx-prompts-summary-label">
            <p>{copy.currentStatus}</p>
            <button type="button" onClick={onOpenPromptBackups} disabled={Boolean(promptRestoreBusyId)}>
              <History size={14} aria-hidden="true" />
              {copy.backups}
            </button>
          </div>
          <div className="cx-prompts-active-title" aria-live="polite">
            <span className={cx(
              "cx-prompts-state-dot",
              activeInstructionEnabled && "cx-prompts-state-dot--active",
              codexInstructionInactive && "cx-prompts-state-dot--warning",
            )} aria-hidden="true" />
            <strong>{activeInstructionEnabled ? activeInstructionTitleResolved : codexInstructionInactive ? copy.inactiveByConfig : copy.noActive}</strong>
            {activeInstructionEnabled && <StatusBadge tone="success" dot={false}>{activeModeLabel}</StatusBadge>}
            {codexInstructionInactive && <StatusBadge tone="warning" dot={false}>{copy.inactiveByConfig}</StatusBadge>}
          </div>
          <span>
            {activeInstructionEnabled
              ? activeModeCopy.detail
              : codexInstructionInactive
                ? copy.inactiveByConfigDetail(codexInactiveInstructionFile || "Markdown")
                : copy.inactiveDetail}
          </span>
        </div>

        <div className="cx-prompts-mode-choice">
          <div className="cx-prompts-mode-copy" ref={promptModeHelpRef}>
            <div className="cx-prompts-mode-title">
              <strong>{copy.enableMethod}</strong>
              <IconButton
                icon={<CircleHelp size={15} />}
                label={copy.helpLabel}
                variant="ghost"
                size="sm"
                aria-expanded={promptModeHelpOpen}
                aria-controls={helpId}
                onClick={onTogglePromptModeHelp}
              />
            </div>
            {promptModeHelpOpen && (
              <div id={helpId} className="cx-prompts-mode-help" role="dialog" aria-label={copy.helpLabel}>
                <div><strong>{copy.keepExisting}</strong><span>{appendModeCopy.help}</span></div>
                <div><strong>{copy.replaceExisting}</strong><span>{replaceModeCopy.help}</span></div>
              </div>
            )}
            <span>{modePending ? copy.pendingMode(selectedModeLabel) : copy.modeHint}</span>
          </div>
          <div className="cx-prompts-mode-segments" role="radiogroup" aria-label={copy.enableMethod}>
            <button
              type="button"
              role="radio"
              aria-checked={promptInjectionMode === "append"}
              className={cx("cx-prompts-mode-button", promptInjectionMode === "append" && "cx-prompts-mode-button--active")}
              title={appendModeCopy.title}
              onClick={() => onPromptInjectionModeChange("append")}
            >
              <CirclePlus size={16} aria-hidden="true" />
              {copy.keepExisting}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={promptInjectionMode === "replace"}
              className={cx("cx-prompts-mode-button", promptInjectionMode === "replace" && "cx-prompts-mode-button--active")}
              title={replaceModeCopy.title}
              onClick={() => onPromptInjectionModeChange("replace")}
            >
              <ArrowLeftRight size={16} aria-hidden="true" />
              {copy.replaceExisting}
            </button>
          </div>
        </div>
      </section>

      <RuntimeDeploymentPanel
        lang={lang}
        engine={promptEngine}
        instructionEnabled={activeInstructionEnabled}
        codexRuntimeEntryPath={codexRuntimeEntryPath}
        claudeRuntimeEntryPath={claudeRuntimeEntryPath}
        claudeRuntime={claudeRuntime}
        zcodeRuntime={zcodeRuntime}
        grokRuntime={grokRuntime}
        kiloRuntime={kiloRuntime}
        piRuntime={piRuntime}
        loading={loading}
        previewLoading={runtimePreviewLoading}
        onPreview={() => void requestRuntimePreview(activeInstructionEnabled ? "uninstall" : "install")}
        onToggleClaudeRuntime={() => void requestClaudeRuntimePreview()}
      />
      {runtimePreviewError && <p className="cx-prompt-runtime-error" role="alert">{runtimePreviewError}</p>}

      <div className="cx-prompt-category-toolbar">
        <div className="cx-skills-tabs cx-prompt-category-tabs" role="group" aria-label={copy.manageCategories}>
          {promptCategories.categories.map((category) => (
            <button
              type="button"
              aria-pressed={category.id === promptCategories.activeCategoryId}
              className={cx("cx-skills-tab", "cx-prompt-category-tab", category.id === promptCategories.activeCategoryId && "cx-skills-tab--active")}
              key={category.id}
              onClick={() => promptCategories.setActiveCategoryId(category.id)}
            >
              {category.name}
            </button>
          ))}
        </div>
        <button type="button" className="cx-prompt-category-manage" onClick={() => setCategoryManagerOpen(true)}>
          <Settings2 size={16} aria-hidden="true" />{copy.manageCategories}
        </button>
      </div>

      <PageTransition pageKey={`prompts-category:${promptCategories.activeCategoryId}`}>
        <section className="cx-prompts-list-panel" aria-label={lang === "zh" ? "提示词模板" : "Prompt templates"}>
          <div className="cx-prompts-list">
          {visiblePromptCount === 0 && (
            <div className="cx-prompt-category-list-empty"><FileText size={22} aria-hidden="true" />{copy.emptyCategory}</div>
          )}
          {activeTemplates.filter((template) =>
            promptIsVisible(promptCategoryKey(promptEngine, "builtin", template.id))).map((template) => {
            const enabled = template.id === activeBuiltinId;
            return (
              <PromptRow
                key={template.id}
                title={template.title}
                description={template.subtitle}
                enabled={enabled}
                loading={loading}
                toggleLabel={enabled ? copy.disable : copy.enable}
                onToggle={() => confirmPromptChange(
                  enabled ? "uninstall" : "install",
                  enabled ? disableHandler : () => enableBuiltinHandler(template.id),
                )}
              >
                {enabled && (
                  <div className="cx-prompts-row-meta">
                    <StatusBadge tone="accent" dot={false}>{copy.current} · {activeModeLabel}</StatusBadge>
                  </div>
                )}
              </PromptRow>
            );
          })}

          {isCodex && promptCatalogReady && orphanedBuiltinPrompt
            && promptIsVisible(promptCategoryKey(promptEngine, "builtin", orphanedBuiltinPrompt.id)) && (
            <PromptRow
              title={orphanedBuiltinPrompt.title}
              description={orphanedBuiltinPrompt.description || copy.removedDescription}
              enabled
              loading={loading}
              toggleLabel={copy.disable}
              onToggle={() => confirmPromptChange("uninstall", disableHandler)}
            >
              <div className="cx-prompts-row-meta">
                <StatusBadge tone="accent" dot={false}>{copy.current} · {activeModeLabel}</StatusBadge>
                <StatusBadge tone="warning" dot={false}>{copy.onlineRemoved}</StatusBadge>
              </div>
            </PromptRow>
          )}

          {activeSavedPrompts.filter((prompt) =>
            promptIsVisible(savedPromptCategoryKey(promptEngine, prompt))).map((prompt) => {
            const managed = prompt.id === activeManagedSavedPromptId;
            const preserved = isCodex && !managed && Boolean(preservedSavedPromptFilename) && prompt.filename === preservedSavedPromptFilename;
            const enabled = managed || preserved;
            return (
              <PromptRow
                key={prompt.id}
                title={prompt.title}
                description={preserved ? copy.preservedDescription : copy.customDescription}
                enabled={enabled}
                loading={loading}
                toggleLabel={managed ? copy.disable : preserved ? copy.disableExternal : copy.enable}
                onToggle={() => managed
                  ? confirmPromptChange("uninstall", disableHandler)
                  : preserved
                    ? confirmPromptChange("uninstall", onDisableExternalPrompt)
                    : confirmPromptChange("install", () => enableSavedHandler(prompt.id))}
                actions={(
                  <div className="cx-prompts-icon-actions">
                    <IconButton
                      icon={<PencilLine size={15} />}
                      label={copy.edit}
                      size="sm"
                      onClick={() => editHandler(prompt)}
                      disabled={loading}
                    />
                    <IconButton
                      icon={<Trash2 size={15} />}
                      label={copy.remove}
                      variant="danger"
                      size="sm"
                      onClick={() => void deleteSavedPrompt(prompt)}
                      disabled={loading}
                    />
                  </div>
                )}
              >
                {managed && (
                  <div className="cx-prompts-row-meta">
                    <StatusBadge tone="accent" dot={false}>{copy.current} · {activeModeLabel}</StatusBadge>
                  </div>
                )}
                {!managed && preserved && (
                  <div className="cx-prompts-row-meta">
                    <StatusBadge tone="info" dot={false}>{copy.current} · {copy.appendMode}</StatusBadge>
                  </div>
                )}
              </PromptRow>
            );
          })}

          {isCodex && externalPrompt
            && promptIsVisible(promptCategoryKey(promptEngine, "external", externalPrompt.filename || externalPrompt.title)) && (
            <PromptRow
              title={externalPrompt.title || copy.existingPrompt}
              description={externalPrompt.description}
              enabled
              loading={loading}
              toggleLabel={copy.disableExternal}
              onToggle={() => confirmPromptChange("uninstall", onDisableExternalPrompt)}
            >
              {externalPrompt.filename && <code className="cx-prompts-external-path">{externalPrompt.filename}</code>}
            </PromptRow>
          )}
          </div>
        </section>
      </PageTransition>
      </section>
    </PageTransition>
  );
}
