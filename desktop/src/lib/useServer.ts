import { useCallback, useEffect, useRef, useState } from "react";
import { api, type AppSnapshot, type LogLine, type ServerEvent } from "./api";

type LogListener = (lines: LogLine[]) => void;

/** Single source of server state. Logs bypass React: they go straight to the
 *  terminal, so a noisy server never re-renders the UI. */
export function useServer() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const listeners = useRef(new Set<LogListener>());
  const backlog = useRef<LogLine[]>([]);
  const lastSeq = useRef(-1);

  const publish = useCallback((lines: LogLine[]) => {
    // The listener attaches before the snapshot loads, so the two can overlap.
    const fresh = lines.filter((line) => line.seq > lastSeq.current);
    if (fresh.length === 0) return;
    lastSeq.current = fresh[fresh.length - 1].seq;
    backlog.current.push(...fresh);
    backlog.current = backlog.current.slice(-5000);
    listeners.current.forEach((listener) => listener(fresh));
  }, []);

  const adopt = useCallback(
    (next: AppSnapshot) => {
      setSnapshot(next);
      publish(next.server.history);
    },
    [publish],
  );

  useEffect(() => {
    const apply = (event: ServerEvent) => {
      if (event.kind === "logs") {
        publish(event.data);
        return;
      }
      setSnapshot((current) => {
        if (!current) return current;
        switch (event.kind) {
          case "state":
            return { ...current, server: { ...current.server, state: event.data } };
          case "players":
            return { ...current, server: { ...current.server, players: event.data } };
          case "stats":
            return { ...current, server: { ...current.server, stats: event.data } };
          case "version":
            return { ...current, version: event.data };
        }
      });
    };

    const unlisten = api.onEvent(apply);
    unlisten.then(() => api.snapshot()).then(adopt, (e) => setError(String(e)));
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [adopt, publish]);

  const run = useCallback(async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }, []);

  const subscribeLogs = useCallback((listener: LogListener) => {
    listener(backlog.current);
    listeners.current.add(listener);
    return () => {
      listeners.current.delete(listener);
    };
  }, []);

  const selectFolder = useCallback(
    (path: string) => run(async () => adopt(await api.setServerPath(path))),
    [adopt, run],
  );

  const install = useCallback(
    (path: string, eulaAccepted: boolean) =>
      run(async () => adopt(await api.installServer(path, eulaAccepted))),
    [adopt, run],
  );

  return { snapshot, error, run, selectFolder, install, subscribeLogs };
}
