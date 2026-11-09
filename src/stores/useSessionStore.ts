import { create } from "zustand";
import type {
  ConnectError,
  ConnectionProfile,
  HostKeyInfo,
  SessionTab,
  TransferProgress,
} from "@/lib/types";
import { connectSsh, disconnectSsh, trustHostKey } from "@/lib/api";

export interface HostKeyPrompt {
  info: HostKeyInfo;
  /** True if the host presented a different key than the one on record. */
  changed: boolean;
  profile: ConnectionProfile;
}

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "kind" in err) {
    const e = err as ConnectError;
    if (e.kind === "other") return e.message;
    return e.kind === "hostKeyChanged"
      ? `Host key for ${e.host} has changed`
      : `Unknown host key for ${e.host}`;
  }
  return err instanceof Error ? err.message : String(err);
}

interface SessionStore {
  tabs: SessionTab[];
  activeTabId: string | null;
  sidebarOpen: boolean;
  sftpOpen: boolean;
  portForwardOpen: boolean;
  transfers: TransferProgress[];
  setSidebarOpen: (open: boolean) => void;
  setSftpOpen: (open: boolean) => void;
  setPortForwardOpen: (open: boolean) => void;
  setActiveTab: (id: string) => void;
  hostKeyPrompt: HostKeyPrompt | null;
  /** Connect to a saved profile. Resolves to null if the user must first confirm the host key. */
  openSession: (profile: ConnectionProfile) => Promise<string | null>;
  /** Answer the host key prompt: trust the key and connect, or cancel. */
  resolveHostKeyPrompt: (trust: boolean) => Promise<void>;
  closeTab: (id: string) => Promise<void>;
  updateTabStatus: (
    id: string,
    status: SessionTab["status"],
    error?: string
  ) => void;
  addTransfer: (transfer: TransferProgress) => void;
  updateTransfer: (transfer: TransferProgress) => void;
  /** Remove a session's finished (completed or failed) transfers from the list. */
  clearFinishedTransfers: (sessionId: string) => void;
}

export const useSessionStore = create<SessionStore>((set, get) => ({
  tabs: [],
  activeTabId: null,
  sidebarOpen: true,
  sftpOpen: false,
  portForwardOpen: false,
  transfers: [],
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
  setSftpOpen: (open) => set({ sftpOpen: open }),
  setPortForwardOpen: (open) => set({ portForwardOpen: open }),
  setActiveTab: (id) => set({ activeTabId: id }),
  hostKeyPrompt: null,
  openSession: async (profile) => {
    const tabId = crypto.randomUUID();
    const tab: SessionTab = {
      id: tabId,
      profileId: profile.id,
      title: profile.name || `${profile.username}@${profile.host}`,
      host: profile.host,
      port: profile.port,
      username: profile.username,
      status: "connecting",
    };
    set((state) => ({
      tabs: [...state.tabs, tab],
      activeTabId: tabId,
    }));

    try {
      const sessionId = await connectSsh(profile.id);
      set((state) => ({
        tabs: state.tabs.map((t) =>
          t.id === tabId
            ? { ...t, id: sessionId, status: "connected", connectedAt: Date.now() }
            : t
        ),
        activeTabId: sessionId,
      }));
      return sessionId;
    } catch (err) {
      const e = err as ConnectError;
      if (e && (e.kind === "unknownHostKey" || e.kind === "hostKeyChanged")) {
        // Drop the placeholder tab; it is reopened if the user trusts the key.
        const remaining = get().tabs.filter((t) => t.id !== tabId);
        set((state) => ({
          tabs: remaining,
          activeTabId:
            state.activeTabId === tabId
              ? (remaining[remaining.length - 1]?.id ?? null)
              : state.activeTabId,
          hostKeyPrompt: {
            info: e,
            changed: e.kind === "hostKeyChanged",
            profile,
          },
        }));
        return null;
      }
      const message = errorMessage(err);
      set((state) => ({
        tabs: state.tabs.map((t) =>
          t.id === tabId ? { ...t, status: "error", error: message } : t
        ),
      }));
      throw new Error(message);
    }
  },
  resolveHostKeyPrompt: async (trust) => {
    const prompt = get().hostKeyPrompt;
    set({ hostKeyPrompt: null });
    if (!prompt || !trust) return;
    const { info, changed, profile } = prompt;
    await trustHostKey(info.host, info.port, info.key, changed);
    await get().openSession(profile);
  },
  closeTab: async (id) => {
    try {
      await disconnectSsh(id);
    } catch {
      // session may already be closed
    }
    const { tabs, activeTabId } = get();
    const remaining = tabs.filter((t) => t.id !== id);
    set({
      tabs: remaining,
      activeTabId:
        activeTabId === id
          ? remaining.length > 0
            ? remaining[remaining.length - 1].id
            : null
          : activeTabId,
      sftpOpen: activeTabId === id ? false : get().sftpOpen,
      portForwardOpen: activeTabId === id ? false : get().portForwardOpen,
    });
  },
  updateTabStatus: (id, status, error) =>
    set((state) => ({
      tabs: state.tabs.map((t) =>
        t.id === id
          ? {
              ...t,
              status,
              error,
              connectedAt:
                status === "connected" ? Date.now() : t.connectedAt,
            }
          : t
      ),
    })),
  addTransfer: (transfer) =>
    set((state) => ({ transfers: [...state.transfers, transfer] })),
  updateTransfer: (transfer) =>
    set((state) => ({
      transfers: state.transfers.map((t) =>
        t.transferId === transfer.transferId ? transfer : t
      ),
    })),
  clearFinishedTransfers: (sessionId) =>
    set((state) => ({
      transfers: state.transfers.filter(
        (t) => t.sessionId !== sessionId || t.status === "in_progress"
      ),
    })),
}));
