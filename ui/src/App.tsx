import { useCallback, useEffect, useReducer, useRef, useState, type JSX } from "react";
import { QuickConnect } from "./connect/QuickConnect";
import {
  createSession as apiCreateSession,
  deleteFolder as apiDeleteFolder,
  deleteSession as apiDeleteSession,
  duplicateSession as apiDuplicateSession,
  exportDiagnostics as apiExportDiagnostics,
  isCommandError,
  listFolders,
  listSessions,
  saveFolder as apiSaveFolder,
  updateSession as apiUpdateSession,
  type FolderDto,
  type FolderInput,
  type PasswordInput,
  type SessionDto,
  type SessionInput,
} from "./lib/api";
import { SessionForm } from "./sessions/SessionForm";
import { SessionTree } from "./sessions/SessionTreeView";
import { sessionTarget } from "./sessions/sessionTree";
import { TabsView } from "./tabs/TabsView";
import { createTabController, type TabController } from "./tabs/tabController";
import { tabsReducer, type ConnectTarget, type Tab } from "./tabs/tabState";

const DEFAULT_SIZE = { rows: 24, cols: 80 };

type FormState = { kind: "create" } | { kind: "edit"; session: SessionDto };

function tabTitle(target: ConnectTarget): string {
  return `${target.username}@${target.host}`;
}

export default function App(): JSX.Element {
  const [tabs, dispatch] = useReducer(tabsReducer, [] as readonly Tab[]);
  const [activeTabId, setActiveTabId] = useState<number | null>(null);
  const [controllers, setControllers] = useState<ReadonlyMap<number, TabController>>(new Map());
  const controllersRef = useRef<Map<number, TabController>>(new Map());
  const nextIdRef = useRef(1);
  const [sessions, setSessions] = useState<readonly SessionDto[]>([]);
  const [folders, setFolders] = useState<readonly FolderDto[]>([]);
  const [formState, setFormState] = useState<FormState | null>(null);
  const [diagnosticResult, setDiagnosticResult] = useState<
    { kind: "ok"; path: string } | { kind: "error"; message: string } | null
  >(null);
  const pendingPasswordUpdates = useRef<Map<number, string>>(new Map());
  const [pendingUpdateVersion, setPendingUpdateVersion] = useState(0);

  const reload = useCallback(async (): Promise<void> => {
    try {
      const [nextSessions, nextFolders] = await Promise.all([listSessions(), listFolders()]);
      setSessions(nextSessions);
      setFolders(nextFolders);
    } catch {
      // Sin backend la barra lateral se muestra vacía, sin interrumpir las pestañas.
    }
  }, []);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- carga inicial desde el backend
    void reload();
  }, [reload]);

  useEffect(() => {
    const liveIds = new Set(tabs.map((t) => t.id));
    let changed = false;
    for (const id of controllersRef.current.keys()) {
      if (!liveIds.has(id)) {
        controllersRef.current.delete(id);
        changed = true;
      }
    }
    if (changed) {
      setControllers(new Map(controllersRef.current));
    }
  }, [tabs]);

  const handleUpdateSavedPassword = useCallback((sessionId: number, value: string): void => {
    pendingPasswordUpdates.current.set(sessionId, value);
    setPendingUpdateVersion((version) => version + 1);
  }, []);

  function handlePromptCancelled(id: number): void {
    dispatch({
      type: "failed",
      id,
      error: { kind: "auth_cancelled", message: "Autenticación cancelada" },
    });
  }

  useEffect(() => {
    // Solo se guarda la contraseña si la conexión con ella terminó conectada.
    for (const [sessionId, value] of pendingPasswordUpdates.current) {
      const tab = tabs.find((item) => item.target.sessionId === sessionId);
      if (tab === undefined || tab.status.kind === "connecting") {
        continue;
      }
      pendingPasswordUpdates.current.delete(sessionId);
      if (tab.status.kind !== "connected") {
        continue;
      }
      const session = sessions.find((item) => item.id === sessionId);
      if (session === undefined) {
        continue;
      }
      const input: SessionInput = {
        name: session.name,
        host: session.host,
        port: session.port,
        username: session.username,
        auth: session.auth,
        folder: session.folder,
      };
      void apiUpdateSession(sessionId, input, { kind: "set", value })
        .then(() => {
          void reload();
        })
        .catch(() => {
          // Si falla el guardado se conserva la contraseña anterior.
        });
    }
  }, [tabs, sessions, pendingUpdateVersion, reload]);

  function handleConnect(target: ConnectTarget): void {
    const id = nextIdRef.current;
    nextIdRef.current += 1;
    const tab: Tab = {
      id,
      title: tabTitle(target),
      target,
      status: { kind: "connecting" },
    };
    const controller = createTabController({ id, target }, dispatch);
    controllersRef.current.set(id, controller);
    setControllers(new Map(controllersRef.current));
    dispatch({ type: "opened", tab });
    setActiveTabId(id);
    void controller.connect({}, DEFAULT_SIZE);
  }

  async function handleSave(input: SessionInput, password: PasswordInput): Promise<void> {
    if (formState?.kind === "edit") {
      await apiUpdateSession(formState.session.id, input, password);
    } else {
      const secret =
        input.auth.kind === "password" && input.auth.remember && password.kind === "set"
          ? password.value
          : undefined;
      if (secret === undefined) {
        await apiCreateSession(input);
      } else {
        await apiCreateSession(input, secret);
      }
    }
    await reload();
    setFormState(null);
  }

  async function handleDuplicate(session: SessionDto): Promise<void> {
    try {
      await apiDuplicateSession(session.id);
      await reload();
    } catch {
      // Un fallo al duplicar deja la lista tal cual.
    }
  }

  async function handleDelete(session: SessionDto): Promise<void> {
    try {
      await apiDeleteSession(session.id);
      await reload();
    } catch {
      // Un fallo al borrar deja la lista tal cual.
    }
  }

  async function handleSaveFolder(input: FolderInput): Promise<void> {
    try {
      await apiSaveFolder(input);
      await reload();
    } catch {
      // Un fallo al guardar la carpeta deja la lista tal cual.
    }
  }

  async function handleDeleteFolder(id: number): Promise<void> {
    try {
      await apiDeleteFolder(id);
      await reload();
    } catch {
      // Un fallo al borrar la carpeta deja la lista tal cual.
    }
  }

  async function handleExportDiagnostics(): Promise<void> {
    setDiagnosticResult(null);
    try {
      const path = await apiExportDiagnostics();
      setDiagnosticResult({ kind: "ok", path });
    } catch (error) {
      const message = isCommandError(error)
        ? error.message
        : error instanceof Error
          ? error.message
          : "No se pudo exportar el diagnóstico";
      setDiagnosticResult({ kind: "error", message });
    }
  }

  const liveIds = new Set(tabs.map((t) => t.id));
  const effectiveActive =
    activeTabId !== null && liveIds.has(activeTabId) ? activeTabId : (tabs.at(-1)?.id ?? null);

  return (
    <main>
      <aside aria-label="Sesiones guardadas">
        <SessionTree
          folders={folders}
          sessions={sessions}
          onNewSession={() => setFormState({ kind: "create" })}
          onConnect={(session) => handleConnect(sessionTarget(session))}
          onEdit={(session) => setFormState({ kind: "edit", session })}
          onDuplicate={(session) => {
            void handleDuplicate(session);
          }}
          onDelete={(session) => {
            void handleDelete(session);
          }}
          onSaveFolder={(folder) => {
            void handleSaveFolder(folder);
          }}
          onDeleteFolder={(id) => {
            void handleDeleteFolder(id);
          }}
        />
        {formState !== null && (
          <SessionForm
            key={formState.kind === "edit" ? formState.session.id : "new"}
            initial={formState.kind === "edit" ? formState.session : undefined}
            folders={folders}
            onSubmit={handleSave}
            onCancel={() => setFormState(null)}
          />
        )}
      </aside>
      <div>
        <QuickConnect onSubmit={handleConnect} />
        <button
          type="button"
          onClick={() => {
            void handleExportDiagnostics();
          }}
        >
          Exportar diagnóstico
        </button>
        {diagnosticResult?.kind === "ok" && (
          <p role="status">{`Diagnóstico exportado: ${diagnosticResult.path}`}</p>
        )}
        {diagnosticResult?.kind === "error" && <p role="alert">{diagnosticResult.message}</p>}
      </div>
      <TabsView
        tabs={tabs}
        activeTabId={effectiveActive}
        onSelect={setActiveTabId}
        controllers={controllers}
        onPromptCancelled={handlePromptCancelled}
        onUpdateSavedPassword={handleUpdateSavedPassword}
      />
    </main>
  );
}
