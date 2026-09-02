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
import type { ConnectionProfile } from "@/lib/types";
import { useProfileStore } from "@/stores/useProfileStore";
import { useSessionStore } from "@/stores/useSessionStore";

interface ConnectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editProfile?: ConnectionProfile | null;
}

const emptyProfile = (): ConnectionProfile => ({
  id: crypto.randomUUID(),
  name: "",
  group: "",
  host: "",
  port: 22,
  username: "",
  auth: { type: "password", password: "" },
});

export function ConnectionDialog({
  open: isOpen,
  onOpenChange,
  editProfile,
}: ConnectionDialogProps) {
  const [profile, setProfile] = useState<ConnectionProfile>(emptyProfile());
  const [connectAfterSave, setConnectAfterSave] = useState(false);
  const [saving, setSaving] = useState(false);
  const addOrUpdateProfile = useProfileStore((s) => s.addOrUpdateProfile);
  const openSession = useSessionStore((s) => s.openSession);

  useEffect(() => {
    if (isOpen) {
      setProfile(editProfile ? { ...editProfile } : emptyProfile());
      setConnectAfterSave(!editProfile);
    }
  }, [isOpen, editProfile]);

  const update = (partial: Partial<ConnectionProfile>) =>
    setProfile((p) => ({ ...p, ...partial }));

  const handlePickKey = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "SSH Key", extensions: ["pem", "key", "pub", ""] }],
    });
    if (selected && typeof selected === "string") {
      update({
        auth: { ...profile.auth, type: "key", keyPath: selected },
      });
    }
  };

  const handleSave = async (andConnect: boolean) => {
    if (!profile.name || !profile.host || !profile.username) return;
    setSaving(true);
    try {
      await addOrUpdateProfile(profile);
      if (andConnect) {
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
      }
      onOpenChange(false);
    } finally {
      setSaving(false);
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
                variant={profile.auth.type === "password" ? "default" : "secondary"}
                size="sm"
                onClick={() =>
                  update({ auth: { type: "password", password: "" } })
                }
              >
                Password
              </Button>
              <Button
                type="button"
                variant={profile.auth.type === "key" ? "default" : "secondary"}
                size="sm"
                onClick={() =>
                  update({ auth: { type: "key", keyPath: "" } })
                }
              >
                SSH Key
              </Button>
            </div>
          </div>

          {profile.auth.type === "password" ? (
            <div className="grid gap-2">
              <Label htmlFor="password">Password</Label>
              <Input
                id="password"
                type="password"
                value={profile.auth.password ?? ""}
                onChange={(e) =>
                  update({
                    auth: { ...profile.auth, password: e.target.value },
                  })
                }
              />
            </div>
          ) : (
            <>
              <div className="grid gap-2">
                <Label>Private Key</Label>
                <div className="flex gap-2">
                  <Input
                    value={profile.auth.keyPath ?? ""}
                    onChange={(e) =>
                      update({
                        auth: { ...profile.auth, keyPath: e.target.value },
                      })
                    }
                    placeholder="~/.ssh/id_rsa"
                    className="flex-1"
                  />
                  <Button type="button" variant="secondary" onClick={handlePickKey}>
                    Browse
                  </Button>
                </div>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="passphrase">Key Passphrase (optional)</Label>
                <Input
                  id="passphrase"
                  type="password"
                  value={profile.auth.keyPassphrase ?? ""}
                  onChange={(e) =>
                    update({
                      auth: {
                        ...profile.auth,
                        keyPassphrase: e.target.value,
                      },
                    })
                  }
                />
              </div>
            </>
          )}
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
          <Button
            disabled={saving}
            onClick={() => handleSave(connectAfterSave || !editProfile)}
          >
            {editProfile ? "Save & Connect" : "Connect"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
