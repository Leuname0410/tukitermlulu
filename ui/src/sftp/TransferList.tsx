import { useEffect, useState, type JSX } from "react";
import {
  cancelTransfer,
  clearTransfers,
  isCommandError,
  onTransfers,
  type TransferDto,
  type TransferState,
} from "../lib/api";

export interface TransferListProps {
  /** Estado externo opcional; sin él el componente se suscribe a `onTransfers`. */
  transfers?: readonly TransferDto[];
  onError?(message: string): void;
}

const STATE_LABELS: Record<TransferState, string> = {
  queued: "En cola",
  running: "En curso",
  done: "Completada",
  failed: "Fallida",
  cancelled: "Cancelada",
};

function errorText(cause: unknown): string {
  if (isCommandError(cause)) return cause.message;
  return cause instanceof Error ? cause.message : "No se pudo actualizar la transferencia";
}

/** Lista compacta de transferencias con progreso, cancelación y limpieza. */
export function TransferList({ transfers, onError }: TransferListProps): JSX.Element {
  const [internalJobs, setInternalJobs] = useState<readonly TransferDto[]>([]);
  const jobs = transfers ?? internalJobs;

  useEffect(() => {
    if (transfers !== undefined) return;
    let active = true;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unsubscribe = await onTransfers((next) => {
          if (active) setInternalJobs(next);
        });
        if (active) {
          stop = unsubscribe;
        } else {
          unsubscribe();
        }
      } catch {
        // Sin webview Tauri no hay eventos de transferencia.
      }
    })();
    return () => {
      active = false;
      stop?.();
    };
  }, [transfers]);

  function report(cause: unknown): void {
    onError?.(errorText(cause));
  }

  return (
    <section aria-label="Transferencias">
      <ul>
        {jobs.map((job) => (
          <li key={job.id}>
            <span>{job.remote}</span>
            <progress
              aria-label={`Progreso de ${job.remote}`}
              value={job.transferred}
              max={job.total ?? Math.max(job.transferred, 1)}
            />
            <span data-state={job.state}>{STATE_LABELS[job.state]}</span>
            <button type="button" onClick={() => void cancelTransfer(job.id).catch(report)}>
              Cancelar
            </button>
          </li>
        ))}
      </ul>
      <button type="button" onClick={() => void clearTransfers().catch(report)}>
        Limpiar terminadas
      </button>
    </section>
  );
}
