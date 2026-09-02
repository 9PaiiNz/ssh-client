import { X } from "lucide-react";
import type { SessionTab } from "@/lib/types";
import { cn } from "@/lib/utils";

interface TabBarProps {
  tabs: SessionTab[];
  activeTabId: string | null;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
}

export function TabBar({ tabs, activeTabId, onSelect, onClose }: TabBarProps) {
  if (tabs.length === 0) return null;

  return (
    <div className="flex h-9 shrink-0 items-end gap-0.5 overflow-x-auto border-b border-border bg-background px-2">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className={cn(
            "group flex h-8 max-w-[200px] cursor-pointer items-center gap-2 rounded-t-md border border-b-0 px-3 text-xs transition-colors",
            activeTabId === tab.id
              ? "border-border bg-surface text-foreground"
              : "border-transparent bg-transparent text-muted hover:bg-surface-hover hover:text-foreground"
          )}
          onClick={() => onSelect(tab.id)}
        >
          <span
            className={cn(
              "h-1.5 w-1.5 shrink-0 rounded-full",
              tab.status === "connected" && "bg-accent",
              tab.status === "connecting" && "bg-yellow-400 animate-pulse",
              tab.status === "error" && "bg-danger",
              tab.status === "disconnected" && "bg-muted"
            )}
          />
          <span className="truncate">{tab.title}</span>
          <button
            className="ml-auto shrink-0 rounded p-0.5 opacity-0 hover:bg-surface-hover group-hover:opacity-100"
            onClick={(e) => {
              e.stopPropagation();
              onClose(tab.id);
            }}
          >
            <X className="h-3 w-3" />
          </button>
        </div>
      ))}
    </div>
  );
}
