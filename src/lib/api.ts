import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConnectionProfile,
  PortForwardRule,
  SftpEntry,
  TransferProgress,
} from "./types";

export async function listProfiles(): Promise<ConnectionProfile[]> {
  return invoke("list_profiles");
}

export async function saveProfile(profile: ConnectionProfile): Promise<void> {
  return invoke("save_profile", { profile });
}

export async function deleteProfile(id: string): Promise<void> {
  return invoke("delete_profile", { id });
}

export interface ConnectParams {
  profileId?: string;
  host: string;
  port: number;
  username: string;
  authType: "password" | "key";
  password?: string;
  keyPath?: string;
  keyPassphrase?: string;
}

export async function connectSsh(params: ConnectParams): Promise<string> {
  return invoke("connect_ssh", { params });
}

export async function disconnectSsh(sessionId: string): Promise<void> {
  return invoke("disconnect_ssh", { sessionId });
}

export async function writeTerminal(
  sessionId: string,
  data: string
): Promise<void> {
  return invoke("write_terminal", { sessionId, data });
}

export async function resizeTerminal(
  sessionId: string,
  cols: number,
  rows: number
): Promise<void> {
  return invoke("resize_terminal", { sessionId, cols, rows });
}

export async function listSftpDir(
  sessionId: string,
  path: string
): Promise<SftpEntry[]> {
  return invoke("list_sftp_dir", { sessionId, path });
}

export async function downloadSftpFile(
  sessionId: string,
  remotePath: string,
  localPath: string
): Promise<string> {
  return invoke("download_sftp_file", { sessionId, remotePath, localPath });
}

export async function uploadSftpFile(
  sessionId: string,
  localPath: string,
  remotePath: string
): Promise<string> {
  return invoke("upload_sftp_file", { sessionId, localPath, remotePath });
}

export async function listPortForwards(
  sessionId: string
): Promise<PortForwardRule[]> {
  return invoke("list_port_forwards", { sessionId });
}

export async function addPortForward(
  sessionId: string,
  forwardType: "local" | "remote",
  bindHost: string,
  bindPort: number,
  targetHost: string,
  targetPort: number
): Promise<PortForwardRule> {
  return invoke("add_port_forward", {
    sessionId,
    forwardType,
    bindHost,
    bindPort,
    targetHost,
    targetPort,
  });
}

export async function removePortForward(
  sessionId: string,
  ruleId: string
): Promise<void> {
  return invoke("remove_port_forward", { sessionId, ruleId });
}

export function onTerminalOutput(
  sessionId: string,
  handler: (data: string) => void
): Promise<UnlistenFn> {
  return listen<string>(`terminal-output:${sessionId}`, (event) => {
    handler(event.payload);
  });
}

export function onSessionStatus(
  sessionId: string,
  handler: (status: string, error?: string) => void
): Promise<UnlistenFn> {
  return listen<{ status: string; error?: string }>(
    `session-status:${sessionId}`,
    (event) => {
      handler(event.payload.status, event.payload.error);
    }
  );
}

export function onTransferProgress(
  handler: (progress: TransferProgress) => void
): Promise<UnlistenFn> {
  return listen<TransferProgress>("sftp-transfer-progress", (event) => {
    handler(event.payload);
  });
}
