import { useCallback, useEffect, useRef, useState } from "react";
import { History, Trash2 } from "lucide-react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import {
  TERMINAL_BACKGROUND,
  terminalOptions,
} from "@/components/terminal/terminalOptions";
import {
  deleteHistory,
  historyEnabled,
  listHistory,
  readHistory,
  setHistoryEnabled,
} from "@/lib/api";
import type { ConnectionProfile, HistoryEntry } from "@/lib/types";
import { cn } from "@/lib/utils";

interface HistoryDialogProps {
  profile: ConnectionProfile | null;
  onOpenChange: (open: boolean) => void;
}

const END_REASON: Record<HistoryEntry["endReason"], string> = {
  closed: "Tab closed",
  disconnected: "Disconnected",
  appExit: "App closed",
};

/** Saved terminal output of a host's ended sessions. */
export function HistoryDialog({ profile, onOpenChange }: HistoryDialogProps) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const profileId = profile?.id;

  const refresh = useCallback(async () => {
    if (!profileId) return;
    try {
      const list = await listHistory(profileId);
      setEntries(list);
      setSelectedId((cur) =>
        cur && list.some((e) => e.id === cur) ? cur : (list[0]?.id ?? null)
      );
    } catch (err) {
      setError(String(err));
    }
  }, [profileId]);

  useEffect(() => {
    if (!profileId) return;
    setEntries([]);
    setSelectedId(null);
    setError(null);
    refresh();
    historyEnabled().then(setEnabled).catch(() => {});
  }, [profileId, refresh]);

  const toggleEnabled = async () => {
    try {
      await setHistoryEnabled(!enabled);
      setEnabled(!enabled);
    } catch (err) {
      setError(String(err));
    }
  };

  const remove = async (entryId?: string) => {
    if (!profileId) return;
    try {
      await deleteHistory(profileId, entryId);
      await refresh();
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <Dialog open={!!profile} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[80vh] max-w-5xl flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <History className="h-4 w-4 text-accent" />
            Session history — {profile?.name}
          </DialogTitle>
          <DialogDescription>
            Terminal output of your last {entries.length > 0 ? entries.length : ""}{" "}
            sessions on this host, saved when the session ended.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-1 gap-3">
          <div className="flex w-56 shrink-0 flex-col gap-1 overflow-y-auto">
            {entries.map((e) => (
              <button
                key={e.id}
                onClick={() => setSelectedId(e.id)}
                className={cn(
                  "group flex items-start gap-2 rounded-md px-2 py-1.5 text-left text-xs",
                  e.id === selectedId
                    ? "bg-surface-hover text-foreground"
                    : "text-muted hover:bg-surface-hover hover:text-foreground"
                )}
              >
                <div className="min-w-0 flex-1">
                  <div className="font-medium text-foreground">
                    {new Date(e.endedAt).toLocaleString()}
                  </div>
                  <div className="truncate">
                    {END_REASON[e.endReason]} · {formatDuration(e.endedAt - e.startedAt)}{" "}
                    · {formatSize(e.size)}
                  </div>
                </div>
                <span
                  role="button"
                  title="Delete"
                  className="rounded p-0.5 text-danger opacity-0 hover:bg-surface group-hover:opacity-100"
                  onClick={(ev) => {
                    ev.stopPropagation();
                    remove(e.id);
                  }}
                >
                  <Trash2 className="h-3 w-3" />
                </span>
              </button>
            ))}
            {entries.length === 0 && (
              <p className="px-2 py-4 text-center text-xs text-muted">
                No saved sessions yet. Output is saved when a session ends.
              </p>
            )}
          </div>

          <div className="min-w-0 flex-1 overflow-hidden rounded-md border border-border">
            {profileId && selectedId ? (
              <HistoryViewer profileId={profileId} entryId={selectedId} />
            ) : (
              <div
                className="h-full"
                style={{ background: TERMINAL_BACKGROUND }}
              />
            )}
          </div>
        </div>

        {error && <p className="text-sm text-danger">{error}</p>}

        <div className="flex items-center justify-between gap-2">
          <label className="flex cursor-pointer items-center gap-2 text-xs text-muted">
            <input
              type="checkbox"
              checked={enabled}
              onChange={toggleEnabled}
              className="accent-accent"
            />
            Save session output (all hosts). It may contain sensitive data, and
            is stored only for your user account.
          </label>
          <Button
            variant="danger"
            size="sm"
            disabled={entries.length === 0}
            onClick={() => remove()}
          >
            Clear history
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Read-only terminal showing one saved session. */
function HistoryViewer({
  profileId,
  entryId,
}: {
  profileId: string;
  entryId: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;
    const terminal = new Terminal({
      ...terminalOptions,
      disableStdin: true,
      cursorBlink: false,
      cursorInactiveStyle: "none",
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(container);

    const fit = () => {
      if (container.clientWidth > 0 && container.clientHeight > 0) fitAddon.fit();
    };
    fit();
    const resizeObserver = new ResizeObserver(fit);
    resizeObserver.observe(container);

    let disposed = false;
    readHistory(profileId, entryId)
      .then((text) => {
        if (disposed) return;
        terminal.write(text, () => terminal.scrollToBottom());
      })
      .catch((err) => {
        if (!disposed) terminal.writeln(`\x1b[31m${String(err)}\x1b[0m`);
      });

    return () => {
      disposed = true;
      resizeObserver.disconnect();
      terminal.dispose();
    };
  }, [profileId, entryId]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full p-1"
      style={{ background: TERMINAL_BACKGROUND }}
    />
  );
}

function formatDuration(ms: number): string {
  const min = Math.round(ms / 60000);
  if (min < 1) return "<1 min";
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  return `${h} h ${min % 60} min`;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
