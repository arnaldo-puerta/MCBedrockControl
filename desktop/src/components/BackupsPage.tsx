import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type AppSnapshot, type BackupInfo, type BackupReason, type Backups, type ServerState } from "../lib/api";
import { formatBackupDate, formatBytes, PRIMARY_BUTTON, SMALL_BUTTON } from "../lib/format";
import { PageHeader } from "../lib/pages";

const REASONS: Record<BackupReason, string> = {
  manual: "Manual",
  beforeDelete: "Automática, antes de borrar",
  beforeRestore: "Automática, antes de restaurar",
  beforeUpdate: "Automática, antes de actualizar",
  beforeRename: "Automática, antes de cambiar el nombre",
};

interface Props {
  state: ServerState;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  adopt: (snapshot: AppSnapshot) => void;
}

export function BackupsPage({ state, run, adopt }: Props) {
  const [data, setData] = useState<Backups | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const stopped = state === "stopped" || state === "crashed";
  const transitioning = state === "starting" || state === "stopping";

  useEffect(() => {
    run(async () => setData(await api.backups()));
  }, [run]);

  async function task(label: string, action: () => Promise<unknown>) {
    setBusy(label);
    await run(action);
    setBusy(null);
  }

  async function restore(backup: BackupInfo) {
    const confirmed = await ask(
      `El mundo "${backup.world}" volverá a como estaba el ${formatBackupDate(backup.created)}. ` +
        "Antes se guardará una copia de cómo está ahora.",
      { title: "Restaurar copia", kind: "warning", okLabel: "Restaurar", cancelLabel: "Cancelar" },
    );
    if (!confirmed) return;
    await task("Restaurando…", async () => {
      adopt(await api.restoreBackup(backup.file));
      setData(await api.backups());
    });
  }

  async function remove(backup: BackupInfo) {
    const confirmed = await ask(`Se borrará la copia del ${formatBackupDate(backup.created)}. No se puede deshacer.`, {
      title: "Borrar copia",
      kind: "warning",
      okLabel: "Borrar",
      cancelLabel: "Cancelar",
    });
    if (confirmed) await task("Borrando…", async () => setData(await api.deleteBackup(backup.file)));
  }

  return (
    <section className="space-y-4 p-6">
      <PageHeader page="backups">
        {data && (
          <button onClick={() => revealItemInDir(data.dir)} className={SMALL_BUTTON}>
            Abrir carpeta
          </button>
        )}
        <button
          onClick={() => task("Guardando copia…", async () => setData(await api.createBackup()))}
          disabled={!!busy || transitioning}
          className={PRIMARY_BUTTON}
        >
          Crear copia ahora
        </button>
      </PageHeader>
      {state === "running" && (
        <p className="text-xs text-muted">Puedes copiar el mundo sin detener el servidor ni echar a nadie.</p>
      )}

      {busy && <p role="status" className="text-sm text-amber">{busy}</p>}

      <p className="text-xs text-muted">
        Se guardan solas antes de borrar un mundo, restaurar una copia o actualizar el servidor. Se conservan las 10 automáticas más
        recientes; las manuales, hasta que las borres. Cada copia es un .mcworld que también puedes abrir en
        Minecraft.
      </p>

      {data && data.backups.length === 0 && (
        <p className="rounded-xl border border-line bg-panel p-4 text-sm text-muted">Todavía no hay copias.</p>
      )}

      <ul className="space-y-2">
        {data?.backups.map((backup) => (
          <li key={backup.file} className="flex flex-wrap items-center gap-2 rounded-xl border border-line bg-panel p-4">
            <div className="mr-auto min-w-0">
              <p className="truncate font-semibold">
                {backup.world} · {formatBackupDate(backup.created)}
              </p>
              <p className="text-xs text-muted">
                {REASONS[backup.reason]} · {formatBytes(backup.sizeBytes)}
              </p>
            </div>
            <button
              onClick={() => restore(backup)}
              disabled={!stopped || !!busy}
              title={stopped ? undefined : "Detén el servidor para restaurar"}
              className={SMALL_BUTTON}
            >
              Restaurar
            </button>
            <button onClick={() => remove(backup)} disabled={!!busy} className={SMALL_BUTTON}>
              Borrar
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
