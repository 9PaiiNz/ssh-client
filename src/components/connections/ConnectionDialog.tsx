import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { keychainAvailable } from "@/lib/api";
import type { ConnectionProfile } from "@/lib/types";
import { useProfileStore } from "@/stores/useProfileStore";
import { useSessionStore } from "@/stores/useSessionStore";

interface ConnectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editProfile?: ConnectionProfile | null;
}

type AuthType = ConnectionProfile["auth"]["type"];

const emptyProfile = (): ConnectionProfile => ({
  id: crypto.randomUUID(),
  name: "",
  group: "",
  host: "",
  port: 22,
  username: "",
  auth: { type: "password" },
});

export function ConnectionDialog({
  open: isOpen,
  onOpenChange,
  editProfile,
}: ConnectionDialogProps) {
  const [profile, setProfile] = useState<ConnectionProfile>(emptyProfile());
  // Newly typed password / passphrase. Saved secrets are never sent to the UI;
  // an empty field keeps the saved one unless `clearSecret` is set.
  const [secret, setSecret] = useState("");
  const [clearSecret, setClearSecret] = useState(false);
  const [keychainOk, setKeychainOk] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const addOrUpdateProfile = useProfileStore((s) => s.addOrUpdateProfile);
  const openSession = useSessionStore((s) => s.openSession);

  useEffect(() => {
    if (isOpen) {
      setProfile(editProfile ? { ...editProfile, auth: { ...editProfile.auth } } : emptyProfile());
      setSecret("");
      setClearSecret(false);
      setError(null);
      keychainAvailable()
        .then(setKeychainOk)
        .catch(() => setKeychainOk(null));
    }
  }, [isOpen, editProfile]);

  const update = (partial: Partial<ConnectionProfile>) =>
    setProfile((p) => ({ ...p, ...partial }));

  const setAuthType = (type: AuthType) => {
    if (type === profile.auth.type) return;
    // Switching back to the saved type restores its saved-secret state.
    const auth =
      editProfile?.auth.type === type
        ? { ...editProfile.auth }
        : { type, keyPath: type === "key" ? "" : undefined };
    update({ auth });
    setSecret("");
    setClearSecret(false);
  };

  const setKeyPath = (keyPath: string) =>
    setProfile((p) => ({ ...p, auth: { ...p.auth, keyPath } }));

  const handlePickKey = async () => {
    // No extension filter — Tauri/rfd cannot show extensionless files (id_rsa, id_ed25519).
    const selected = await open({
      multiple: false,
      title: "Select SSH private key",
    });
    if (selected && typeof selected === "string") {
      setKeyPath(selected);
    }
  };

  const isKey = profile.auth.type === "key";
  const hasSaved = isKey ? !!profile.auth.hasPassphrase : !!profile.auth.hasPassword;
  const secretLabel = isKey ? "passphrase" : "password";

  const handleSave = async (andConnect: boolean) => {
    if (!profile.name || !profile.host || !profile.username) return;
    if (isKey && !profile.auth.keyPath?.trim()) return;

    // undefined = keep saved secret, "" = remove it, otherwise replace it.
    const newSecret = clearSecret ? "" : secret || undefined;
    const toSave: ConnectionProfile = {
      ...profile,
      auth: isKey
        ? { type: "key", keyPath: profile.auth.keyPath, keyPassphrase: newSecret }
        : { type: "password", password: newSecret },
    };

    setSaving(true);
    setError(null);
    try {
      await addOrUpdateProfile(toSave);
    } catch (err) {
      setError(`Could not save: ${err instanceof Error ? err.message : String(err)}`);
      return;
    } finally {
      setSaving(false);
    }
    onOpenChange(false);
    if (andConnect) {
      // Connection errors show up in the session tab.
      openSession(toSave).catch(() => {});
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {editProfile ? "Edit Connection" : "New Connection"}
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-4 py-2">
          <div className="grid gap-2">
            <Label htmlFor="name">Name</Label>
            <Input
              id="name"
              value={profile.name}
              onChange={(e) => update({ name: e.target.value })}
              placeholder="My Server"
            />
          </div>

          <div className="grid gap-2">
            <Label htmlFor="group">Group</Label>
            <Input
              id="group"
              value={profile.group ?? ""}
              onChange={(e) => update({ group: e.target.value })}
              placeholder="Production"
            />
          </div>

          <div className="grid grid-cols-3 gap-2">
            <div className="col-span-2 grid gap-2">
              <Label htmlFor="host">Host</Label>
              <Input
                id="host"
                value={profile.host}
                onChange={(e) => update({ host: e.target.value })}
                placeholder="192.168.1.1"
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="port">Port</Label>
              <Input
                id="port"
                type="number"
                value={profile.port}
                onChange={(e) => update({ port: parseInt(e.target.value) || 22 })}
              />
            </div>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="username">Username</Label>
            <Input
              id="username"
              value={profile.username}
              onChange={(e) => update({ username: e.target.value })}
              placeholder="root"
            />
          </div>

          <div className="grid gap-2">
            <Label>Authentication</Label>
            <div className="flex gap-2">
              <Button
                type="button"
                variant={!isKey ? "default" : "secondary"}
                size="sm"
                onClick={() => setAuthType("password")}
              >
                Password
              </Button>
              <Button
                type="button"
                variant={isKey ? "default" : "secondary"}
                size="sm"
                onClick={() => setAuthType("key")}
              >
                SSH Key
              </Button>
            </div>
          </div>

          {isKey && (
            <div className="grid gap-2">
              <Label>Private Key</Label>
              <div className="flex gap-2">
                <Input
                  value={profile.auth.keyPath ?? ""}
                  onChange={(e) => setKeyPath(e.target.value)}
                  placeholder="~/.ssh/id_ed25519"
                  className="flex-1"
                />
                <Button type="button" variant="secondary" onClick={handlePickKey}>
                  Browse
                </Button>
              </div>
              <p className="text-[11px] text-muted">
                Supports id_rsa, id_ed25519, .pem, and other OpenSSH private keys.
              </p>
            </div>
          )}

          <div className="grid gap-2">
            <Label htmlFor="secret">
              {isKey ? "Key Passphrase (optional)" : "Password"}
            </Label>
            <Input
              id="secret"
              type="password"
              autoComplete="off"
              value={secret}
              placeholder={
                hasSaved && !clearSecret ? `Saved — leave empty to keep` : undefined
              }
              onChange={(e) => {
                setSecret(e.target.value);
                setClearSecret(false);
              }}
            />
            {hasSaved && !secret && (
              <p className="text-[11px] text-muted">
                {clearSecret ? (
                  <>
                    The saved {secretLabel} will be removed.{" "}
                    <button
                      type="button"
                      className="text-accent hover:underline"
                      onClick={() => setClearSecret(false)}
                    >
                      Undo
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="text-accent hover:underline"
                    onClick={() => setClearSecret(true)}
                  >
                    Remove saved {secretLabel}
                  </button>
                )}
              </p>
            )}
            {keychainOk === true && (
              <p className="text-[11px] text-muted">
                Saved securely in your system keychain.
              </p>
            )}
            {keychainOk === false && (
              <p className="text-[11px] text-danger">
                No system keychain found (e.g. GNOME Keyring or KWallet). The{" "}
                {secretLabel} will be saved in a file only your user account can
                read.
              </p>
            )}
          </div>

          {error && <p className="text-sm text-danger">{error}</p>}
        </div>

        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="secondary"
            disabled={saving}
            onClick={() => handleSave(false)}
          >
            Save
          </Button>
          <Button disabled={saving} onClick={() => handleSave(true)}>
            {editProfile ? "Save & Connect" : "Connect"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
