import { useEffect, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  Plus,
  Search,
  Server,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { ConnectionProfile } from "@/lib/types";
import { useProfileStore } from "@/stores/useProfileStore";
import { useSessionStore } from "@/stores/useSessionStore";

interface SidebarProps {
  onNewConnection: () => void;
  onEditConnection: (profile: ConnectionProfile) => void;
}

export function Sidebar({ onNewConnection, onEditConnection }: SidebarProps) {
  const { profiles, loadProfiles, removeProfile } = useProfileStore();
  const openSession = useSessionStore((s) => s.openSession);
  const [search, setSearch] = useState("");
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  useEffect(() => {
    loadProfiles();
  }, [loadProfiles]);

  const filtered = profiles.filter(
    (p) =>
      p.name.toLowerCase().includes(search.toLowerCase()) ||
      p.host.toLowerCase().includes(search.toLowerCase()) ||
      (p.group ?? "").toLowerCase().includes(search.toLowerCase())
  );

  const grouped = filtered.reduce<Record<string, ConnectionProfile[]>>(
    (acc, profile) => {
      const group = profile.group || "Ungrouped";
      if (!acc[group]) acc[group] = [];
      acc[group].push(profile);
      return acc;
    },
    {}
  );

  const toggleGroup = (group: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  };

  const handleConnect = async (profile: ConnectionProfile) => {
    await openSession(
      {
        profileId: profile.id,
        host: profile.host,
        port: profile.port,
        username: profile.username,
        authType: profile.auth.type,
        password: profile.auth.password,
        keyPath: profile.auth.keyPath,
        keyPassphrase: profile.auth.keyPassphrase,
      },
      profile.name,
      profile.id
    );
  };

  return (
    <div className="flex h-full flex-col border-r border-border bg-surface">
      <div className="flex items-center justify-between border-b border-border px-3 py-3">
        <span className="text-sm font-semibold tracking-wide">Hosts</span>
        <Button variant="ghost" size="icon" onClick={onNewConnection}>
          <Plus className="h-4 w-4" />
        </Button>
      </div>

      <div className="px-3 py-2">
        <div className="relative">
          <Search className="absolute left-2.5 top-2 h-3.5 w-3.5 text-muted" />
          <Input
            placeholder="Search hosts..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-8 h-8 text-xs"
          />
        </div>
      </div>

      <ScrollArea className="flex-1 px-2">
        <div className="space-y-1 pb-3">
          {Object.entries(grouped).map(([group, hosts]) => (
            <div key={group}>
              <button
                onClick={() => toggleGroup(group)}
                className="flex w-full items-center gap-1.5 rounded px-2 py-1.5 text-xs font-medium text-muted hover:text-foreground"
              >
                {collapsedGroups.has(group) ? (
                  <ChevronRight className="h-3 w-3" />
                ) : (
                  <ChevronDown className="h-3 w-3" />
                )}
                <Folder className="h-3 w-3" />
                {group}
                <span className="ml-auto text-[10px] opacity-60">
                  {hosts.length}
                </span>
              </button>
              {!collapsedGroups.has(group) &&
                hosts.map((profile) => (
                  <div
                    key={profile.id}
                    className="group flex items-center gap-1 rounded-md px-2 py-1.5 hover:bg-surface-hover"
                  >
                    <button
                      onClick={() => handleConnect(profile)}
                      className="flex flex-1 items-center gap-2 text-left text-sm min-w-0"
                    >
                      <Server className="h-3.5 w-3.5 shrink-0 text-accent" />
                      <div className="min-w-0">
                        <div className="truncate font-medium">{profile.name}</div>
                        <div className="truncate text-[11px] text-muted">
                          {profile.username}@{profile.host}
                        </div>
                      </div>
                    </button>
                    <div className="flex opacity-0 group-hover:opacity-100">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6"
                        onClick={() => onEditConnection(profile)}
                      >
                        <span className="text-[10px]">✎</span>
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6 text-danger"
                        onClick={() => removeProfile(profile.id)}
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  </div>
                ))}
            </div>
          ))}
          {filtered.length === 0 && (
            <p className="px-2 py-4 text-center text-xs text-muted">
              No hosts found
            </p>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
