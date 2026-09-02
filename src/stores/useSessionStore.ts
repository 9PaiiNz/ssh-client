import { create } from "zustand";
import type { SessionTab, TransferProgress } from "@/lib/types";
import {
  connectSsh,
  disconnectSsh,
  type ConnectParams,
} from "@/lib/api";

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
  openSession: (params: ConnectParams, title?: string, profileId?: string) => Promise<string>;
  closeTab: (id: string) => Promise<void>;
  updateTabStatus: (
    id: string,
    status: SessionTab["status"],
    error?: string
  ) => void;
  addTransfer: (transfer: TransferProgress) => void;
  updateTransfer: (transfer: TransferProgress) => void;
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
  openSession: async (params, title, profileId) => {
    const tabId = crypto.randomUUID();
    const tab: SessionTab = {
      id: tabId,
      profileId,
      title: title ?? `${params.username}@${params.host}`,
      host: params.host,
      port: params.port,
      username: params.username,
      status: "connecting",
    };
    set((state) => ({
      tabs: [...state.tabs, tab],
      activeTabId: tabId,
    }));

    try {
      const sessionId = await connectSsh(params);
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
      const message = err instanceof Error ? err.message : String(err);
      set((state) => ({
        tabs: state.tabs.map((t) =>
          t.id === tabId ? { ...t, status: "error", error: message } : t
        ),
      }));
      throw err;
    }
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
}));
