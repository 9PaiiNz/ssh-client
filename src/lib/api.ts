import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConnectionProfile,
  HistoryEntry,
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

export async function keychainAvailable(): Promise<boolean> {
  return invoke("keychain_available");
}

/** Connect using a saved profile; rejects with a ConnectError. */
export async function connectSsh(profileId: string): Promise<string> {
  return invoke("connect_ssh", { profileId });
}

/** Trust a server host key the user confirmed. `replace` drops old keys for the host. */
export async function trustHostKey(
  host: string,
  port: number,
  key: string,
  replace: boolean
): Promise<void> {
  return invoke("trust_host_key", { host, port, key, replace });
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

/** Download a remote folder recursively into `localDir`; resolves to the created folder. */
export async function downloadSftpDir(
  sessionId: string,
  remotePath: string,
  localDir: string
): Promise<string> {
  return invoke("download_sftp_dir", { sessionId, remotePath, localDir });
}

/** Upload local files and folders (recursively) into the remote folder `remoteDir`. */
export async function uploadSftpPaths(
  sessionId: string,
  localPaths: string[],
  remoteDir: string
): Promise<void> {
  return invoke("upload_sftp_paths", { sessionId, localPaths, remoteDir });
}

/** Delete a remote file or folder (recursively); resolves to the number of removed entries. */
export async function deleteSftpPath(
  sessionId: string,
  path: string
): Promise<number> {
  return invoke("delete_sftp_path", { sessionId, path });
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

/** A chunk of terminal output; `seq` is the stream position after it. */
export interface TerminalChunk {
  seq: number;
  data: string;
}

export function onTerminalOutput(
  sessionId: string,
  handler: (chunk: TerminalChunk) => void
): Promise<UnlistenFn> {
  return listen<TerminalChunk>(`terminal-output:${sessionId}`, (event) => {
    handler(event.payload);
  });
}

/** Recent output of an open session; `seq` is the stream position it ends at. */
export async function getScrollback(sessionId: string): Promise<TerminalChunk> {
  return invoke("get_scrollback", { sessionId });
}

/** Saved sessions of a profile, newest first. */
export async function listHistory(profileId: string): Promise<HistoryEntry[]> {
  return invoke("list_history", { profileId });
}

export async function readHistory(
  profileId: string,
  entryId: string
): Promise<string> {
  return invoke("read_history", { profileId, entryId });
}

/** Delete one saved session, or all of the profile's if `entryId` is omitted. */
export async function deleteHistory(
  profileId: string,
  entryId?: string
): Promise<void> {
  return invoke("delete_history", { profileId, entryId: entryId ?? null });
}

export async function historyEnabled(): Promise<boolean> {
  return invoke("history_enabled");
}

export async function setHistoryEnabled(enabled: boolean): Promise<void> {
  return invoke("set_history_enabled", { enabled });
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
