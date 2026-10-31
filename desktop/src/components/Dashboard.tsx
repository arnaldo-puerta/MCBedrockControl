import type { AppSnapshot, ServerState } from "../lib/api";

const STATUS: Record<ServerState, { label: string; dot: string }> = {
  stopped: { label: "Detenido", dot: "bg-muted" },
  starting: { label: "Iniciando…", dot: "bg-amber animate-pulse" },
  running: { label: "En línea", dot: "bg-grass-light" },
  stopping: { label: "Guardando y deteniendo…", dot: "bg-amber animate-pulse" },
  crashed: { label: "Se detuvo por un error", dot: "bg-danger" },
};

function formatUptime(totalSecs: number) {
  const hours = Math.floor(totalSecs / 3600);
  const minutes = Math.floor((totalSecs % 3600) / 60);
  const seconds = totalSecs % 60;
  return [hours, minutes, seconds].map((n) => String(n).padStart(2, "0")).join(":");
}

function Card({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return (
    <div className="min-w-0 rounded-xl border border-line bg-panel p-4">
      <p className="text-xs text-muted">{label}</p>
      <p className="mt-1 truncate text-lg font-semibold" title={value}>{value}</p>
      {detail && <p className="mt-0.5 text-xs text-muted">{detail}</p>}
    </div>
  );
}

interface Props {
  snapshot: AppSnapshot;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onChangeServer: () => void;
}

const ACTION = "rounded-lg px-4 py-2.5 text-sm font-semibold hover:brightness-110 disabled:opacity-40";

export function Dashboard({ snapshot, onStart, onStop, onRestart, onChangeServer }: Props) {
  const { info, version, server } = snapshot;
  const { state, stats, players } = server;
  const status = STATUS[state];
  const live = state === "running";
  const idle = state === "stopped" || state === "crashed";

  return (
    <section className="space-y-4 p-6">
      <header className="flex items-start gap-4">
        <div className="min-w-0 flex-1">
          <h1 className="text-2xl font-semibold">{info.name || "Servidor"}</h1>
          <p className="mt-1 truncate text-sm text-muted">{snapshot.serverPath}</p>
        </div>
        <button
          onClick={onChangeServer}
          disabled={!idle}
          className="shrink-0 rounded-lg bg-line px-3 py-1.5 text-sm hover:brightness-125 disabled:opacity-40"
        >
          Cambiar o instalar
        </button>
      </header>

      <div className="flex flex-wrap items-center gap-3 rounded-xl border border-line bg-panel p-4">
        <span className={`size-3 rounded-full ${status.dot}`} />
        <span role="status" className="mr-auto font-semibold">{status.label}</span>
        <button onClick={onStart} disabled={!idle} className={`${ACTION} bg-grass`}>
          Iniciar
        </button>
        <button onClick={onRestart} disabled={!live} className={`${ACTION} bg-line`}>
          Reiniciar
        </button>
        <button onClick={onStop} disabled={!live} className={`${ACTION} bg-line`}>
          Detener
        </button>
      </div>

      {state === "crashed" && (
        <p role="alert" className="rounded-xl border border-danger/50 bg-danger/10 p-4 text-sm">
          El servidor se cerró inesperadamente. Revisa las últimas líneas en la consola.
        </p>
      )}

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Card label="Versión" value={version ?? "—"} />
        <Card label="Puerto" value={info.port || "—"} />
        <Card label="Mundo" value={info.world || "—"} />
        <Card label="Jugadores" value={`${players.length} / ${info.maxPlayers || "—"}`} />
        <Card label="CPU" value={`${stats.cpuPercent.toFixed(1)} %`} />
        <Card
          label="RAM"
          value={`${stats.memoryMb.toLocaleString("es")} MB`}
          detail={`${stats.memoryPercent.toFixed(1)} % del sistema`}
        />
        <Card label="Tiempo activo" value={idle ? "—" : formatUptime(stats.uptimeSecs)} />
      </div>

      <div className="rounded-xl border border-line bg-panel p-4">
        <h2 className="text-sm font-semibold">Jugadores conectados</h2>
        {players.length === 0 ? (
          <p className="mt-2 text-sm text-muted">No hay nadie conectado.</p>
        ) : (
          <ul className="mt-2 space-y-1 text-sm">
            {players.map((player) => <li key={player.name}>{player.name}</li>)}
          </ul>
        )}
      </div>
    </section>
  );
}
