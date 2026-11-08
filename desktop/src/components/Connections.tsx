import { useCallback, useEffect, useState, type ReactNode } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type Playit, type PlayitTunnel } from "../lib/api";
import { PRIMARY_BUTTON, SMALL_BUTTON } from "../lib/format";
import { PageHeader } from "../lib/pages";

const POLL_MS = 3000;

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }
  return (
    <button onClick={copy} className={PRIMARY_BUTTON}>
      {copied ? "¡Copiado!" : "Copiar"}
    </button>
  );
}

type StepState = "done" | "current" | "locked";

function Step({ n, title, state, children, doneExtra }: {
  n: number;
  title: string;
  state: StepState;
  children?: ReactNode;
  doneExtra?: ReactNode;
}) {
  const marker = {
    done: "bg-grass text-ink",
    current: "border-2 border-grass-light text-grass-light",
    locked: "border border-line text-muted",
  }[state];
  return (
    <li className={`flex gap-3 rounded-xl border border-line bg-panel p-4 ${state === "locked" ? "opacity-50" : ""}`}>
      <span className={`flex size-7 shrink-0 items-center justify-center rounded-full text-sm font-bold ${marker}`}>
        {state === "done" ? "✓" : n}
      </span>
      <div className="min-w-0 flex-1 space-y-2">
        <p className="font-semibold">{title}</p>
        {state === "current" && children}
        {state === "done" && doneExtra}
      </div>
    </li>
  );
}

function Note({ tone = "info", children }: { tone?: "info" | "warn"; children: ReactNode }) {
  const style = tone === "warn" ? "bg-amber/10 text-amber" : "bg-base text-muted";
  return <div className={`rounded-lg p-3 text-sm ${style}`}>{children}</div>;
}

/** `host:port` as playit shows it, split for Minecraft's two fields. */
function splitAddress(address: string) {
  const at = address.lastIndexOf(":");
  if (at <= 0) return { host: address, port: "19132" };
  return { host: address.slice(0, at), port: address.slice(at + 1) };
}

function pointsAt(tunnel: PlayitTunnel, port: string) {
  return tunnel.destination.endsWith(`:${port}`);
}

interface Props {
  run: (action: () => Promise<unknown>) => Promise<boolean>;
}

export function Connections({ run }: Props) {
  const [data, setData] = useState<Playit | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [linkUrl, setLinkUrl] = useState<string | null>(null);

  // Loads quietly: a failed check shows on this page, not as an app banner.
  const refresh = useCallback(async () => {
    try {
      setData(await api.playit());
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const status = data?.status;
  const agent = status?.agent ?? null;
  const settling = agent?.phase === "starting" || status?.service === "starting";

  // New tunnels appear here once created on the website, so keep looking
  // while the user might be there, and while playit finishes starting.
  const waitingForTunnel = agent?.phase === "running" && !agent.tunnels.some((t) => pointsAt(t, data!.port));
  useEffect(() => {
    if (!settling && !waitingForTunnel) return;
    const timer = setInterval(() => {
      if (!busy) refresh();
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [settling, waitingForTunnel, busy, refresh]);

  async function task(label: string, action: () => Promise<unknown>) {
    setBusy(label);
    await run(action);
    await refresh();
    setBusy(null);
  }

  async function link(replace: boolean) {
    if (replace) {
      const confirmed = await ask(
        "Se desvinculará la cuenta de Playit de este equipo para que entres con otra. " +
          "Tus túneles seguirán guardados en tu cuenta actual.",
        { title: "Usar otra cuenta", kind: "warning", okLabel: "Continuar", cancelLabel: "Cancelar" },
      );
      if (!confirmed) return;
    }
    setBusy("Preparando Playit…");
    await run(async () => {
      const { code, url } = await api.startPlayitLink(replace);
      setLinkUrl(url);
      await openUrl(url);
      setBusy("Esperando a que apruebes en el navegador…");
      await api.finishPlayitLink(code);
    });
    setLinkUrl(null);
    await refresh();
    setBusy(null);
  }

  async function openTunnels() {
    await openUrl(await api.playitTunnelsUrl());
  }

  if (!data || !status) {
    return (
      <section className="space-y-3 p-6">
        <PageHeader page="connections" />
        {loadError ? (
          <>
            <Note tone="warn">No se pudo revisar Playit: {loadError}</Note>
            <button onClick={refresh} className={SMALL_BUTTON}>Reintentar</button>
          </>
        ) : (
          <p role="status" className="text-sm text-muted">Revisando Playit…</p>
        )}
      </section>
    );
  }

  const { port } = data;
  const installed = status.installed && status.supportsNethernet;
  const serviceMissing = status.installed && status.service === "missing";
  const on = installed && status.service === "running";
  const linked = on && agent?.phase === "running";
  const ours = agent?.tunnels.filter((t) => pointsAt(t, port) && !t.disabledReason) ?? [];
  const others = agent?.tunnels.filter((t) => !ours.includes(t)) ?? [];
  const tunnel = linked ? ours[0] : undefined;
  const stateOf = (done: boolean, unlocked: boolean): StepState => (done ? "done" : unlocked ? "current" : "locked");

  return (
    <section className="space-y-4 p-6">
      <PageHeader page="connections">
        <button onClick={refresh} disabled={!!busy} className={SMALL_BUTTON}>
          Actualizar
        </button>
      </PageHeader>
      <p className="text-xs text-muted">Usa Playit: tus amigos no tienen que instalar nada.</p>

      {tunnel && (() => {
        const { host, port: publicPort } = splitAddress(tunnel.address);
        return (
          <div className="space-y-3 rounded-xl border border-grass bg-grass/10 p-5">
            <p className="text-sm text-grass-light">Comparte esto con tus amigos</p>
            <div className="flex items-center gap-3">
              <div className="min-w-0 flex-1">
                <p className="text-xs text-muted">Dirección</p>
                <p className="truncate font-mono text-xl font-semibold select-text">{host}</p>
              </div>
              <CopyButton text={host} />
            </div>
            <p className="text-sm">
              Puerto: <span className="font-mono font-semibold select-text">{publicPort}</span>
            </p>
            <p className="text-sm text-muted">En Minecraft: Jugar → Servidores → Agregar servidor.</p>
          </div>
        );
      })()}

      {busy && <p role="status" className="text-sm text-amber">{busy}</p>}

      {agent?.notices.map((notice) => (
        <Note key={notice.message} tone="warn">
          Playit dice: {notice.message}
          {notice.link && (
            <button onClick={() => openUrl(notice.link!)} className="ml-2 underline">Resolver</button>
          )}
        </Note>
      ))}

      <ol className="space-y-2">
        <Step
          n={1}
          title={status.installed && !status.supportsNethernet ? "Actualizar Playit" : "Instalar Playit"}
          state={stateOf(installed && !serviceMissing, true)}
          doneExtra={<p className="text-xs text-muted">Versión {status.version}</p>}
        >
          <p className="text-sm text-muted">
            {serviceMissing
              ? "Playit está a medio instalar. Si se está instalando ahora, espera un minuto y pulsa Actualizar; si no, reinstálalo."
              : status.installed
                ? `Tu versión ${status.version ?? ""} es antigua y no funciona con el Minecraft actual.`
                : "Es gratis. Windows te pedirá permiso para instalarlo."}
          </p>
          <button
            onClick={() => task("Instalando Playit… puede tardar un minuto.", api.installPlayit)}
            disabled={!!busy}
            className={PRIMARY_BUTTON}
          >
            {serviceMissing ? "Reinstalar Playit" : status.installed ? "Actualizar Playit" : "Instalar Playit"}
          </button>
        </Step>

        <Step n={2} title="Encender Playit" state={stateOf(on, installed && !serviceMissing)}>
          <p className="text-sm text-muted">Playit tiene que estar encendido para que tus amigos puedan entrar.</p>
          <button
            onClick={() => task("Encendiendo Playit…", () => api.setPlayitRunning(true))}
            disabled={!!busy || settling}
            className={PRIMARY_BUTTON}
          >
            {settling ? "Encendiendo…" : "Encender"}
          </button>
        </Step>

        <Step
          n={3}
          title="Tu cuenta de Playit"
          state={stateOf(linked, on)}
          doneExtra={
            <button onClick={() => link(true)} disabled={!!busy} className="text-xs text-muted underline">
              Usar otra cuenta
            </button>
          }
        >
          {settling ? (
            <p role="status" className="text-sm text-muted">Conectando con Playit…</p>
          ) : linkUrl ? (
            <Note>
              Se abrió Playit en tu navegador. Inicia sesión o crea tu cuenta y pulsa <b>Approve</b>; esta página
              seguirá sola.{" "}
              <button onClick={() => openUrl(linkUrl)} className="underline">Abrir el enlace otra vez</button>
            </Note>
          ) : (
            <>
              {agent?.phase === "invalidAccount" && (
                <Note tone="warn">La cuenta guardada en este equipo ya no funciona. Vincúlala de nuevo.</Note>
              )}
              {agent?.phase === "overLimit" && (
                <Note tone="warn">
                  Playit desactivó este equipo por superar el límite de tu cuenta.
                  {agent.error ? ` (${agent.error})` : ""}
                </Note>
              )}
              {agent?.phase === "error" && agent.error && <Note tone="warn">Playit tiene un problema: {agent.error}</Note>}
              <p className="text-sm text-muted">
                Se abrirá Playit en tu navegador para que inicies sesión o crees tu cuenta gratis.
              </p>
              <button onClick={() => link(agent?.phase === "invalidAccount")} disabled={!!busy} className={PRIMARY_BUTTON}>
                Vincular cuenta
              </button>
            </>
          )}
        </Step>

        <Step n={4} title="Tu dirección para amigos" state={stateOf(!!tunnel, linked)}>
          {agent?.emailVerified === false && (
            <Note tone="warn">
              Playit puede pedir que verifiques el correo de tu cuenta antes de crear túneles. Busca su correo en
              tu bandeja de entrada.
            </Note>
          )}
          <p className="text-sm text-muted">
            En Playit, pulsa <b>Create Tunnel</b>, elige <b>Minecraft Bedrock (NetherNet)</b> y en la dirección
            local pon <b>127.0.0.1</b> con el puerto <b>{port}</b>. La dirección aparecerá aquí sola.
          </p>
          {others.length > 0 && (
            <Note>
              Tienes {others.length === 1 ? "un túnel" : `${others.length} túneles`} que no apunta
              {others.length === 1 ? "" : "n"} a este servidor (puerto {port}):{" "}
              {others.map((t) => `${t.address} → ${t.destination}${t.disabledReason ? " (desactivado)" : ""}`).join(", ")}
            </Note>
          )}
          <button onClick={openTunnels} className={PRIMARY_BUTTON}>
            Abrir mis túneles en Playit
          </button>
        </Step>
      </ol>

      {tunnel && (
        <Note>
          Si tus amigos no pueden entrar, abre el túnel en Playit y revisa que sea del tipo{" "}
          <b>Minecraft Bedrock (NetherNet)</b>: los túneles antiguos de Bedrock no funcionan con el Minecraft actual.
        </Note>
      )}

      {on && (
        <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
          <span className="mr-auto">Mantén activada la lista de permitidos en Jugadores: la dirección es pública.</span>
          <button
            onClick={() => task("Apagando Playit…", () => api.setPlayitRunning(false))}
            disabled={!!busy}
            className={SMALL_BUTTON}
          >
            Apagar Playit
          </button>
        </div>
      )}
    </section>
  );
}
