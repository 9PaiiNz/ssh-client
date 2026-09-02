import { useEffect, useState } from "react";
import type { SessionTab } from "@/lib/types";

interface StatusBarProps {
  activeTab: SessionTab | null;
}

function formatUptime(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
  return `${seconds}s`;
}

export function StatusBar({ activeTab }: StatusBarProps) {
  const [uptime, setUptime] = useState("");

  useEffect(() => {
    if (!activeTab?.connectedAt || activeTab.status !== "connected") {
      setUptime("");
      return;
    }
    const update = () => {
      setUptime(formatUptime(Date.now() - activeTab.connectedAt!));
    };
    update();
    const interval = setInterval(update, 1000);
    return () => clearInterval(interval);
  }, [activeTab?.connectedAt, activeTab?.status]);

  return (
    <div className="flex h-6 shrink-0 items-center justify-between border-t border-border bg-surface px-3 text-[11px] text-muted">
      <div className="flex items-center gap-3">
        {activeTab ? (
          <>
            <span>
              {activeTab.username}@{activeTab.host}:{activeTab.port}
            </span>
            <span className="capitalize">{activeTab.status}</span>
            {activeTab.error && (
              <span className="text-danger truncate max-w-[300px]">
                {activeTab.error}
              </span>
            )}
          </>
        ) : (
          <span>Not connected</span>
        )}
      </div>
      {uptime && <span>Connected {uptime}</span>}
    </div>
  );
}
