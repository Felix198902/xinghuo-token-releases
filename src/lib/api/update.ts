import { invoke } from "@tauri-apps/api/core";

export interface AppUpdateInfo {
  available: boolean;
  currentVersion: string;
  latestVersion: string;
  title?: string;
  notes?: string;
  downloadUrl: string;
  source: "GitHub" | "Gitee";
}

export interface AppUpdateDownloadProgress {
  downloaded: number;
  total?: number;
}

export async function checkAppUpdate(): Promise<AppUpdateInfo> {
  return await invoke("check_app_update");
}

export async function installAppUpdate(): Promise<boolean> {
  return await invoke("install_app_update");
}
