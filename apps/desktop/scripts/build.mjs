import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const tauriCli = fileURLToPath(
  new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url),
);
const windowsNodeStageScript = fileURLToPath(
  new URL("./stage-windows-node.ps1", import.meta.url),
);
const userArgs = process.argv.slice(2);
if (userArgs[0] === "--") {
  userArgs.shift();
}
const args = ["build"];
const hasSigningKey = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY?.trim());

function requireSuccess(result, label) {
  if (result.error) {
    console.error(`[build] ${label}: ${result.error.message}`);
    process.exit(1);
  }

  if (result.signal) {
    console.error(`[build] ${label} terminated by signal ${result.signal}.`);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

if (process.platform === "win32") {
  const powershell = process.env.SystemRoot
    ? join(
        process.env.SystemRoot,
        "System32",
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
      )
    : "powershell.exe";
  const stageResult = spawnSync(
    powershell,
    [
      "-NoLogo",
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      windowsNodeStageScript,
    ],
    { env: process.env, stdio: "inherit" },
  );
  requireSuccess(stageResult, "Failed to stage the Windows Node.js runtime");
}

if (!hasSigningKey) {
  console.log(
    "[build] TAURI_SIGNING_PRIVATE_KEY is not set; updater artifacts are disabled for this local build.",
  );
  args.push(
    "--config",
    JSON.stringify({ bundle: { createUpdaterArtifacts: false } }),
  );
}

args.push(...userArgs);

const result = spawnSync(process.execPath, [tauriCli, ...args], {
  env: process.env,
  stdio: "inherit",
});

requireSuccess(result, "Failed to build the Tauri application");
