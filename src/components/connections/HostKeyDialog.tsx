import { useState } from "react";
import { ShieldAlert, ShieldQuestion } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useSessionStore } from "@/stores/useSessionStore";

/** Asks the user to confirm an unknown server host key, or warns about a changed one. */
export function HostKeyDialog() {
  const prompt = useSessionStore((s) => s.hostKeyPrompt);
  const resolve = useSessionStore((s) => s.resolveHostKeyPrompt);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!prompt) return null;
  const { info, changed } = prompt;
  const hostLabel = info.port === 22 ? info.host : `${info.host}:${info.port}`;

  const answer = async (trust: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await resolve(trust);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && !busy && answer(false)}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {changed ? (
              <ShieldAlert className="h-5 w-5 text-danger" />
            ) : (
              <ShieldQuestion className="h-5 w-5 text-accent" />
            )}
            {changed ? "Host key has changed!" : "Unknown host"}
          </DialogTitle>
          <DialogDescription>
            {changed ? (
              <>
                The server at <b>{hostLabel}</b> presented a different key than
                the one saved for it. Someone may be intercepting the connection
                (man-in-the-middle attack), or the server was reinstalled.
                Only continue if you know why the key changed.
              </>
            ) : (
              <>
                This is the first connection to <b>{hostLabel}</b>. Check that
                the fingerprint below matches the server before trusting it.
                On the server you can run{" "}
                <code className="text-foreground">
                  ssh-keygen -lf /etc/ssh/ssh_host_{keyFileName(info.keyType)}_key.pub
                </code>
                .
              </>
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-1 rounded-md bg-background p-3 text-xs">
          <span className="text-muted">{info.keyType} key fingerprint</span>
          <code className="break-all font-mono text-sm text-foreground select-all">
            {info.fingerprint}
          </code>
        </div>

        {error && <p className="text-sm text-danger">{error}</p>}

        <div className="flex justify-end gap-2">
          <Button variant="secondary" disabled={busy} onClick={() => answer(false)}>
            Cancel
          </Button>
          <Button
            variant={changed ? "danger" : "default"}
            disabled={busy}
            onClick={() => answer(true)}
          >
            {changed ? "Replace saved key & connect" : "Trust & connect"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Map an SSH key algorithm to the usual host key file name part. */
function keyFileName(keyType: string): string {
  if (keyType.includes("ed25519")) return "ed25519";
  if (keyType.includes("ecdsa")) return "ecdsa";
  if (keyType.includes("rsa")) return "rsa";
  return "*";
}
