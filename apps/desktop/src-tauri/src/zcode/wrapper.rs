//! Launcher 脚本生成与 runtime patch 逻辑（跨平台共享，纯文本操作）。
//!
//! 生成的 Node.js launcher 通过 ZCode 自带的 Electron node 执行，无需 Python。

use crate::constants::{ZCODE_AGENT_ARGS_JSON, ZCODE_LAUNCHER_LOG_NAME, ZCODE_PATCH_NEEDLE};
use crate::error::Result;
use crate::zcode::ZcodeInstallPlan;
use serde_json::json;

/// 归一化 system prompt 内容：清理 GLM ChatML 传输标记。
pub(crate) fn normalize_system_prompt_content(content: &str) -> String {
    let leading = &content[..content.len() - content.trim_start().len()];
    let mut text = content.trim_start().to_string();
    let prefixes = [
        "<|im_start|>system:<project_instructions>",
        "<|im_start|>system:",
        "<|im_start|>system",
    ];
    for prefix in prefixes {
        if text.starts_with(prefix) {
            let rest = &text[prefix.len()..];
            if prefix.ends_with("<project_instructions>") {
                text = format!("<project_instructions>{}", rest);
            } else {
                text = rest.trim_start_matches(['\r', '\n']).to_string();
            }
            break;
        }
    }
    let stripped = text.trim_end();
    if stripped.ends_with("<|im_end|>") {
        text = format!("{}\n", stripped[..stripped.len() - "<|im_end|>".len()].trim_end());
    }
    format!("{}{}", leading, text)
}

/// 生成 config.json 内容（诊断用）。
pub(crate) fn render_config(plan: &ZcodeInstallPlan) -> String {
    let payload = json!({
        "mode": "zcode-app-launcher",
        "system_file": plan.paths.system_file.display().to_string(),
        "launcher": plan.paths.launcher.display().to_string(),
        "zcode_runtime": plan.zcode_runtime.display().to_string(),
        "node_command": plan.node_command.display().to_string(),
        "cache_dir": plan.paths.cache_dir.display().to_string(),
        "launcher_log": plan.paths.launcher_log.display().to_string(),
        "agent_server_args_json": ZCODE_AGENT_ARGS_JSON,
        "app_bundle_modified": false,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_default() + "\n"
}

/// 生成 patch sidecar 内容（JavaScript 模块）。
///
/// 将 patch 锚点与替换模板存放在独立 .js 文件中，避免在 launcher 内做复杂转义。
/// replacement 模板中的 {SYSTEM_FILE} 占位符由 launcher 在运行时替换为实际路径。
pub(crate) fn render_patch_sidecar() -> String {
    let needle_escaped = ZCODE_PATCH_NEEDLE.replace('\\', '\\\\').replace('`', '\\`');
    format!(
        r#""use strict";
// ZCode runtime patch parameters (generated at install time by Codex-X)
module.exports = {{
  needle: `{needle}`,
  replacementTemplate: "{template}"
}};
"#,
        needle = needle_escaped,
        template = PATCH_REPLACEMENT_TEMPLATE,
    )
}

/// 生成 Node.js launcher 脚本。
///
/// launcher 使用 ZCode 自带的 Electron node 执行（设 ELECTRON_RUN_AS_NODE=1），
/// 完全不依赖 Python 或任何外部运行时。
pub(crate) fn render_launcher() -> String {
    LAUNCHER_JS.replace("LAUNCHER_LOG_NAME_PH", ZCODE_LAUNCHER_LOG_NAME)
}

/// patch 替换模板，{SYSTEM_FILE} 为运行时占位符。
const PATCH_REPLACEMENT_TEMPLATE: &str =
    "customSystemPrompt:(this.config.systemPrompt&&this.config.systemPrompt.trim()?this.config.systemPrompt:(()=>{try{let e=process.env.ZCODE_KEYSMITH_SYSTEM_FILE||'{SYSTEM_FILE}';let t=require(\"node:fs\");return t.existsSync(e)?t.readFileSync(e,\"utf8\"):void 0}catch{return void 0}})()),language:";

/// Launcher JavaScript 脚本模板。
const LAUNCHER_JS: &str = r###"
"use strict";
// ZCode keysmith launcher - runs via ZCode bundled Electron Node.js
// No Python or external runtime required
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { execFileSync } = require("node:child_process");

const ORIGINAL_RUNTIME = process.env.ZCODE_KEYSMITH_ORIGINAL || "";
const SYSTEM_FILE = process.env.ZCODE_KEYSMITH_SYSTEM_FILE || "";
const NODE_COMMAND = process.env.ZCODE_KEYSMITH_NODE_COMMAND || process.execPath;
const CACHE_DIR = process.env.ZCODE_KEYSMITH_CACHE_DIR || "";
const LOG_DIR = process.env.ZCODE_KEYSMITH_LOG_DIR || "";
const LOG_FILE = path.join(LOG_DIR, "LAUNCHER_LOG_NAME_PH");
const PATCH_SIDECAR = path.join(path.dirname(SYSTEM_FILE), "..", "bin", "patch.js");

function loadPatch() {
  try { return require(PATCH_SIDECAR); } catch { return null; }
}

function getPatchedRuntimePath() {
  const patch = loadPatch();
  if (!patch || !ORIGINAL_RUNTIME || !fs.existsSync(ORIGINAL_RUNTIME)) {
    throw new Error("ZCode runtime or patch sidecar not found");
  }
  const original = fs.readFileSync(ORIGINAL_RUNTIME, "utf8");
  if (!original.includes(patch.needle)) {
    throw new Error("ZCode runtime patch anchor not found: " + ORIGINAL_RUNTIME);
  }
  const replacement = patch.replacementTemplate.replace(
    "{SYSTEM_FILE}",
    SYSTEM_FILE.replace(/\\/g, "\\\\")
  );
  const patched = original.replace(patch.needle, replacement);
  const digest = crypto
    .createHash("sha256")
    .update(ORIGINAL_RUNTIME + "\0" + original + "\0" + replacement)
    .digest("hex")
    .slice(0, 16);
  fs.mkdirSync(CACHE_DIR, { recursive: true });
  const cachePath = path.join(CACHE_DIR, "zcode-keysmith-runtime-" + digest + ".cjs");
  try {
    if (!fs.existsSync(cachePath) || fs.readFileSync(cachePath, "utf8") !== patched) {
      const tmp = cachePath + ".tmp";
      fs.writeFileSync(tmp, patched, "utf8");
      fs.renameSync(tmp, cachePath);
    }
  } catch { /* ignore race */ }
  return cachePath;
}

function logInvocation(runtime, args) {
  try {
    fs.mkdirSync(LOG_DIR, { recursive: true });
    const event = {
      started_at: new Date().toISOString(),
      pid: process.pid,
      argv: process.argv,
      agent_args: args,
      runtime: String(runtime),
      original_runtime: ORIGINAL_RUNTIME,
      system_file: SYSTEM_FILE,
      node_command: NODE_COMMAND,
    };
    fs.appendFileSync(LOG_FILE, JSON.stringify(event) + "\n", "utf8");
  } catch { /* ignore */ }
}

function main() {
  const runtime = getPatchedRuntimePath();
  const args = process.argv.slice(2).length > 0
    ? process.argv.slice(2) : ["app-server", "--stdio"];
  logInvocation(runtime, args);
  const env = Object.assign({}, process.env, { ELECTRON_RUN_AS_NODE: "1" });
  execFileSync(NODE_COMMAND, [runtime].concat(args), {
    stdio: "inherit",
    env: env,
  });
}

if (require.main === module) { main(); }

"###;
