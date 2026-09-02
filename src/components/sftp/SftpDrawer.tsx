import { useCallback, useEffect, useState } from "react";
import {
  ChevronRight,
  Download,
  File,
  Folder,
  FolderOpen,
  Upload,
  X,
} from "lucide-react";
import { open as openDialog, save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { SftpEntry, TransferProgress } from "@/lib/types";
import {
  downloadSftpFile,
  listSftpDir,
  onTransferProgress,
  uploadSftpFile,
} from "@/lib/api";
import { useSessionStore } from "@/stores/useSessionStore";
import { cn } from "@/lib/utils";

interface SftpDrawerProps {
  sessionId: string;
  isOpen: boolean;
  onClose: () => void;
}

export function SftpDrawer({ sessionId, isOpen, onClose }: SftpDrawerProps) {
  const [currentPath, setCurrentPath] = useState("/");
  const [entries, setEntries] = useState<SftpEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const transfers = useSessionStore((s) => s.transfers);
  const addTransfer = useSessionStore((s) => s.addTransfer);
  const updateTransfer = useSessionStore((s) => s.updateTransfer);

  const loadDir = useCallback(
    async (path: string) => {
      setLoading(true);
      try {
        const items = await listSftpDir(sessionId, path);
        setEntries(items);
        setCurrentPath(path);
      } catch (err) {
        console.error("SFTP list error:", err);
      } finally {
        setLoading(false);
      }
    },
    [sessionId]
  );

  useEffect(() => {
    if (isOpen) loadDir("/");
  }, [isOpen, loadDir]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onTransferProgress((progress) => {
      const existing = useSessionStore
        .getState()
        .transfers.find((t) => t.transferId === progress.transferId);
      if (existing) updateTransfer(progress);
      else addTransfer(progress);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [addTransfer, updateTransfer]);

  const navigateUp = () => {
    if (currentPath === "/") return;
    const parts = currentPath.split("/").filter(Boolean);
    parts.pop();
    loadDir("/" + parts.join("/"));
  };

  const handleDownload = async (entry: SftpEntry) => {
    const localPath = await save({
      defaultPath: entry.name,
    });
    if (!localPath) return;
    try {
      await downloadSftpFile(sessionId, entry.path, localPath);
    } catch (err) {
      console.error("Download error:", err);
    }
  };

  const handleUpload = async () => {
    const localPath = await openDialog({ multiple: false });
    if (!localPath || typeof localPath !== "string") return;
    const fileName = localPath.split(/[/\\]/).pop() ?? "upload";
    const remotePath =
      currentPath === "/"
        ? `/${fileName}`
        : `${currentPath}/${fileName}`;
    try {
      await uploadSftpFile(sessionId, localPath, remotePath);
      loadDir(currentPath);
    } catch (err) {
      console.error("Upload error:", err);
    }
  };

  const pathParts = currentPath.split("/").filter(Boolean);
  const sessionTransfers = transfers.filter((t) => t.sessionId === sessionId);

  if (!isOpen) return null;

  return (
    <div className="flex h-full w-80 shrink-0 flex-col border-l border-border bg-surface">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-semibold">SFTP</span>
        <div className="flex gap-1">
          <Button variant="ghost" size="icon" className="h-7 w-7" onClick={handleUpload}>
            <Upload className="h-3.5 w-3.5" />
          </Button>
          <Button variant="ghost" size="icon" className="h-7 w-7" onClick={onClose}>
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      <div className="flex items-center gap-1 border-b border-border px-3 py-1.5 text-xs text-muted overflow-x-auto">
        <button
          onClick={() => loadDir("/")}
          className="hover:text-foreground shrink-0"
        >
          /
        </button>
        {pathParts.map((part, i) => (
          <span key={i} className="flex items-center gap-1 shrink-0">
            <ChevronRight className="h-3 w-3" />
            <button
              onClick={() =>
                loadDir("/" + pathParts.slice(0, i + 1).join("/"))
              }
              className="hover:text-foreground"
            >
              {part}
            </button>
          </span>
        ))}
      </div>

      <ScrollArea className="flex-1">
        {loading ? (
          <p className="p-4 text-xs text-muted">Loading...</p>
        ) : (
          <div className="p-1">
            {currentPath !== "/" && (
              <button
                onClick={navigateUp}
                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-sm hover:bg-surface-hover"
              >
                <FolderOpen className="h-4 w-4 text-muted" />
                ..
              </button>
            )}
            {entries.map((entry) => (
              <div
                key={entry.path}
                className="group flex items-center gap-2 rounded px-2 py-1.5 hover:bg-surface-hover"
              >
                <button
                  className="flex flex-1 items-center gap-2 text-left text-sm min-w-0"
                  onClick={() =>
                    entry.isDir ? loadDir(entry.path) : handleDownload(entry)
                  }
                >
                  {entry.isDir ? (
                    <Folder className="h-4 w-4 shrink-0 text-accent" />
                  ) : (
                    <File className="h-4 w-4 shrink-0 text-muted" />
                  )}
                  <span className="truncate">{entry.name}</span>
                  {!entry.isDir && (
                    <span className="ml-auto text-[10px] text-muted shrink-0">
                      {formatSize(entry.size)}
                    </span>
                  )}
                </button>
                {!entry.isDir && (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 opacity-0 group-hover:opacity-100"
                    onClick={() => handleDownload(entry)}
                  >
                    <Download className="h-3 w-3" />
                  </Button>
                )}
              </div>
            ))}
          </div>
        )}
      </ScrollArea>

      {sessionTransfers.length > 0 && (
        <div className="border-t border-border p-2 space-y-1 max-h-24 overflow-y-auto">
          {sessionTransfers.map((t) => (
            <TransferItem key={t.transferId} transfer={t} />
          ))}
        </div>
      )}
    </div>
  );
}

function TransferItem({ transfer }: { transfer: TransferProgress }) {
  const pct =
    transfer.totalBytes > 0
      ? Math.round((transfer.bytesTransferred / transfer.totalBytes) * 100)
      : 0;

  return (
    <div className="text-[10px]">
      <div className="flex justify-between text-muted">
        <span className="truncate">
          {transfer.direction === "upload" ? "↑" : "↓"} {transfer.fileName}
        </span>
        <span
          className={cn(
            transfer.status === "failed" && "text-danger",
            transfer.status === "completed" && "text-accent"
          )}
        >
          {transfer.status === "completed"
            ? "Done"
            : transfer.status === "failed"
              ? "Failed"
              : `${pct}%`}
        </span>
      </div>
      {transfer.status === "in_progress" && (
        <div className="mt-0.5 h-1 rounded-full bg-border">
          <div
            className="h-full rounded-full bg-accent transition-all"
            style={{ width: `${pct}%` }}
          />
        </div>
      )}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
