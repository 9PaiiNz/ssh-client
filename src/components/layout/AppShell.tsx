import { useState } from "react";
import {
  ArrowLeftRight,
  FolderOpen,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Terminal,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Sidebar } from "@/components/layout/Sidebar";
import { TabBar } from "@/components/layout/TabBar";
import { StatusBar } from "@/components/layout/StatusBar";
import { TerminalView } from "@/components/terminal/TerminalView";
import { ConnectionDialog } from "@/components/connections/ConnectionDialog";
import { SftpDrawer } from "@/components/sftp/SftpDrawer";
import { PortForwardPanel } from "@/components/portforward/PortForwardPanel";
import type { ConnectionProfile } from "@/lib/types";
import { useSessionStore } from "@/stores/useSessionStore";

export function AppShell() {
  const {
    tabs,
    activeTabId,
    sidebarOpen,
    sftpOpen,
    portForwardOpen,
    setSidebarOpen,
    setSftpOpen,
    setPortForwardOpen,
    setActiveTab,
    closeTab,
  } = useSessionStore();

  const [dialogOpen, setDialogOpen] = useState(false);
  const [editProfile, setEditProfile] = useState<ConnectionProfile | null>(null);

  const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;
  const isConnected = activeTab?.status === "connected";

  const handleNewConnection = () => {
    setEditProfile(null);
    setDialogOpen(true);
  };

  const handleEditConnection = (profile: ConnectionProfile) => {
    setEditProfile(profile);
    setDialogOpen(true);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-1 min-h-0">
        {sidebarOpen && (
          <div className="w-60 shrink-0">
            <Sidebar
              onNewConnection={handleNewConnection}
              onEditConnection={handleEditConnection}
            />
          </div>
        )}

        <div className="flex flex-1 flex-col min-w-0 relative">
          <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2">
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={() => setSidebarOpen(!sidebarOpen)}
            >
              {sidebarOpen ? (
                <PanelLeftClose className="h-4 w-4" />
              ) : (
                <PanelLeftOpen className="h-4 w-4" />
              )}
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={handleNewConnection}
            >
              <Plus className="h-4 w-4" />
            </Button>
            {isConnected && (
              <>
                <Button
                  variant={sftpOpen ? "default" : "ghost"}
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => {
                    setSftpOpen(!sftpOpen);
                    if (!sftpOpen) setPortForwardOpen(false);
                  }}
                >
                  <FolderOpen className="h-4 w-4" />
                </Button>
                <Button
                  variant={portForwardOpen ? "default" : "ghost"}
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => {
                    setPortForwardOpen(!portForwardOpen);
                    if (!portForwardOpen) setSftpOpen(false);
                  }}
                >
                  <ArrowLeftRight className="h-4 w-4" />
                </Button>
              </>
            )}
          </div>

          <TabBar
            tabs={tabs}
            activeTabId={activeTabId}
            onSelect={setActiveTab}
            onClose={closeTab}
          />

          <div className="flex flex-1 min-h-0">
            <div className="flex-1 min-w-0">
              {activeTab ? (
                activeTab.status === "connecting" ? (
                  <div className="flex h-full items-center justify-center text-muted text-sm">
                    Connecting to {activeTab.title}...
                  </div>
                ) : activeTab.status === "error" ? (
                  <div className="flex h-full flex-col items-center justify-center gap-2 text-muted">
                    <p className="text-danger text-sm">{activeTab.error}</p>
                    <p className="text-xs">Close this tab and try again</p>
                  </div>
                ) : (
                  <TerminalView sessionId={activeTab.id} />
                )
              ) : (
                <div className="flex h-full flex-col items-center justify-center gap-4 text-muted">
                  <Terminal className="h-12 w-12 opacity-30" />
                  <div className="text-center">
                    <p className="text-sm font-medium text-foreground">
                      No active session
                    </p>
                    <p className="text-xs mt-1">
                      Select a host from the sidebar or create a new connection
                    </p>
                  </div>
                  <Button onClick={handleNewConnection}>
                    <Plus className="h-4 w-4 mr-1" />
                    New Connection
                  </Button>
                </div>
              )}
            </div>

            {isConnected && sftpOpen && activeTab && (
              <SftpDrawer
                sessionId={activeTab.id}
                isOpen={sftpOpen}
                onClose={() => setSftpOpen(false)}
              />
            )}
          </div>

          {isConnected && portForwardOpen && activeTab && (
            <PortForwardPanel
              sessionId={activeTab.id}
              open={portForwardOpen}
              onClose={() => setPortForwardOpen(false)}
            />
          )}
        </div>
      </div>

      <StatusBar activeTab={activeTab} />

      <ConnectionDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        editProfile={editProfile}
      />
    </div>
  );
}
