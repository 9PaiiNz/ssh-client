import { useCallback, useEffect, useRef, useState } from "react";
import {
  ChevronRight,
  Download,
  File,
  Folder,
  FolderOpen,
  FolderUp,
  RefreshCw,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { open as openDialog, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { SftpEntry, TransferProgress } from "@/lib/types";
import {
  deleteSftpPath,
  downloadSftpDir,
  downloadSftpFile,
  listSftpDir,
  onTransferProgress,
  uploadSftpPaths,
} from "@/lib/api";
import { useSessionStore } from "@/stores/useSessionStore";
import { cn } from "@/lib/utils";

interface SftpDrawerProps {
  sessionId: string;
  isOpen: boolean;
  onClose: () => void;
}

interface MenuState {
  x: number;
  y: number;
  /** The right-clicked entry, or null for the empty area. */
  entry: SftpEntry | null;
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Dialog results come back as a string, an array or null. */
function toPaths(selected: string | string[] | null): string[] {
  if (!selected) return [];
  return Array.isArray(selected) ? selected : [selected];
}

export function SftpDrawer({ sessionId, isOpen, onClose }: SftpDrawerProps) {
  const [currentPath, setCurrentPath] = useState("/");
  const [entries, setEntries] = useState<SftpEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [dragging, setDragging] = useState(false);
  /** Folder row under the cursor while dragging files in. */
  const [dropDir, setDropDir] = useState<string | null>(null);
  const [toDelete, setToDelete] = useState<SftpEntry | null>(null);
  const [deleting, setDeleting] = useState(false);
  const transfers = useSessionStore((s) => s.transfers);
  const addTransfer = useSessionStore((s) => s.addTransfer);
  const updateTransfer = useSessionStore((s) => s.updateTransfer);
  const clearFinishedTransfers = useSessionStore((s) => s.clearFinishedTransfers);

  const currentPathRef = useRef(currentPath);
  currentPathRef.current = currentPath;

  const loadDir = useCallback(
    async (path: string) => {
      setLoading(true);
      setError(null);
      try {
        const items = await listSftpDir(sessionId, path);
        setEntries(items);
        setCurrentPath(path);
      } catch (err) {
        setError(`Could not open ${path}: ${errorText(err)}`);
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
    let disposed = false;
    onTransferProgress((progress) => {
      const existing = useSessionStore
        .getState()
        .transfers.find((t) => t.transferId === progress.transferId);
      if (existing) updateTransfer(progress);
      else addTransfer(progress);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [addTransfer, updateTransfer]);

  const upload = useCallback(
    async (localPaths: string[], remoteDir: string) => {
      if (localPaths.length === 0) return;
      setError(null);
      try {
        await uploadSftpPaths(sessionId, localPaths, remoteDir);
      } catch (err) {
        setError(`Upload failed: ${errorText(err)}`);
      }
      if (remoteDir === currentPathRef.current) loadDir(remoteDir);
    },
    [sessionId, loadDir]
  );

  // Files and folders dragged in from the file manager. Dropping on a folder
  // row uploads into that folder, anywhere else into the current folder.
  const uploadRef = useRef(upload);
  uploadRef.current = upload;
  useEffect(() => {
    if (!isOpen) return;
    const folderAt = (pos: { x: number; y: number }) => {
      const dpr = window.devicePixelRatio || 1;
      const el = document.elementFromPoint(pos.x / dpr, pos.y / dpr);
      return el?.closest<HTMLElement>("[data-drop-dir]")?.dataset.dropDir ?? null;
    };
    let unlisten: (() => void) | undefined;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (payload.type === "enter" || payload.type === "over") {
          setDragging(true);
          setDropDir(folderAt(payload.position));
        } else if (payload.type === "drop") {
          const target = folderAt(payload.position) ?? currentPathRef.current;
          setDragging(false);
          setDropDir(null);
          uploadRef.current(payload.paths, target);
        } else {
          setDragging(false);
          setDropDir(null);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isOpen]);

  // Close the context menu on outside click, Escape, scroll or resize.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    window.addEventListener("wheel", close, { passive: true });
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
      window.removeEventListener("wheel", close);
      window.removeEventListener("blur", close);
    };
  }, [menu]);

  const navigateUp = () => {
    if (currentPath === "/") return;
    const parts = currentPath.split("/").filter(Boolean);
    parts.pop();
    loadDir("/" + parts.join("/"));
  };

  const handleDownload = async (entry: SftpEntry) => {
    setError(null);
    try {
      if (entry.isDir) {
        const dir = await openDialog({
          directory: true,
          title: `Download folder "${entry.name}" into…`,
        });
        if (!dir || typeof dir !== "string") return;
        await downloadSftpDir(sessionId, entry.path, dir);
      } else {
        const localPath = await save({ defaultPath: entry.name });
        if (!localPath) return;
        await downloadSftpFile(sessionId, entry.path, localPath);
      }
    } catch (err) {
      setError(`Download failed: ${errorText(err)}`);
    }
  };

  const confirmDelete = async () => {
    if (!toDelete) return;
    setDeleting(true);
    setError(null);
    try {
      await deleteSftpPath(sessionId, toDelete.path);
    } catch (err) {
      setError(`Could not delete ${toDelete.name}: ${errorText(err)}`);
    } finally {
      setDeleting(false);
      setToDelete(null);
      loadDir(currentPathRef.current);
    }
  };

  const pickAndUpload = async (remoteDir: string, folders: boolean) => {
    const selected = await openDialog({
      multiple: true,
      directory: folders,
      title: folders ? "Select folders to upload" : "Select files to upload",
    });
    await upload(toPaths(selected), remoteDir);
  };

  const openMenu = (e: React.MouseEvent, entry: SftpEntry | null) => {
    e.preventDefault();
    e.stopPropagation();
    // Keep the menu inside the window.
    const x = Math.min(e.clientX, window.innerWidth - 200);
    const y = Math.min(e.clientY, window.innerHeight - (entry?.isDir ? 200 : 100));
    setMenu({ x, y, entry });
  };

  const pathParts = currentPath.split("/").filter(Boolean);
  const sessionTransfers = transfers.filter((t) => t.sessionId === sessionId);
  const hasFinished = sessionTransfers.some((t) => t.status !== "in_progress");

  if (!isOpen) return null;

  return (
    <div className="relative flex h-full w-80 shrink-0 flex-col border-l border-border bg-surface">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-semibold">SFTP</span>
        <div className="flex gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title="Upload files"
            onClick={() => pickAndUpload(currentPath, false)}
          >
            <Upload className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title="Upload folder"
            onClick={() => pickAndUpload(currentPath, true)}
          >
            <FolderUp className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title="Refresh"
            onClick={() => loadDir(currentPath)}
          >
            <RefreshCw className="h-3.5 w-3.5" />
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

      {error && (
        <div className="flex items-start gap-2 border-b border-border bg-danger/10 px-3 py-1.5 text-[11px] text-danger">
          <span className="flex-1 break-words">{error}</span>
          <button onClick={() => setError(null)} className="shrink-0 hover:opacity-70">
            <X className="h-3 w-3" />
          </button>
        </div>
      )}

      <ScrollArea className="flex-1">
        <div
          className="min-h-full"
          onContextMenu={(e) => openMenu(e, null)}
        >
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
                  data-drop-dir={entry.isDir ? entry.path : undefined}
                  onContextMenu={(e) => openMenu(e, entry)}
                  className={cn(
                    "group flex items-center gap-2 rounded px-2 py-1.5 hover:bg-surface-hover",
                    dropDir === entry.path && "bg-accent/20 ring-1 ring-accent",
                    menu?.entry?.path === entry.path && "bg-surface-hover"
                  )}
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
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 opacity-0 group-hover:opacity-100"
                    title={entry.isDir ? "Download folder" : "Download"}
                    onClick={() => handleDownload(entry)}
                  >
                    <Download className="h-3 w-3" />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      </ScrollArea>

      {sessionTransfers.length > 0 && (
        <div className="border-t border-border p-2">
          {hasFinished && (
            <div className="mb-1 flex justify-end">
              <button
                className="text-[10px] text-muted hover:text-foreground"
                onClick={() => clearFinishedTransfers(sessionId)}
              >
                Clear finished
              </button>
            </div>
          )}
          <div className="max-h-28 space-y-1 overflow-y-auto">
            {sessionTransfers.map((t) => (
              <TransferItem key={t.transferId} transfer={t} />
            ))}
          </div>
        </div>
      )}

      {dragging && (
        // pointer-events-none so folder rows below can still be hit-tested.
        <div className="pointer-events-none absolute inset-0 z-10 flex items-end justify-center border-2 border-dashed border-accent bg-accent/5 p-4">
          <span className="rounded bg-surface px-2 py-1 text-xs text-foreground shadow">
            Drop to upload to <b>{dropDir ?? currentPath}</b>
          </span>
        </div>
      )}

      {menu && (
        <div
          className="fixed z-50 min-w-[180px] rounded-md border border-border bg-surface py-1 text-sm shadow-xl"
          style={{ left: menu.x, top: menu.y }}
          onMouseDown={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          {menu.entry ? (
            <>
              {menu.entry.isDir && (
                <MenuItem
                  icon={<FolderOpen className="h-3.5 w-3.5" />}
                  label="Open"
                  onClick={() => {
                    setMenu(null);
                    loadDir(menu.entry!.path);
                  }}
                />
              )}
              <MenuItem
                icon={<Download className="h-3.5 w-3.5" />}
                label={menu.entry.isDir ? "Download folder…" : "Download…"}
                onClick={() => {
                  setMenu(null);
                  handleDownload(menu.entry!);
                }}
              />
              {menu.entry.isDir && (
                <>
                  <MenuItem
                    icon={<Upload className="h-3.5 w-3.5" />}
                    label="Upload files here…"
                    onClick={() => {
                      setMenu(null);
                      pickAndUpload(menu.entry!.path, false);
                    }}
                  />
                  <MenuItem
                    icon={<FolderUp className="h-3.5 w-3.5" />}
                    label="Upload folder here…"
                    onClick={() => {
                      setMenu(null);
                      pickAndUpload(menu.entry!.path, true);
                    }}
                  />
                </>
              )}
              <div className="my-1 h-px bg-border" />
              <MenuItem
                icon={<Trash2 className="h-3.5 w-3.5" />}
                label="Delete…"
                danger
                onClick={() => {
                  setMenu(null);
                  setToDelete(menu.entry);
                }}
              />
            </>
          ) : (
            <>
              <MenuItem
                icon={<Upload className="h-3.5 w-3.5" />}
                label="Upload files…"
                onClick={() => {
                  setMenu(null);
                  pickAndUpload(currentPath, false);
                }}
              />
              <MenuItem
                icon={<FolderUp className="h-3.5 w-3.5" />}
                label="Upload folder…"
                onClick={() => {
                  setMenu(null);
                  pickAndUpload(currentPath, true);
                }}
              />
              <MenuItem
                icon={<RefreshCw className="h-3.5 w-3.5" />}
                label="Refresh"
                onClick={() => {
                  setMenu(null);
                  loadDir(currentPath);
                }}
              />
            </>
          )}
        </div>
      )}

      <Dialog
        open={!!toDelete}
        onOpenChange={(open) => !open && !deleting && setToDelete(null)}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Trash2 className="h-4 w-4 text-danger" />
              Delete {toDelete?.isDir ? "folder" : "file"}?
            </DialogTitle>
            <DialogDescription>
              {toDelete?.isDir ? (
                <>
                  <b className="text-foreground">{toDelete?.path}</b> and{" "}
                  <b>everything inside it</b> will be permanently deleted from
                  the server. This cannot be undone.
                </>
              ) : (
                <>
                  <b className="text-foreground">{toDelete?.path}</b> will be
                  permanently deleted from the server. This cannot be undone.
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          <div className="flex justify-end gap-2">
            <Button
              variant="secondary"
              disabled={deleting}
              onClick={() => setToDelete(null)}
            >
              Cancel
            </Button>
            <Button variant="danger" disabled={deleting} onClick={confirmDelete}>
              {deleting ? "Deleting…" : "Delete"}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function MenuItem({
  icon,
  label,
  onClick,
  danger,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      className={cn(
        "flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-surface-hover",
        danger && "text-danger"
      )}
      onClick={onClick}
    >
      <span className={danger ? "text-danger" : "text-muted"}>{icon}</span>
      {label}
    </button>
  );
}

function TransferItem({ transfer }: { transfer: TransferProgress }) {
  const pct =
    transfer.totalBytes > 0
      ? Math.round((transfer.bytesTransferred / transfer.totalBytes) * 100)
      : 0;

  return (
    <div className="text-[10px]" title={transfer.error}>
      <div className="flex justify-between gap-2 text-muted">
        <span className="truncate">
          {transfer.direction === "upload" ? "↑" : "↓"} {transfer.fileName}
        </span>
        <span
          className={cn(
            "shrink-0",
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
      {transfer.status === "failed" && transfer.error && (
        <div className="truncate text-danger">{transfer.error}</div>
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
