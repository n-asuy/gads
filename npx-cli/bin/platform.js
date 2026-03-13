#!/usr/bin/env node
"use strict";

const { execSync } = require("child_process");

// Public bucket URL for binary distribution (not a secret)
const R2_PUBLIC_URL = "https://pub-2f89aa8de0ba4782a25bdce332a03b16.r2.dev";

function getEffectiveArch() {
  const platform = process.platform;
  const nodeArch = process.arch;

  if (platform === "darwin") {
    if (nodeArch === "arm64") return "arm64";
    try {
      const translated = execSync("sysctl -in sysctl.proc_translated", {
        encoding: "utf8",
      }).trim();
      if (translated === "1") return "arm64";
    } catch {
      // sysctl key not present -> assume true Intel
    }
    return "x64";
  }

  if (/arm/i.test(nodeArch)) return "arm64";

  if (platform === "win32") {
    const pa = process.env.PROCESSOR_ARCHITECTURE || "";
    const paw = process.env.PROCESSOR_ARCHITEW6432 || "";
    if (/arm/i.test(pa) || /arm/i.test(paw)) return "arm64";
  }

  return "x64";
}

const PLATFORM_MAP = {
  "linux-x64": "linux-x64",
  "linux-arm64": "linux-arm64",
  "win32-x64": "windows-x64",
  "win32-arm64": "windows-arm64",
  "darwin-x64": "macos-x64",
  "darwin-arm64": "macos-arm64",
};

function getPlatformDir() {
  const key = `${process.platform}-${getEffectiveArch()}`;
  const dir = PLATFORM_MAP[key];
  if (!dir) {
    console.error(`Unsupported platform: ${key}`);
    console.error("Supported platforms:");
    console.error("  - Linux x64 / ARM64");
    console.error("  - Windows x64 / ARM64");
    console.error("  - macOS x64 (Intel) / ARM64 (Apple Silicon)");
    process.exit(1);
  }
  return dir;
}

function getBinaryName(base) {
  return process.platform === "win32" ? `${base}.exe` : base;
}

module.exports = { getEffectiveArch, getPlatformDir, getBinaryName, R2_PUBLIC_URL };
