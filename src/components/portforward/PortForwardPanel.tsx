import { useEffect, useState } from "react";
import { Plus, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { PortForwardRule } from "@/lib/types";
import {
  addPortForward,
  listPortForwards,
  removePortForward,
} from "@/lib/api";

interface PortForwardPanelProps {
  sessionId: string;
  open: boolean;
  onClose: () => void;
}

export function PortForwardPanel({
  sessionId,
  open,
  onClose,
}: PortForwardPanelProps) {
  const [rules, setRules] = useState<PortForwardRule[]>([]);
  const [forwardType, setForwardType] = useState<"local" | "remote">("local");
  const [bindHost, setBindHost] = useState("127.0.0.1");
  const [bindPort, setBindPort] = useState("8080");
  const [targetHost, setTargetHost] = useState("127.0.0.1");
  const [targetPort, setTargetPort] = useState("80");
  const [adding, setAdding] = useState(false);

  const loadRules = async () => {
    try {
      const list = await listPortForwards(sessionId);
      setRules(list);
    } catch {
      setRules([]);
    }
  };

  useEffect(() => {
    if (open) loadRules();
  }, [open, sessionId]);

  const handleAdd = async () => {
    setAdding(true);
    try {
      await addPortForward(
        sessionId,
        forwardType,
        bindHost,
        parseInt(bindPort) || 0,
        targetHost,
        parseInt(targetPort) || 0
      );
      await loadRules();
    } finally {
      setAdding(false);
    }
  };

  const handleRemove = async (ruleId: string) => {
    await removePortForward(sessionId, ruleId);
    await loadRules();
  };

  if (!open) return null;

  return (
    <div className="absolute right-4 top-12 z-40 w-96 rounded-lg border border-border bg-surface shadow-xl">
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <span className="text-sm font-semibold">Port Forwarding</span>
        <Button variant="ghost" size="icon" className="h-7 w-7" onClick={onClose}>
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>

      <div className="p-4 space-y-3">
        <div className="flex gap-2">
          <Button
            size="sm"
            variant={forwardType === "local" ? "default" : "secondary"}
            onClick={() => setForwardType("local")}
          >
            Local
          </Button>
          <Button
            size="sm"
            variant={forwardType === "remote" ? "default" : "secondary"}
            onClick={() => setForwardType("remote")}
          >
            Remote
          </Button>
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div className="grid gap-1">
            <Label className="text-xs">Bind Host</Label>
            <Input
              value={bindHost}
              onChange={(e) => setBindHost(e.target.value)}
              className="h-8 text-xs"
            />
          </div>
          <div className="grid gap-1">
            <Label className="text-xs">Bind Port</Label>
            <Input
              value={bindPort}
              onChange={(e) => setBindPort(e.target.value)}
              className="h-8 text-xs"
            />
          </div>
          <div className="grid gap-1">
            <Label className="text-xs">Target Host</Label>
            <Input
              value={targetHost}
              onChange={(e) => setTargetHost(e.target.value)}
              className="h-8 text-xs"
            />
          </div>
          <div className="grid gap-1">
            <Label className="text-xs">Target Port</Label>
            <Input
              value={targetPort}
              onChange={(e) => setTargetPort(e.target.value)}
              className="h-8 text-xs"
            />
          </div>
        </div>

        <Button size="sm" className="w-full" disabled={adding} onClick={handleAdd}>
          <Plus className="h-3.5 w-3.5 mr-1" />
          Add Rule
        </Button>

        {rules.length > 0 && (
          <div className="space-y-1 pt-2 border-t border-border">
            {rules.map((rule) => (
              <div
                key={rule.id}
                className="flex items-center gap-2 rounded px-2 py-1.5 text-xs hover:bg-surface-hover"
              >
                <span className="rounded bg-surface-hover px-1.5 py-0.5 text-[10px] uppercase text-accent">
                  {rule.forwardType}
                </span>
                <span className="flex-1 truncate">
                  {rule.bindHost}:{rule.bindPort} → {rule.targetHost}:
                  {rule.targetPort}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 text-danger"
                  onClick={() => handleRemove(rule.id)}
                >
                  <Trash2 className="h-3 w-3" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
