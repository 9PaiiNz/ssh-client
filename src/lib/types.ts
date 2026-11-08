export interface ConnectionProfile {
  id: string;
  name: string;
  group?: string;
  host: string;
  port: number;
  username: string;
  auth: {
    type: "password" | "key";
    /** Write-only: sent when saving; never returned by listProfiles. */
    password?: string;
    keyPath?: string;
    /** Write-only: sent when saving; never returned by listProfiles. */
    keyPassphrase?: string;
    /** Set by the backend when a password / passphrase is saved. */
    hasPassword?: boolean;
    hasPassphrase?: boolean;
  };
}

export interface HostKeyInfo {
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
  /** OpenSSH-encoded public key, passed back to trustHostKey. */
  key: string;
}

/** Error thrown by connectSsh. */
export type ConnectError =
  | ({ kind: "unknownHostKey" } & HostKeyInfo)
  | ({ kind: "hostKeyChanged" } & HostKeyInfo)
  | { kind: "other"; message: string };

export interface SessionTab {
  id: string;
  profileId?: string;
  title: string;
  host: string;
  port: number;
  username: string;
  status: "connecting" | "connected" | "disconnected" | "error";
  connectedAt?: number;
  error?: string;
}

export interface SftpEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modified: number;
}

export interface PortForwardRule {
  id: string;
  sessionId: string;
  forwardType: "local" | "remote";
  bindHost: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
}

export interface TransferProgress {
  transferId: string;
  sessionId: string;
  fileName: string;
  direction: "upload" | "download";
  bytesTransferred: number;
  totalBytes: number;
  status: "in_progress" | "completed" | "failed";
  error?: string;
}
