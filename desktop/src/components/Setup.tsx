import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type InstallProgress } from "../lib/api";

const EULA_URL = "https://aka.ms/MinecraftEULA";
const PRIVACY_URL = "https://go.microsoft.com/fwlink/?LinkId=521839";

function megabytes(bytes: number) {
  return (bytes / (1024 * 1024)).toFixed(0);
}

function progressLabel({ phase, done, total }: InstallProgress) {
  if (phase === "extract") return `Instalando archivos: ${done} de ${total}`;
  return total
    ? `Descargando: ${megabytes(done)} de ${megabytes(total)} MB`
    : `Descargando: ${megabytes(done)} MB`;
}

interface Props {
  error: string | null;
  onSelect: (path: string) => Promise<boolean>;
  onInstall: (path: string, eulaAccepted: boolean) => Promise<boolean>;
  onDone: () => void;
  onBack?: () => void;
}

export function Setup({ error, onSelect, onInstall, onDone, onBack }: Props) {
  const [path, setPath] = useState("");
  const [eulaAccepted, setEulaAccepted] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);

  useEffect(() => {
    api.defaultInstallPath().then(setPath);
    const unlisten = api.onInstallProgress(setProgress);
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  async function chooseInstallFolder() {
    const folder = await open({ directory: true, title: "Carpeta de instalación" });
    if (folder) setPath(folder);
  }

  async function useExisting() {
    const folder = await open({ directory: true, title: "Carpeta del servidor Bedrock" });
    if (folder && (await onSelect(folder))) onDone();
  }

  async function install() {
    setInstalling(true);
    setProgress(null);
    const installed = await onInstall(path, eulaAccepted);
    setInstalling(false);
    if (installed) onDone();
  }

  const percent = progress?.total ? (progress.done / progress.total) * 100 : 0;

  return (
    <main className="flex h-full items-center justify-center overflow-y-auto p-6">
      <div className="w-full max-w-lg space-y-5">
        <header className="flex items-center gap-4">
          <img src="/logo.png" alt="" className="size-14" />
          <div>
            <h1 className="text-2xl font-semibold">Instalar servidor</h1>
            <p className="text-sm text-muted">Descarga la última versión oficial de Bedrock.</p>
          </div>
        </header>

        <div className="space-y-4 rounded-xl border border-line bg-panel p-5">
          <div>
            <p className="text-xs text-muted">Se instalará en</p>
            <div className="mt-1 flex items-center gap-2">
              <p className="min-w-0 flex-1 truncate text-sm" title={path}>{path}</p>
              <button
                onClick={chooseInstallFolder}
                disabled={installing}
                className="rounded-lg bg-line px-3 py-1.5 text-sm hover:brightness-125 disabled:opacity-40"
              >
                Cambiar
              </button>
            </div>
          </div>

          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              checked={eulaAccepted}
              disabled={installing}
              onChange={(event) => setEulaAccepted(event.target.checked)}
              className="mt-0.5 accent-grass"
            />
            <span>
              Acepto el{" "}
              <button type="button" onClick={() => openUrl(EULA_URL)} className="text-grass-light underline">
                EULA de Minecraft
              </button>{" "}
              y la{" "}
              <button type="button" onClick={() => openUrl(PRIVACY_URL)} className="text-grass-light underline">
                Política de privacidad
              </button>
            </span>
          </label>

          {installing ? (
            <div className="space-y-2">
              <div
                role="progressbar"
                aria-valuenow={Math.round(percent)}
                className="h-2 overflow-hidden rounded-full bg-line"
              >
                <div className="h-full bg-grass-light" style={{ width: `${percent}%` }} />
              </div>
              <div className="flex items-center justify-between gap-3">
                <p role="status" className="text-sm text-muted">
                  {progress ? progressLabel(progress) : "Buscando la última versión…"}
                </p>
                <button onClick={api.cancelInstall} className="rounded-lg bg-line px-3 py-1.5 text-sm hover:brightness-125">
                  Cancelar
                </button>
              </div>
            </div>
          ) : (
            <button
              onClick={install}
              disabled={!eulaAccepted || !path}
              className="w-full rounded-lg bg-grass px-5 py-2.5 text-sm font-semibold hover:brightness-110 disabled:opacity-40"
            >
              Descargar e instalar
            </button>
          )}
        </div>

        {error && <p role="alert" className="text-sm text-danger">{error}</p>}

        {!installing && (
          <div className="flex items-center justify-between text-sm">
            <button onClick={useExisting} className="text-grass-light underline">
              Ya tengo un servidor
            </button>
            {onBack && (
              <button onClick={onBack} className="text-muted hover:text-ink">Volver</button>
            )}
          </div>
        )}
      </div>
    </main>
  );
}
