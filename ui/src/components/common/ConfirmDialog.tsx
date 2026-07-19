import { AlertTriangle } from "lucide-react";
import Modal from "./Modal";
import Button from "./Button";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function ConfirmDialog({ open, title, message, confirmLabel = "Confirm", danger, onConfirm, onCancel }: ConfirmDialogProps) {
  return (
    <Modal open={open} onClose={onCancel}>
      <div className="flex items-start gap-3">
        {danger && (
          <span className="mt-0.5 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--color-status-error)]/12 text-[var(--color-status-error)]">
            <AlertTriangle className="h-4 w-4" strokeWidth={2} />
          </span>
        )}
        <div>
          <h3 className="text-sm font-semibold text-neutral-100">{title}</h3>
          <p className="mt-1 text-sm text-neutral-400">{message}</p>
        </div>
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="default" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant={danger ? "danger-primary" : "primary"} onClick={onConfirm}>
          {confirmLabel}
        </Button>
      </div>
    </Modal>
  );
}
