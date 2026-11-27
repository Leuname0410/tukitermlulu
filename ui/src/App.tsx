import { useCallback, useEffect, useReducer, useRef, useState, type JSX } from "react";
import { QuickConnect } from "./connect/QuickConnect";
import {
  createSession as apiCreateSession,
  deleteFolder as apiDeleteFolder,
  deleteSession as apiDeleteSession,
  duplicateSession as apiDuplicateSession,
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
      <QuickConnect onSubmit={handleConnect} />
      <TabsView
        tabs={tabs}
        activeTabId={effectiveActive}
        onSelect={setActiveTabId}
        controllers={controllers}
      />
    </main>
  );
}
