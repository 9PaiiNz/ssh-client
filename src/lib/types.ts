export interface ConnectionProfile {
  id: string;
  name: string;
  group?: string;
  host: string;
  port: number;
  username: string;
  auth: {
    type: "password" | "key";
    password?: string;
    keyPath?: string;
    keyPassphrase?: string;
  };
}

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
